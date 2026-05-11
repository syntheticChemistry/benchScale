// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright © 2024-2025 DataScienceBioLab
//
//! VM Lifecycle Cleanup
//!
//! Robust cleanup of VMs and their resources using the `virt` crate API
//! (no `virsh` CLI shell-outs). Handles orphaned QEMU processes, stale
//! disk images, cloud-init ISOs, and bulk cleanup operations.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::{error, info, warn};
use virt::error::ErrorNumber;

fn libvirt_connect() -> anyhow::Result<virt::connect::Connect> {
    let uri = super::libvirt_uri();
    Ok(virt::connect::Connect::open(Some(&uri))?)
}

/// VM cleanup manager.
pub struct VmCleanup {
    image_dir: PathBuf,
}

impl VmCleanup {
    /// Create a new cleanup manager.
    pub fn new(image_dir: impl Into<PathBuf>) -> Self {
        Self {
            image_dir: image_dir.into(),
        }
    }

    /// Clean up a specific VM by name.
    ///
    /// 1. Graceful shutdown via libvirt API
    /// 2. Force destroy if still running
    /// 3. Undefine from libvirt
    /// 4. Remove disk images and cloud-init ISOs
    pub fn cleanup_vm(&self, vm_name: &str) -> Result<()> {
        info!("Cleaning up VM: {}", vm_name);

        if let Ok(conn) = libvirt_connect() {
            if let Ok(domain) = virt::domain::Domain::lookup_by_name(&conn, vm_name) {
                let _ = domain.shutdown();
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(5));

        // Force destroy if still running
        let conn = libvirt_connect().context("Failed to connect for VM destroy")?;
        match virt::domain::Domain::lookup_by_name(&conn, vm_name) {
            Ok(domain) => {
                if let Err(e) = domain.destroy() {
                    let msg = e.message();
                    if !msg.contains("domain is not running")
                        && !msg.contains("failed to get domain")
                    {
                        warn!("Failed to destroy VM {}: {}", vm_name, msg);
                    }
                }
            }
            Err(e) => {
                if e.code() != ErrorNumber::NoDomain
                    && !e.message().contains("failed to get domain")
                {
                    warn!("Failed to destroy VM {}: {}", vm_name, e.message());
                }
            }
        }

        // Undefine the VM
        let conn = libvirt_connect().context("Failed to connect for VM undefine")?;
        match virt::domain::Domain::lookup_by_name(&conn, vm_name) {
            Ok(domain) => {
                if let Err(e) = domain.undefine() {
                    let msg = e.message();
                    if !msg.contains("failed to get domain") {
                        warn!("Failed to undefine VM {}: {}", vm_name, msg);
                    }
                }
            }
            Err(e) => {
                if e.code() != ErrorNumber::NoDomain
                    && !e.message().contains("failed to get domain")
                {
                    warn!("Failed to undefine VM {}: {}", vm_name, e.message());
                }
            }
        }

        let disk_path = self.image_dir.join(format!("{}.qcow2", vm_name));
        if disk_path.exists() {
            std::fs::remove_file(&disk_path)
                .with_context(|| format!("Failed to remove disk image: {}", disk_path.display()))?;
            info!("  Removed disk image: {:?}", disk_path);
        }

        let cidata_path = self.image_dir.join(format!("{}-cidata.iso", vm_name));
        if cidata_path.exists() {
            std::fs::remove_file(&cidata_path).with_context(|| {
                format!("Failed to remove cloud-init ISO: {}", cidata_path.display())
            })?;
            info!("  Removed cloud-init ISO: {:?}", cidata_path);
        }

        info!("VM {} cleaned up successfully", vm_name);
        Ok(())
    }

    /// Clean up all VMs matching a prefix.
    pub fn cleanup_matching(&self, prefix: &str) -> Result<Vec<String>> {
        info!("Cleaning up all VMs matching prefix: {}", prefix);

        let conn = libvirt_connect().context("Failed to list VMs")?;
        let domains = conn.list_all_domains(0)?;
        let matching_vms: Vec<String> = domains
            .into_iter()
            .filter_map(|d| d.get_name().ok())
            .filter(|name| name.starts_with(prefix))
            .collect();

        info!("  Found {} matching VMs", matching_vms.len());

        let mut cleaned = Vec::new();
        for vm_name in matching_vms {
            match self.cleanup_vm(&vm_name) {
                Ok(()) => cleaned.push(vm_name),
                Err(e) => error!("Failed to clean up VM {}: {}", vm_name, e),
            }
        }

        Ok(cleaned)
    }

    /// Clean up orphaned disk images (no corresponding VM).
    pub fn cleanup_orphaned_disks(&self) -> Result<Vec<PathBuf>> {
        info!("Cleaning up orphaned disk images");

        let conn = libvirt_connect().context("Failed to list VMs")?;
        let domains = conn.list_all_domains(0)?;
        let vms: std::collections::HashSet<String> = domains
            .into_iter()
            .filter_map(|d| d.get_name().ok())
            .collect();

        let mut cleaned = Vec::new();

        for entry in std::fs::read_dir(&self.image_dir).context("Failed to read image directory")? {
            let entry = entry?;
            let path = entry.path();

            if let Some(ext) = path.extension()
                && ext == "qcow2"
                && let Some(file_stem) = path.file_stem()
            {
                let vm_name = file_stem.to_string_lossy().to_string();

                if vm_name.contains("cloudimg") || vm_name.contains("base") {
                    continue;
                }

                if !vms.contains(&vm_name) {
                    warn!("  Found orphaned disk: {:?}", path);
                    std::fs::remove_file(&path).with_context(|| {
                        format!("Failed to remove orphaned disk: {}", path.display())
                    })?;
                    cleaned.push(path);
                }
            }
        }

        info!("Cleaned up {} orphaned disk images", cleaned.len());
        Ok(cleaned)
    }

    /// Emergency cleanup: stop all VMs and clean everything.
    pub fn emergency_cleanup(&self) -> Result<()> {
        warn!("EMERGENCY CLEANUP - This will stop ALL VMs!");

        let conn = libvirt_connect().context("Failed to list VMs")?;
        let domains = conn.list_all_domains(0)?;
        let vms: Vec<String> = domains
            .into_iter()
            .filter_map(|d| d.get_name().ok())
            .collect();

        info!("  Found {} VMs to clean", vms.len());

        for vm_name in vms {
            if let Err(e) = self.cleanup_vm(&vm_name) {
                error!("Failed to clean up VM {}: {}", vm_name, e);
            }
        }

        self.cleanup_orphaned_disks()?;

        info!("Emergency cleanup complete");
        Ok(())
    }
}

impl Default for VmCleanup {
    fn default() -> Self {
        Self::new(crate::config::StorageConfig::default().vm_images_dir_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_creation() {
        let expected = crate::config::StorageConfig::default().vm_images_dir_or_default();
        let cleanup = VmCleanup::new(&expected);
        assert_eq!(cleanup.image_dir, expected);
    }
}
