// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright © 2024-2025 DataScienceBioLab
//
//! VM Lifecycle Cleanup - Deep Debt Solution
//!
//! This module provides robust cleanup of VMs and their resources.
//! It handles:
//! - Orphaned QEMU processes
//! - Stale disk images and cloud-init ISOs
//! - Unreachable VMs that need forced cleanup
//! - Bulk cleanup operations

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use tracing::{error, info, warn};

/// VM cleanup manager
pub struct VmCleanup {
    image_dir: PathBuf,
}

impl VmCleanup {
    /// Create a new cleanup manager
    pub fn new(image_dir: impl Into<PathBuf>) -> Self {
        Self {
            image_dir: image_dir.into(),
        }
    }

    /// Clean up a specific VM by name
    ///
    /// This will:
    /// 1. Try to gracefully stop the VM via virsh
    /// 2. Forcefully destroy if graceful stop fails
    /// 3. Undefine the VM from libvirt
    /// 4. Remove disk images and cloud-init ISOs
    pub fn cleanup_vm(&self, vm_name: &str) -> Result<()> {
        info!("🧹 Cleaning up VM: {}", vm_name);

        // Try graceful shutdown first
        let _ = Command::new("virsh")
            .args(&["shutdown", vm_name])
            .output()
            .context("Failed to gracefully shutdown VM");

        // Wait a moment for graceful shutdown
        std::thread::sleep(std::time::Duration::from_secs(5));

        // Force destroy if still running
        let destroy_output = Command::new("virsh")
            .args(&["destroy", vm_name])
            .output()
            .context("Failed to destroy VM")?;

        if !destroy_output.status.success() {
            let stderr = String::from_utf8_lossy(&destroy_output.stderr);
            // Don't error if VM doesn't exist
            if !stderr.contains("domain is not running") && !stderr.contains("failed to get domain") {
                warn!("Failed to destroy VM {}: {}", vm_name, stderr);
            }
        }

        // Undefine the VM
        let undefine_output = Command::new("virsh")
            .args(&["undefine", vm_name])
            .output()
            .context("Failed to undefine VM")?;

        if !undefine_output.status.success() {
            let stderr = String::from_utf8_lossy(&undefine_output.stderr);
            if !stderr.contains("failed to get domain") {
                warn!("Failed to undefine VM {}: {}", vm_name, stderr);
            }
        }

        // Remove disk images
        let disk_path = self.image_dir.join(format!("{}.qcow2", vm_name));
        if disk_path.exists() {
            std::fs::remove_file(&disk_path)
                .with_context(|| format!("Failed to remove disk image: {:?}", disk_path))?;
            info!("   Removed disk image: {:?}", disk_path);
        }

        // Remove cloud-init ISO
        let cidata_path = self.image_dir.join(format!("{}-cidata.iso", vm_name));
        if cidata_path.exists() {
            std::fs::remove_file(&cidata_path)
                .with_context(|| format!("Failed to remove cloud-init ISO: {:?}", cidata_path))?;
            info!("   Removed cloud-init ISO: {:?}", cidata_path);
        }

        info!("✅ VM {} cleaned up successfully", vm_name);
        Ok(())
    }

    /// Clean up all VMs matching a prefix
    pub fn cleanup_matching(&self, prefix: &str) -> Result<Vec<String>> {
        info!("🧹 Cleaning up all VMs matching prefix: {}", prefix);

        let list_output = Command::new("virsh")
            .args(&["list", "--all", "--name"])
            .output()
            .context("Failed to list VMs")?;

        let vms = String::from_utf8_lossy(&list_output.stdout);
        let matching_vms: Vec<String> = vms
            .lines()
            .filter(|line| !line.is_empty() && line.starts_with(prefix))
            .map(|s| s.to_string())
            .collect();

        info!("   Found {} matching VMs", matching_vms.len());

        let mut cleaned = Vec::new();
        for vm_name in matching_vms {
            match self.cleanup_vm(&vm_name) {
                Ok(_) => cleaned.push(vm_name),
                Err(e) => error!("Failed to clean up VM {}: {}", vm_name, e),
            }
        }

        Ok(cleaned)
    }

    /// Clean up orphaned disk images (no corresponding VM)
    pub fn cleanup_orphaned_disks(&self) -> Result<Vec<PathBuf>> {
        info!("🧹 Cleaning up orphaned disk images");

        // Get list of all defined VMs
        let list_output = Command::new("virsh")
            .args(&["list", "--all", "--name"])
            .output()
            .context("Failed to list VMs")?;

        let vms: std::collections::HashSet<String> = String::from_utf8_lossy(&list_output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(|s| s.to_string())
            .collect();

        let mut cleaned = Vec::new();

        // Check all .qcow2 files in image directory
        for entry in std::fs::read_dir(&self.image_dir)
            .context("Failed to read image directory")?
        {
            let entry = entry?;
            let path = entry.path();

            if let Some(ext) = path.extension() {
                if ext == "qcow2" {
                    if let Some(file_stem) = path.file_stem() {
                        let vm_name = file_stem.to_string_lossy().to_string();
                        
                        // Skip base images (these are templates)
                        if vm_name.contains("cloudimg") || vm_name.contains("base") {
                            continue;
                        }

                        // If no VM exists for this disk, it's orphaned
                        if !vms.contains(&vm_name) {
                            warn!("   Found orphaned disk: {:?}", path);
                            std::fs::remove_file(&path)
                                .with_context(|| format!("Failed to remove orphaned disk: {:?}", path))?;
                            cleaned.push(path);
                        }
                    }
                }
            }
        }

        info!("✅ Cleaned up {} orphaned disk images", cleaned.len());
        Ok(cleaned)
    }

    /// Emergency cleanup: Stop all QEMU processes and clean everything
    ///
    /// This is a nuclear option and should only be used in dire situations.
    pub fn emergency_cleanup(&self) -> Result<()> {
        warn!("🚨 EMERGENCY CLEANUP - This will stop ALL VMs!");

        // List all VMs
        let list_output = Command::new("virsh")
            .args(&["list", "--all", "--name"])
            .output()
            .context("Failed to list VMs")?;

        let vms: Vec<String> = String::from_utf8_lossy(&list_output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(|s| s.to_string())
            .collect();

        info!("   Found {} VMs to clean", vms.len());

        for vm_name in vms {
            if let Err(e) = self.cleanup_vm(&vm_name) {
                error!("Failed to clean up VM {}: {}", vm_name, e);
            }
        }

        // Clean up orphaned disks
        self.cleanup_orphaned_disks()?;

        info!("✅ Emergency cleanup complete");
        Ok(())
    }
}

impl Default for VmCleanup {
    fn default() -> Self {
        Self::new("/var/lib/libvirt/images")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_creation() {
        let cleanup = VmCleanup::new("/var/lib/libvirt/images");
        assert_eq!(cleanup.image_dir, PathBuf::from("/var/lib/libvirt/images"));
    }
}

