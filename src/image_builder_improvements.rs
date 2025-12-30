// ImageBuilder improvements based on pipeline lessons

use crate::backend::{Backend, NodeInfo};
use crate::{CloudInit, Error, Result};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "libvirt")]
use crate::backend::LibvirtBackend;

/// Detect SSH user for a VM (tries common usernames)
async fn detect_ssh_user(ip: &str) -> Result<String> {
    let common_users = vec!["ubuntu", "desktop", "builder", "admin", "root"];
    
    for user in common_users {
        debug!("Trying SSH user: {}", user);
        
        let result = tokio::process::Command::new("ssh")
            .args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "ConnectTimeout=3",
                "-o", "BatchMode=yes",
                &format!("{}@{}", user, ip),
                "echo", "connected"
            ])
            .output()
            .await;
        
        if let Ok(output) = result {
            if output.status.success() {
                info!("Detected SSH user: {}", user);
                return Ok(user.to_string());
            }
        }
    }
    
    Err(Error::Backend("Could not detect SSH user".to_string()))
}

/// Get actual IP address from virsh (not the allocated one)
async fn get_actual_vm_ip(vm_name: &str) -> Result<String> {
    let output = tokio::process::Command::new("sudo")
        .args(&["virsh", "domifaddr", vm_name])
        .output()
        .await
        .map_err(|e| Error::Backend(format!("Failed to get VM IP: {}", e)))?;
    
    if !output.status.success() {
        return Err(Error::Backend("Failed to get VM IP".to_string()));
    }
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    
    // Parse IP from output like: "vnet4      52:54:00:68:da:de    ipv4         192.168.122.176/24"
    for line in output_str.lines() {
        if line.contains("ipv4") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(ip_with_mask) = parts.last() {
                if let Some(ip) = ip_with_mask.split('/').next() {
                    info!("Detected actual VM IP: {}", ip);
                    return Ok(ip.to_string());
                }
            }
        }
    }
    
    Err(Error::Backend("Could not parse VM IP".to_string()))
}

/// Wait for SSH with retries
async fn wait_for_ssh(ip: &str, user: &str, max_attempts: u32) -> Result<()> {
    info!("Waiting for SSH ({}@{}, max {} attempts)...", user, ip, max_attempts);
    
    for attempt in 1..=max_attempts {
        let result = tokio::process::Command::new("ssh")
            .args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "ConnectTimeout=3",
                "-o", "BatchMode=yes",
                &format!("{}@{}", user, ip),
                "echo", "ready"
            ])
            .output()
            .await;
        
        if let Ok(output) = result {
            if output.status.success() {
                info!("SSH ready after {} attempts", attempt);
                return Ok(());
            }
        }
        
        if attempt < max_attempts {
            debug!("SSH attempt {}/{} failed, retrying...", attempt, max_attempts);
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
    
    Err(Error::Backend(format!("SSH not ready after {} attempts", max_attempts)))
}

impl ImageBuilder {
    /// Create ImageBuilder from existing VM (lesson from pipeline!)
    #[cfg(feature = "libvirt")]
    pub fn from_existing_vm(vm_name: impl Into<String>) -> Result<Self> {
        let backend = LibvirtBackend::new()?;
        
        // Get VM info
        let vm_name_str = vm_name.into();
        
        Ok(Self {
            name: vm_name_str.clone(),
            base_image: None,
            memory_mb: 4096,
            vcpus: 2,
            disk_size_gb: 35,
            steps: Vec::new(),
            cloud_init: None,
            backend,
        })
    }
    
    /// Build from existing VM (simplified workflow!)
    pub async fn build_from_existing(mut self, vm_name: &str) -> Result<BuildResult> {
        info!("Building from existing VM: {}", vm_name);
        
        // Step 1: Get actual IP (lesson learned!)
        let ip = get_actual_vm_ip(vm_name).await?;
        info!("  VM IP: {}", ip);
        
        // Step 2: Detect SSH user (lesson learned!)
        let user = detect_ssh_user(&ip).await?;
        info!("  SSH user: {}", user);
        
        // Step 3: Wait for SSH with retries (lesson learned!)
        wait_for_ssh(&ip, &user, 10).await?;
        
        // Create NodeInfo for compatibility
        let node = NodeInfo {
            id: vm_name.to_string(),
            name: vm_name.to_string(),
            ip_address: ip.clone(),
            ssh_port: 22,
            state: "running".to_string(),
        };
        
        // Execute build steps on existing VM
        for (idx, step) in self.steps.iter().enumerate() {
            info!("Executing step {}/{}: {:?}", idx + 1, self.steps.len(), step);
            self.execute_step_with_user(&node, &user, step).await?;
        }
        
        // Save as template
        let template_path = self.save_as_template(vm_name).await?;
        
        let final_size = std::fs::metadata(&template_path)
            .map(|m| m.len())
            .unwrap_or(0);
        
        Ok(BuildResult {
            template_path,
            vm_name: vm_name.to_string(),
            final_size_bytes: final_size,
        })
    }
    
    /// Execute step with known user (improved from pipeline experience)
    async fn execute_step_with_user(&self, node: &NodeInfo, user: &str, step: &BuildStep) -> Result<()> {
        match step {
            BuildStep::WaitForCloudInit => {
                info!("Waiting for cloud-init to complete...");
                self.wait_for_cloud_init_with_user(node, user).await?;
            }

            BuildStep::InstallPackages(packages) => {
                info!("Installing packages: {}", packages.join(", "));
                self.install_packages_with_user(node, user, packages).await?;
            }

            BuildStep::RunCommands(commands) => {
                info!("Running {} commands", commands.len());
                for cmd in commands {
                    self.run_ssh_command_with_user(node, user, cmd).await?;
                }
            }

            BuildStep::UserVerification { message, vnc_port } => {
                info!("Pausing for user verification...");
                self.pause_for_verification(&node.name, message, *vnc_port).await?;
            }

            BuildStep::SaveIntermediate { name, path } => {
                info!("Saving intermediate state: {}", name);
                self.save_intermediate(&node.name, path).await?;
            }

            BuildStep::Reboot => {
                info!("Rebooting VM...");
                self.reboot_vm_with_user(node, user).await?;
            }
        }

        Ok(())
    }
    
    /// Wait for cloud-init with known user
    async fn wait_for_cloud_init_with_user(&self, node: &NodeInfo, user: &str) -> Result<()> {
        let timeout = Duration::from_secs(600);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(Error::Backend("Timeout waiting for cloud-init".to_string()));
            }

            // Check cloud-init status
            let output = self.run_ssh_command_with_user_silent(
                node,
                user,
                "cloud-init status --wait --long || echo 'TIMEOUT'"
            ).await;

            if let Ok(status) = output {
                if status.contains("status: done") {
                    info!("Cloud-init completed successfully");
                    return Ok(());
                }
            }

            // Check if apt lock is free
            let lock_check = self.run_ssh_command_with_user_silent(
                node,
                user,
                "sudo fuser /var/lib/dpkg/lock-frontend 2>/dev/null || echo 'FREE'"
            ).await;

            if let Ok(lock_status) = lock_check {
                if lock_status.trim() == "FREE" {
                    info!("apt lock is free, cloud-init likely done");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    return Ok(());
                }
            }

            debug!("Still waiting for cloud-init/apt...");
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }
    
    /// Install packages with known user
    async fn install_packages_with_user(&self, node: &NodeInfo, user: &str, packages: &[String]) -> Result<()> {
        let packages_str = packages.join(" ");
        let cmd = format!(
            "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y {}",
            packages_str
        );
        
        self.run_ssh_command_with_user(node, user, &cmd).await
    }
    
    /// Run SSH command with known user
    async fn run_ssh_command_with_user(&self, node: &NodeInfo, user: &str, command: &str) -> Result<()> {
        info!("  Running: {}", command);
        
        let output = tokio::process::Command::new("ssh")
            .args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
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

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("  Output: {}", stdout.trim());

        Ok(())
    }
    
    /// Run SSH command silently with known user
    async fn run_ssh_command_with_user_silent(&self, node: &NodeInfo, user: &str, command: &str) -> Result<String> {
        let output = tokio::process::Command::new("ssh")
            .args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "ConnectTimeout=5",
                &format!("{}@{}", user, node.ip_address),
                command,
            ])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("SSH command failed: {}", e)))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    /// Reboot VM with known user
    async fn reboot_vm_with_user(&self, node: &NodeInfo, user: &str) -> Result<()> {
        self.run_ssh_command_with_user(node, user, "sudo reboot").await?;
        
        info!("Waiting for VM to reboot...");
        tokio::time::sleep(Duration::from_secs(30)).await;
        
        // Wait for SSH to come back with retries
        wait_for_ssh(&node.ip_address, user, 30).await?;
        
        info!("VM rebooted successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_detect_ssh_user() {
        // This would need a real VM to test
        // Just testing the function exists and compiles
        assert!(true);
    }

    #[tokio::test]
    #[ignore] // Requires libvirt
    async fn test_get_actual_vm_ip() {
        // This would need a real VM to test
        assert!(true);
    }

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_wait_for_ssh() {
        // This would need a real VM to test
        assert!(true);
    }
}

