//! Image Builder for benchScale
//!
//! Provides high-level API for building VM templates with proper monitoring,
//! user interaction, and state management.
//!
//! # Example
//!
//! ```no_run
//! use benchscale::image_builder::{ImageBuilder, BuildStep};
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let builder = ImageBuilder::new("popos-cosmic")?;
//!
//! // Build with user interaction
//! let template = builder
//!     .from_cloud_image(Path::new("ubuntu-24.04.img"))
//!     .with_memory(4096)
//!     .with_vcpus(2)
//!     .add_step(BuildStep::InstallPackages(vec!["cosmic-desktop".to_string()]))
//!     .add_step(BuildStep::UserVerification {
//!         message: "Check VNC - is COSMIC running?".to_string(),
//!         vnc_port: None, // Auto-detect
//!     })
//!     .build()
//!     .await?;
//!
//! println!("Template saved to: {}", template.display());
//! # Ok(())
//! # }
//! ```

use crate::backend::{Backend, NodeInfo};
use crate::{CloudInit, Error, Result};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "libvirt")]
use crate::backend::LibvirtBackend;

/// Build step for template creation
#[derive(Debug, Clone)]
pub enum BuildStep {
    /// Install packages via apt
    InstallPackages(Vec<String>),
    
    /// Run arbitrary shell commands
    RunCommands(Vec<String>),
    
    /// Wait for cloud-init to complete (handles apt locks)
    WaitForCloudInit,
    
    /// Pause for user verification (GUI check, etc.)
    UserVerification {
        message: String,
        vnc_port: Option<u16>,
    },
    
    /// Save intermediate state
    SaveIntermediate {
        name: String,
        path: PathBuf,
    },
    
    /// Reboot VM
    Reboot,
}

/// Image builder for creating VM templates
pub struct ImageBuilder {
    name: String,
    base_image: Option<PathBuf>,
    memory_mb: u32,
    vcpus: u32,
    disk_size_gb: u32,
    steps: Vec<BuildStep>,
    cloud_init: Option<CloudInit>,
    #[cfg(feature = "libvirt")]
    backend: LibvirtBackend,
}

/// Build result containing template path and metadata
#[derive(Debug)]
pub struct BuildResult {
    pub template_path: PathBuf,
    pub vm_name: String,
    pub final_size_bytes: u64,
}

impl ImageBuilder {
    /// Create a new image builder
    #[cfg(feature = "libvirt")]
    pub fn new(name: impl Into<String>) -> Result<Self> {
        Ok(Self {
            name: name.into(),
            base_image: None,
            memory_mb: 4096,
            vcpus: 2,
            disk_size_gb: 35,
            steps: Vec::new(),
            cloud_init: None,
            backend: LibvirtBackend::new()?,
        })
    }

    /// Set base cloud image
    pub fn from_cloud_image(mut self, path: impl Into<PathBuf>) -> Self {
        self.base_image = Some(path.into());
        self
    }

    /// Set memory in MB
    pub fn with_memory(mut self, memory_mb: u32) -> Self {
        self.memory_mb = memory_mb;
        self
    }

    /// Set vCPUs
    pub fn with_vcpus(mut self, vcpus: u32) -> Self {
        self.vcpus = vcpus;
        self
    }

    /// Set disk size in GB
    pub fn with_disk_size(mut self, disk_size_gb: u32) -> Self {
        self.disk_size_gb = disk_size_gb;
        self
    }

    /// Set cloud-init configuration
    pub fn with_cloud_init(mut self, cloud_init: CloudInit) -> Self {
        self.cloud_init = Some(cloud_init);
        self
    }

    /// Add a build step
    pub fn add_step(mut self, step: BuildStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Build the template
    pub async fn build(mut self) -> Result<BuildResult> {
        let base_image = self.base_image.take()
            .ok_or_else(|| Error::Backend("No base image specified".to_string()))?;

        if !base_image.exists() {
            return Err(Error::Backend(format!(
                "Base image not found: {}",
                base_image.display()
            )));
        }

        let vm_name = format!("{}-builder-{}", self.name, chrono::Utc::now().format("%Y%m%d-%H%M%S"));
        
        info!("Starting image build: {}", vm_name);
        info!("  Base image: {}", base_image.display());
        info!("  Memory: {}MB, vCPUs: {}", self.memory_mb, self.vcpus);
        info!("  Build steps: {}", self.steps.len());

        // Create builder VM
        let node = self.create_builder_vm(&vm_name, &base_image).await?;
        
        info!("Builder VM created: {} at {}", node.name, node.ip_address);
        info!("  VNC: {}", self.get_vnc_display(&vm_name)?);

        // Execute build steps
        for (idx, step) in self.steps.iter().enumerate() {
            info!("Executing step {}/{}: {:?}", idx + 1, self.steps.len(), step);
            self.execute_step(&node, step).await?;
        }

        // Save as template
        let template_path = self.save_as_template(&vm_name).await?;
        
        // Clean up builder VM
        info!("Cleaning up builder VM...");
        self.backend.delete_node(&vm_name).await?;

        let final_size = std::fs::metadata(&template_path)
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(BuildResult {
            template_path,
            vm_name,
            final_size_bytes: final_size,
        })
    }

    /// Create builder VM
    async fn create_builder_vm(&self, name: &str, base_image: &Path) -> Result<NodeInfo> {
        // Create basic cloud-init if not provided
        let cloud_init = self.cloud_init.clone().unwrap_or_else(|| {
            CloudInit::builder()
                .add_user("builder", "")
                .build()
        });

        // Create VM with VNC enabled
        self.backend.create_desktop_vm(
            name,
            base_image,
            &cloud_init,
            self.memory_mb,
            self.vcpus,
            self.disk_size_gb,
        ).await
    }

    /// Execute a build step
    async fn execute_step(&self, node: &NodeInfo, step: &BuildStep) -> Result<()> {
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
                self.pause_for_verification(&node.name, message, *vnc_port).await?;
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

    /// Wait for cloud-init to complete (handles apt locks)
    async fn wait_for_cloud_init_complete(&self, node: &NodeInfo) -> Result<()> {
        info!("Waiting for cloud-init to finish (this handles apt locks)...");
        
        let timeout = Duration::from_secs(600); // 10 minutes
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(Error::Backend("Timeout waiting for cloud-init".to_string()));
            }

            // Check cloud-init status
            let output = self.run_ssh_command_silent(
                node,
                "cloud-init status --wait --long || echo 'TIMEOUT'"
            ).await;

            if let Ok(status) = output {
                if status.contains("status: done") {
                    info!("Cloud-init completed successfully");
                    return Ok(());
                } else if status.contains("TIMEOUT") {
                    warn!("Cloud-init status command timed out, checking manually...");
                }
            }

            // Also check if apt lock is free
            let lock_check = self.run_ssh_command_silent(
                node,
                "sudo fuser /var/lib/dpkg/lock-frontend 2>/dev/null || echo 'FREE'"
            ).await;

            if let Ok(lock_status) = lock_check {
                if lock_status.trim() == "FREE" {
                    info!("apt lock is free, cloud-init likely done");
                    tokio::time::sleep(Duration::from_secs(5)).await; // Extra buffer
                    return Ok(());
                }
            }

            debug!("Still waiting for cloud-init/apt...");
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    /// Install packages via apt
    async fn install_packages(&self, node: &NodeInfo, packages: &[String]) -> Result<()> {
        let packages_str = packages.join(" ");
        let cmd = format!(
            "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y {}",
            packages_str
        );
        
        self.run_ssh_command(node, &cmd).await
    }

    /// Run SSH command with output
    async fn run_ssh_command(&self, node: &NodeInfo, command: &str) -> Result<()> {
        info!("  Running: {}", command);
        
        // TODO: Use proper SSH library instead of shelling out
        let output = tokio::process::Command::new("ssh")
            .args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
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

    /// Run SSH command silently (for status checks)
    async fn run_ssh_command_silent(&self, node: &NodeInfo, command: &str) -> Result<String> {
        let output = tokio::process::Command::new("ssh")
            .args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "ConnectTimeout=5",
                &format!("builder@{}", node.ip_address),
                command,
            ])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("SSH command failed: {}", e)))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Pause for user verification
    async fn pause_for_verification(&self, vm_name: &str, message: &str, vnc_port: Option<u16>) -> Result<()> {
        let vnc_display = if let Some(port) = vnc_port {
            format!("localhost:{}", port)
        } else {
            self.get_vnc_display(vm_name)?
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
        std::io::stdin().read_line(&mut input)
            .map_err(|e| Error::Backend(format!("Failed to read input: {}", e)))?;

        info!("User verification complete, continuing build...");
        Ok(())
    }

    /// Save intermediate state
    async fn save_intermediate(&self, vm_name: &str, path: &Path) -> Result<()> {
        info!("Shutting down VM for intermediate save...");
        
        // Shutdown VM
        let _ = tokio::process::Command::new("virsh")
            .args(&["shutdown", vm_name])
            .output()
            .await;

        // Wait for shutdown
        tokio::time::sleep(Duration::from_secs(30)).await;

        // Copy disk
        let disk_path = format!("/var/lib/libvirt/images/{}.qcow2", vm_name);
        
        tokio::process::Command::new("sudo")
            .args(&["cp", &disk_path, path.to_str().unwrap()])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("Failed to save intermediate: {}", e)))?;

        info!("Intermediate state saved to: {}", path.display());

        // Restart VM
        tokio::process::Command::new("virsh")
            .args(&["start", vm_name])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("Failed to restart VM: {}", e)))?;

        tokio::time::sleep(Duration::from_secs(10)).await;

        Ok(())
    }

    /// Reboot VM
    async fn reboot_vm(&self, node: &NodeInfo) -> Result<()> {
        self.run_ssh_command(node, "sudo reboot").await?;
        
        info!("Waiting for VM to reboot...");
        tokio::time::sleep(Duration::from_secs(30)).await;
        
        // Wait for SSH to come back
        for _ in 0..30 {
            if self.run_ssh_command_silent(node, "echo 'ready'").await.is_ok() {
                info!("VM rebooted successfully");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        Err(Error::Backend("VM failed to reboot".to_string()))
    }

    /// Get VNC display for VM
    fn get_vnc_display(&self, vm_name: &str) -> Result<String> {
        let output = std::process::Command::new("virsh")
            .args(&["vncdisplay", vm_name])
            .output()
            .map_err(|e| Error::Backend(format!("Failed to get VNC display: {}", e)))?;

        if !output.status.success() {
            return Err(Error::Backend("Failed to get VNC display".to_string()));
        }

        let display = String::from_utf8_lossy(&output.stdout).trim().to_string();
        
        // Convert :N to localhost:590N
        if let Some(num) = display.strip_prefix(':') {
            if let Ok(n) = num.parse::<u16>() {
                return Ok(format!("localhost:{}", 5900 + n));
            }
        }

        Ok(display)
    }

    /// Save VM as template
    async fn save_as_template(&self, vm_name: &str) -> Result<PathBuf> {
        info!("Shutting down VM to save as template...");
        
        // Shutdown
        let _ = tokio::process::Command::new("virsh")
            .args(&["shutdown", vm_name])
            .output()
            .await;

        // Wait for shutdown
        tokio::time::sleep(Duration::from_secs(30)).await;

        let disk_path = format!("/var/lib/libvirt/images/{}.qcow2", vm_name);
        let template_path = format!("/var/lib/libvirt/images/{}-template.qcow2", self.name);

        // Sparsify
        info!("Optimizing template...");
        tokio::process::Command::new("sudo")
            .args(&["virt-sparsify", "--in-place", &disk_path])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("Failed to sparsify: {}", e)))?;

        // Copy to final location
        tokio::process::Command::new("sudo")
            .args(&["cp", &disk_path, &template_path])
            .output()
            .await
            .map_err(|e| Error::Backend(format!("Failed to copy template: {}", e)))?;

        // Set permissions
        tokio::process::Command::new("sudo")
            .args(&["chown", "libvirt-qemu:kvm", &template_path])
            .output()
            .await?;

        tokio::process::Command::new("sudo")
            .args(&["chmod", "644", &template_path])
            .output()
            .await?;

        info!("Template saved: {}", template_path);

        Ok(PathBuf::from(template_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_creation() {
        let builder = ImageBuilder::new("test-image").unwrap();
        assert_eq!(builder.name, "test-image");
        assert_eq!(builder.memory_mb, 4096);
        assert_eq!(builder.vcpus, 2);
    }

    #[test]
    fn test_builder_configuration() {
        let builder = ImageBuilder::new("test")
            .unwrap()
            .with_memory(8192)
            .with_vcpus(4)
            .with_disk_size(50);

        assert_eq!(builder.memory_mb, 8192);
        assert_eq!(builder.vcpus, 4);
        assert_eq!(builder.disk_size_gb, 50);
    }

    #[test]
    fn test_build_steps() {
        let builder = ImageBuilder::new("test")
            .unwrap()
            .add_step(BuildStep::WaitForCloudInit)
            .add_step(BuildStep::InstallPackages(vec!["vim".to_string()]))
            .add_step(BuildStep::Reboot);

        assert_eq!(builder.steps.len(), 3);
    }
}

