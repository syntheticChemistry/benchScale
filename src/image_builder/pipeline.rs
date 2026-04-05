// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pipeline execution helpers for the image builder.
//!
//! Contains SSH utilities, cloud-init monitoring, VM lifecycle operations,
//! and all step execution logic.

use super::{BuildStep, ImageBuilder};
use crate::backend::NodeInfo;
use crate::{CloudInit, Error, Result};
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info, warn};
use virt::connect::Connect;
use virt::domain::Domain;

fn parse_vnc_from_domain_xml(xml: &str) -> Option<String> {
    if let Some(idx) = xml.find("type='vnc'").or_else(|| xml.find("type=\"vnc\"")) {
        let slice = xml[idx..].chars().take(512).collect::<String>();
        for prefix in ["port='", "port=\""] {
            if let Some(p) = slice.find(prefix) {
                let rest = &slice[p + prefix.len()..];
                if let Some(end) = rest.find('\'').or_else(|| rest.find('"'))
                    && let Ok(num) = rest[..end].parse::<i32>()
                    && num > 0
                {
                    return Some(format!("localhost:{}", num));
                }
            }
        }
    }
    for line in xml.lines() {
        if line.contains("graphics") && line.contains("vnc") && line.contains("port=") {
            if let Some(port_start) = line.find("port='") {
                let port_str = &line[port_start + 6..];
                if let Some(port_end) = port_str.find('\'')
                    && let Ok(port) = port_str[..port_end].parse::<i32>()
                    && port > 0
                {
                    return Some(format!("localhost:{}", port));
                }
            }
            if let Some(port_start) = line.find("port=\"") {
                let port_str = &line[port_start + 6..];
                if let Some(port_end) = port_str.find('"')
                    && let Ok(port) = port_str[..port_end].parse::<i32>()
                    && port > 0
                {
                    return Some(format!("localhost:{}", port));
                }
            }
        }
    }
    None
}

/// Detect SSH user for a VM (tries common usernames)
pub(super) async fn detect_ssh_user(ip: &str) -> Result<String> {
    let common_users = vec!["ubuntu", "desktop", "builder", "admin"];

    for user in common_users {
        debug!("Trying SSH user: {}", user);

        let result = tokio::process::Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "ConnectTimeout=3",
                "-o",
                "BatchMode=yes",
                &format!("{}@{}", user, ip),
                "echo",
                "connected",
            ])
            .output()
            .await;

        if let Ok(output) = result
            && output.status.success()
        {
            info!("Detected SSH user: {}", user);
            return Ok(user.to_string());
        }
    }

    Err(Error::Backend("Could not detect SSH user".to_string()))
}

/// Get actual IP address from virsh (not the allocated one)
pub(super) async fn get_actual_vm_ip(vm_name: &str) -> Result<String> {
    let vm = vm_name.to_string();
    let ip = tokio::task::spawn_blocking(move || {
        let conn = Connect::open(Some("qemu:///system"))
            .map_err(|e| Error::Backend(format!("Failed to get VM IP: {}", e)))?;
        let domain = Domain::lookup_by_name(&conn, &vm)
            .map_err(|_| Error::Backend("Failed to get VM IP".to_string()))?;
        let ifaces = domain
            .interface_addresses(virt::sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE, 0)
            .map_err(|_| Error::Backend("Failed to get VM IP".to_string()))?;
        for iface in ifaces {
            for addr in &iface.addrs {
                if addr.typed == i64::from(virt::sys::VIR_IP_ADDR_TYPE_IPV4) {
                    return Ok(addr.addr.clone());
                }
            }
        }
        Err(Error::Backend("Could not parse VM IP".to_string()))
    })
    .await
    .map_err(|e| Error::Backend(format!("Failed to get VM IP: {}", e)))??;

    info!("Detected actual VM IP: {}", ip);
    Ok(ip)
}

/// Wait for SSH with retries
pub(super) async fn wait_for_ssh(ip: &str, user: &str, max_attempts: u32) -> Result<()> {
    info!(
        "Waiting for SSH ({}@{}, max {} attempts)...",
        user, ip, max_attempts
    );

    for attempt in 1..=max_attempts {
        let result = tokio::process::Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "ConnectTimeout=3",
                "-o",
                "BatchMode=yes",
                &format!("{}@{}", user, ip),
                "echo",
                "ready",
            ])
            .output()
            .await;

        if let Ok(output) = result
            && output.status.success()
        {
            info!("SSH ready after {} attempts", attempt);
            return Ok(());
        }

        if attempt < max_attempts {
            debug!(
                "SSH attempt {}/{} failed, retrying...",
                attempt, max_attempts
            );
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    Err(Error::Backend(format!(
        "SSH not ready after {} attempts",
        max_attempts
    )))
}

impl ImageBuilder {
    /// Create builder VM
    pub(super) async fn create_builder_vm(
        &self,
        name: &str,
        base_image: &Path,
    ) -> Result<NodeInfo> {
        let cloud_init = self
            .cloud_init
            .clone()
            .unwrap_or_else(|| CloudInit::builder().add_user("builder", "").build());

        self.backend
            .create_desktop_vm(
                name,
                base_image,
                &cloud_init,
                self.memory_mb,
                self.vcpus,
                self.disk_size_gb,
            )
            .await
    }

    /// Execute a build step
    pub(super) async fn execute_step(&self, node: &NodeInfo, step: &BuildStep) -> Result<()> {
        match step {
            BuildStep::WaitForCloudInit => {
                info!("Waiting for cloud-init to complete...");
                self.wait_for_cloud_init_complete(node).await?;
            }
            BuildStep::InstallPackages(packages) => {
                info!("Installing packages: {}", packages.join(", "));
                self.install_packages(node, packages).await?;
            }
            BuildStep::RunCommands(commands) => {
                info!("Running {} commands", commands.len());
                for cmd in commands {
                    self.run_ssh_command(node, cmd).await?;
                }
            }
            BuildStep::UserVerification { message, vnc_port } => {
                info!("Pausing for user verification...");
                self.pause_for_verification(&node.name, message, *vnc_port)
                    .await?;
            }
            BuildStep::SaveIntermediate { name, path } => {
                info!("Saving intermediate state: {}", name);
                self.save_intermediate(&node.name, path).await?;
            }
            BuildStep::Reboot => {
                info!("Rebooting VM...");
                self.reboot_vm(node).await?;
            }
        }

        Ok(())
    }

    /// Execute step with known user
    pub(super) async fn execute_step_with_user(
        &self,
        node: &NodeInfo,
        user: &str,
        step: &BuildStep,
    ) -> Result<()> {
        match step {
            BuildStep::WaitForCloudInit => {
                self.wait_for_cloud_init_with_user(node, user).await?;
            }
            BuildStep::InstallPackages(packages) => {
                self.install_packages_with_user(node, user, packages)
                    .await?;
            }
            BuildStep::RunCommands(commands) => {
                for cmd in commands {
                    self.run_ssh_command_with_user(node, user, cmd).await?;
                }
            }
            BuildStep::UserVerification { message, vnc_port } => {
                self.pause_for_verification(&node.name, message, *vnc_port)
                    .await?;
            }
            BuildStep::SaveIntermediate { name: _, path } => {
                self.save_intermediate(&node.name, path).await?;
            }
            BuildStep::Reboot => {
                self.reboot_vm_with_user(node, user).await?;
            }
        }

        Ok(())
    }

    async fn wait_for_cloud_init_complete(&self, node: &NodeInfo) -> Result<()> {
        info!("Waiting for cloud-init to finish (this handles apt locks)...");

        let timeout = Duration::from_secs(600);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(Error::Backend("Timeout waiting for cloud-init".to_string()));
            }

            let output = Self::run_ssh_command_silent(
                node,
                "cloud-init status --wait --long || echo 'TIMEOUT'",
            )
            .await;

            if let Ok(status) = output {
                if status.contains("status: done") {
                    info!("Cloud-init completed successfully");
                    return Ok(());
                } else if status.contains("TIMEOUT") {
                    warn!("Cloud-init status command timed out, checking manually...");
                }
            }

            let lock_check = Self::run_ssh_command_silent(
                node,
                "sudo fuser /var/lib/dpkg/lock-frontend 2>/dev/null || echo 'FREE'",
            )
            .await;

            if let Ok(lock_status) = lock_check
                && lock_status.trim() == "FREE"
            {
                info!("apt lock is free, cloud-init likely done");
                tokio::time::sleep(Duration::from_secs(5)).await;
                return Ok(());
            }

            debug!("Still waiting for cloud-init/apt...");
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    async fn wait_for_cloud_init_with_user(&self, node: &NodeInfo, user: &str) -> Result<()> {
        let timeout = Duration::from_secs(600);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(Error::Backend("Timeout waiting for cloud-init".to_string()));
            }

            let lock_check = self
                .run_ssh_command_with_user_silent(
                    node,
                    user,
                    "sudo fuser /var/lib/dpkg/lock-frontend 2>/dev/null || echo 'FREE'",
                )
                .await;

            if let Ok(lock_status) = lock_check
                && lock_status.trim() == "FREE"
            {
                info!("Cloud-init/apt ready");
                return Ok(());
            }

            debug!("Waiting for cloud-init/apt...");
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    async fn install_packages(&self, node: &NodeInfo, packages: &[String]) -> Result<()> {
        let packages_str = packages.join(" ");
        let cmd = format!(
            "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y {}",
            packages_str
        );
        self.run_ssh_command(node, &cmd).await
    }

    async fn install_packages_with_user(
        &self,
        node: &NodeInfo,
        user: &str,
        packages: &[String],
    ) -> Result<()> {
        let packages_str = packages.join(" ");
        let cmd = format!(
            "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y {}",
            packages_str
        );
        self.run_ssh_command_with_user(node, user, &cmd).await
    }

    async fn run_ssh_command(&self, node: &NodeInfo, command: &str) -> Result<()> {
        info!("  Running: {}", command);

        let output = tokio::process::Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                &format!("builder@{}", node.ip_address),
                command,
            ])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("SSH command failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Backend(format!("Command failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("  Output: {}", stdout.trim());

        Ok(())
    }

    async fn run_ssh_command_with_user(
        &self,
        node: &NodeInfo,
        user: &str,
        command: &str,
    ) -> Result<()> {
        let output = tokio::process::Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                &format!("{}@{}", user, node.ip_address),
                command,
            ])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("SSH command failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Backend(format!("Command failed: {}", stderr)));
        }

        Ok(())
    }

    fn run_ssh_command_silent(
        node: &NodeInfo,
        command: &str,
    ) -> impl std::future::Future<Output = Result<String>> + 'static {
        let ip = node.ip_address.clone();
        let cmd = command.to_string();
        async move {
            let output = tokio::process::Command::new("ssh")
                .args([
                    "-o",
                    "StrictHostKeyChecking=no",
                    "-o",
                    "UserKnownHostsFile=/dev/null",
                    "-o",
                    "ConnectTimeout=5",
                    &format!("builder@{}", ip),
                    &cmd,
                ])
                .output()
                .await
                .map_err(|e| Error::Backend(format!("SSH command failed: {}", e)))?;

            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }
    }

    async fn run_ssh_command_with_user_silent(
        &self,
        node: &NodeInfo,
        user: &str,
        command: &str,
    ) -> Result<String> {
        let output = tokio::process::Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "ConnectTimeout=5",
                &format!("{}@{}", user, node.ip_address),
                command,
            ])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("SSH command failed: {}", e)))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn reboot_vm(&self, node: &NodeInfo) -> Result<()> {
        self.run_ssh_command(node, "sudo reboot").await?;

        info!("Waiting for VM to reboot...");
        tokio::time::sleep(Duration::from_secs(30)).await;

        for _ in 0..30 {
            if Self::run_ssh_command_silent(node, "echo 'ready'")
                .await
                .is_ok()
            {
                info!("VM rebooted successfully");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        Err(Error::Backend("VM failed to reboot".to_string()))
    }

    async fn reboot_vm_with_user(&self, node: &NodeInfo, user: &str) -> Result<()> {
        self.run_ssh_command_with_user(node, user, "sudo reboot")
            .await?;

        info!("Waiting for VM to reboot...");
        tokio::time::sleep(Duration::from_secs(30)).await;

        wait_for_ssh(&node.ip_address, user, 30).await?;

        info!("VM rebooted successfully");
        Ok(())
    }

    async fn pause_for_verification(
        &self,
        vm_name: &str,
        message: &str,
        vnc_port: Option<u16>,
    ) -> Result<()> {
        let vnc_display = if let Some(port) = vnc_port {
            format!("localhost:{}", port)
        } else {
            Self::get_vnc_display(vm_name)?
        };

        println!("\n╔══════════════════════════════════════════════════════════════════════════╗");
        println!("║  USER VERIFICATION REQUIRED                                              ║");
        println!("╚══════════════════════════════════════════════════════════════════════════╝");
        println!();
        println!("{}", message);
        println!();
        println!("VNC: vncviewer {}", vnc_display);
        println!();
        println!("Press ENTER when ready to continue...");

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| Error::Backend(format!("Failed to read input: {}", e)))?;

        info!("User verification complete, continuing build...");
        Ok(())
    }

    async fn save_intermediate(&self, vm_name: &str, path: &Path) -> Result<()> {
        info!("Shutting down VM for intermediate save...");

        let vm_shutdown = vm_name.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(conn) = Connect::open(Some("qemu:///system"))
                && let Ok(domain) = Domain::lookup_by_name(&conn, &vm_shutdown)
            {
                let _ = domain.shutdown();
            }
        })
        .await;

        tokio::time::sleep(Duration::from_secs(30)).await;

        let images_dir = crate::constants::paths::libvirt_images_dir();
        let disk_path = images_dir.join(format!("{}.qcow2", vm_name));

        let path_str = path
            .to_str()
            .ok_or_else(|| Error::Backend("Invalid UTF-8 in path".to_string()))?;

        info!(
            "Copying VM disk from {} to {}",
            disk_path.display(),
            path_str
        );

        tokio::fs::copy(&disk_path, path_str)
            .await
            .map_err(|e| Error::Backend(format!("Failed to save intermediate: {}", e)))?;

        info!("Intermediate state saved to: {}", path.display());

        let vm_start = vm_name.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = Connect::open(Some("qemu:///system"))
                .map_err(|e| Error::Backend(format!("Failed to restart VM: {}", e)))?;
            let domain = Domain::lookup_by_name(&conn, &vm_start)
                .map_err(|e| Error::Backend(format!("Failed to restart VM: {}", e)))?;
            domain
                .create()
                .map_err(|e| Error::Backend(format!("Failed to restart VM: {}", e)))?;
            Ok::<(), Error>(())
        })
        .await
        .map_err(|e| Error::Backend(format!("Failed to restart VM: {}", e)))??;

        tokio::time::sleep(Duration::from_secs(10)).await;

        Ok(())
    }

    pub(super) fn get_vnc_display(vm_name: &str) -> Result<String> {
        let xml_opt = (|| {
            let conn = Connect::open(Some("qemu:///system")).ok()?;
            let domain = Domain::lookup_by_name(&conn, vm_name).ok()?;
            domain.get_xml_desc(0).ok()
        })();

        if let Some(xml) = xml_opt {
            if let Some(display) = parse_vnc_from_domain_xml(&xml) {
                return Ok(display);
            }
            debug!("Trying to parse VM XML for VNC port...");
        }

        warn!("Could not detect VNC display, VM may not have graphics enabled");
        Ok("(VNC not available)".to_string())
    }

    pub(super) async fn save_as_template(&self, vm_name: &str) -> Result<PathBuf> {
        info!("Shutting down VM to save as template...");

        let vm_shutdown = vm_name.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(conn) = Connect::open(Some("qemu:///system"))
                && let Ok(domain) = Domain::lookup_by_name(&conn, &vm_shutdown)
            {
                let _ = domain.shutdown();
            }
        })
        .await;

        tokio::time::sleep(Duration::from_secs(30)).await;

        let images_dir = crate::constants::paths::libvirt_images_dir();
        let disk_path = images_dir.join(format!("{}.qcow2", vm_name));
        let template_path = images_dir.join(format!("{}-template.qcow2", self.name));

        info!("Optimizing template...");

        let sparsify_result = tokio::process::Command::new("which")
            .arg("virt-sparsify")
            .output()
            .await;

        match sparsify_result {
            Ok(output) if output.status.success() => {
                info!("Running virt-sparsify to optimize disk...");
                tokio::process::Command::new("virt-sparsify")
                    .args([
                        "--in-place",
                        disk_path.to_str().ok_or_else(|| {
                            Error::Backend("VM disk path is not valid UTF-8".to_string())
                        })?,
                    ])
                    .output()
                    .await
                    .map_err(|e| Error::Backend(format!("Failed to sparsify: {}", e)))?;
            }
            _ => {
                info!("virt-sparsify not available, skipping optimization");
            }
        }

        info!("Copying to template location...");
        tokio::fs::copy(&disk_path, &template_path)
            .await
            .map_err(|e| Error::Backend(format!("Failed to copy template: {}", e)))?;

        // Ensure template is group-readable (libvirt group can access).
        // No sudo needed when the pool dir is group-writable and the user is in libvirt.
        if let Err(e) = std::fs::set_permissions(
            &template_path,
            std::os::unix::fs::PermissionsExt::from_mode(0o644),
        ) {
            warn!("Could not set template permissions: {}", e);
        }

        info!("Template saved: {}", template_path.display());

        Ok(template_path)
    }
}
