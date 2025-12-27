//! VM disk management utilities for LibvirtBackend
//!
//! Handles qcow2 disk image operations including copy-on-write overlays.

use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info};

use crate::{Error, Result};

/// Disk image manager for VM backends
pub struct DiskManager {
    overlay_dir: PathBuf,
}

impl DiskManager {
    /// Create a new disk manager with specified overlay directory
    pub fn new(overlay_dir: PathBuf) -> Self {
        Self { overlay_dir }
    }

    /// Create a copy-on-write disk overlay from a base image
    ///
    /// This creates a qcow2 overlay that uses the base image as backing storage,
    /// allowing fast VM creation without copying the entire disk.
    pub async fn create_overlay(&self, base_image: &Path, vm_name: &str) -> Result<PathBuf> {
        // Ensure overlay directory exists
        tokio::fs::create_dir_all(&self.overlay_dir).await?;

        let mut overlay_path = self.overlay_dir.clone();
        overlay_path.push(format!("{}.qcow2", vm_name));

        info!(
            "Creating disk overlay: {} (base: {})",
            overlay_path.display(),
            base_image.display()
        );

        // qemu-img create -f qcow2 -b base.qcow2 -F qcow2 overlay.qcow2
        let output = Command::new("qemu-img")
            .args(&["create", "-f", "qcow2", "-b"])
            .arg(base_image)
            .args(&["-F", "qcow2"])
            .arg(&overlay_path)
            .output()
            .await
            .map_err(|e| Error::Backend(format!("Failed to run qemu-img: {}", e)))?;

        if !output.status.success() {
            return Err(Error::Backend(format!(
                "Failed to create disk overlay: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        debug!(
            "Disk overlay created successfully: {}",
            overlay_path.display()
        );
        Ok(overlay_path)
    }

    /// Delete a disk overlay
    pub async fn delete_overlay(&self, vm_name: &str) -> Result<()> {
        let mut overlay_path = self.overlay_dir.clone();
        overlay_path.push(format!("{}.qcow2", vm_name));

        if overlay_path.exists() {
            info!("Deleting disk overlay: {}", overlay_path.display());
            tokio::fs::remove_file(&overlay_path).await?;
        }

        Ok(())
    }

    /// Check if qemu-img is available
    pub async fn is_available() -> bool {
        Command::new("qemu-img")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Parse memory string (e.g., "2G", "512M", "2048") to megabytes
pub fn parse_memory(mem_str: &str) -> Option<u32> {
    let mem_str = mem_str.trim().to_uppercase();

    if let Some(num) = mem_str.strip_suffix('G') {
        num.parse::<u32>().ok().map(|n| n * 1024)
    } else if let Some(num) = mem_str.strip_suffix('M') {
        num.parse().ok()
    } else {
        // Assume megabytes if no suffix
        mem_str.parse().ok()
    }
}

/// Generate libvirt domain XML for a VM
pub fn generate_domain_xml(
    name: &str,
    disk_path: &Path,
    memory_mb: u32,
    vcpus: u32,
    network: &str,
    serial_log: &Path,
) -> String {
    format!(
        r#"<domain type='kvm'>
  <name>{name}</name>
  <memory unit='MiB'>{memory}</memory>
  <vcpu>{vcpus}</vcpu>
  <os>
    <type arch='x86_64'>hvm</type>
    <boot dev='hd'/>
  </os>
  <features>
    <acpi/>
    <apic/>
  </features>
  <clock offset='utc'/>
  <on_poweroff>destroy</on_poweroff>
  <on_reboot>restart</on_reboot>
  <on_crash>destroy</on_crash>
  <devices>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{disk}'/>
      <target dev='vda' bus='virtio'/>
    </disk>
    <interface type='network'>
      <source network='{network}'/>
      <model type='virtio'/>
    </interface>
    <serial type='file'>
      <source path='{serial_log}'/>
      <target type='isa-serial' port='0'/>
    </serial>
    <console type='file'>
      <source path='{serial_log}'/>
      <target type='serial' port='0'/>
    </console>
    <graphics type='none'/>
  </devices>
</domain>"#,
        name = name,
        memory = memory_mb,
        vcpus = vcpus,
        disk = disk_path.display(),
        network = network,
        serial_log = serial_log.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory() {
        assert_eq!(parse_memory("2G"), Some(2048));
        assert_eq!(parse_memory("512M"), Some(512));
        assert_eq!(parse_memory("2048"), Some(2048));
        assert_eq!(parse_memory("4g"), Some(4096));
        assert_eq!(parse_memory("  1G  "), Some(1024));
        assert_eq!(parse_memory("invalid"), None);
    }

    #[test]
    fn test_generate_domain_xml() {
        let xml = generate_domain_xml(
            "test-vm",
            Path::new("/tmp/disk.qcow2"),
            2048,
            2,
            "test-net",
            Path::new("/tmp/serial.log"),
        );

        assert!(xml.contains("<name>test-vm</name>"));
        assert!(xml.contains("<memory unit='MiB'>2048</memory>"));
        assert!(xml.contains("<vcpu>2</vcpu>"));
        assert!(xml.contains("source file='/tmp/disk.qcow2'"));
        assert!(xml.contains("source network='test-net'"));
        assert!(xml.contains("source path='/tmp/serial.log'"));
    }
}
