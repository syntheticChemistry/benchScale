// SPDX-License-Identifier: AGPL-3.0-only
use anyhow::Result;
use tracing::{error, info, warn, debug};
use std::path::PathBuf;
use virt::connect::Connect;
use virt::domain::Domain;

use super::vm_registry::{VmRegistry, VmStatus};

/// RAII guard for VM lifecycle management.
/// Ensures VMs are cleaned up on drop unless explicitly preserved.
/// Also integrates with VmRegistry for orphan detection.
///
/// # Evolution #17: Capability-Based Path Discovery
///
/// The guard now accepts the images directory from SystemCapabilities
/// rather than hardcoding `/var/lib/libvirt/images`. This makes it:
/// - Portable across different libvirt configurations
/// - Compatible with user-session libvirt
/// - Compatible with custom storage pools
pub struct VmGuard {
    vm_name: String,
    connection: Connect,
    keep_on_drop: bool,
    images_dir: PathBuf,
}

impl VmGuard {
    /// Create a new VM guard with capability-based path discovery.
    ///
    /// # Arguments
    /// * `vm_name` - Name of the VM to guard
    /// * `connection` - Libvirt connection
    /// * `images_dir` - Path to libvirt images directory (from SystemCapabilities)
    ///
    /// By default, the VM will be cleaned up when the guard is dropped.
    /// Also registers the VM in the registry for orphan detection.
    ///
    /// # Example
    /// ```rust,no_run
    /// use benchscale::{LibvirtBackend, VmGuard};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new().await?;
    /// let conn = backend.raw_connection()?;
    /// let images_dir = backend.capabilities().storage.images_dir.clone();
    ///
    /// let guard = VmGuard::new("my-vm".to_string(), conn, images_dir);
    /// // VM will be automatically cleaned up when guard drops
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(vm_name: String, connection: Connect, images_dir: PathBuf) -> Self {
        info!("🔒 VmGuard: Tracking VM '{}'", vm_name);
        debug!("  Images dir: {}", images_dir.display());
        
        // Register VM in registry (best effort, don't fail if registry unavailable)
        if let Ok(mut registry) = VmRegistry::new() {
            if let Err(e) = registry.register(vm_name.clone(), None) {
                warn!("Failed to register VM in registry: {}", e);
            }
        }
        
        Self {
            vm_name,
            connection,
            keep_on_drop: false,
            images_dir,
        }
    }

    /// Mark the VM to be preserved on drop (successful build).
    /// Call this after successful completion to prevent cleanup.
    /// Updates the registry status to Completed.
    pub fn preserve(mut self) -> Self {
        info!("✅ VmGuard: Preserving VM '{}'", self.vm_name);
        self.keep_on_drop = true;
        
        // Update registry status (best effort)
        if let Ok(mut registry) = VmRegistry::new() {
            let _ = registry.update_status(&self.vm_name, VmStatus::Completed);
        }
        
        self
    }

    /// Explicitly cleanup the VM now (destroy, undefine, remove disk).
    /// This consumes the guard to prevent double-cleanup.
    pub fn cleanup(mut self) -> Result<()> {
        self.destroy_vm()?;
        self.keep_on_drop = true; // Prevent drop from trying again
        Ok(())
    }

    /// Get the VM name
    pub fn vm_name(&self) -> &str {
        &self.vm_name
    }

    /// Internal cleanup implementation
    fn destroy_vm(&self) -> Result<()> {
        info!("🧹 VmGuard: Cleaning up VM '{}'", self.vm_name);

        // 1. Destroy and undefine the VM from libvirt
        match Domain::lookup_by_name(&self.connection, &self.vm_name) {
            Ok(domain) => {
                // Destroy if running
                if domain.is_active().unwrap_or(false) {
                    if let Err(e) = domain.destroy() {
                        warn!("  ⚠️  Failed to destroy VM: {}", e);
                    } else {
                        info!("  ✅ VM destroyed");
                    }
                }

                // Undefine the domain
                if let Err(e) = domain.undefine() {
                    warn!("  ⚠️  Failed to undefine VM: {}", e);
                } else {
                    info!("  ✅ VM undefined");
                }
            }
            Err(e) => {
                warn!("  ⚠️  VM not found in libvirt: {}", e);
            }
        }

        // 2. Remove disk image (using discovered path from capabilities)
        let disk_path = self.images_dir.join(format!("{}.qcow2", self.vm_name));
        debug!("  Checking disk: {}", disk_path.display());
        if disk_path.exists() {
            if let Err(e) = std::fs::remove_file(&disk_path) {
                warn!("  ⚠️  Failed to remove disk: {}", e);
            } else {
                info!("  ✅ Disk removed: {}", disk_path.display());
            }
        } else {
            debug!("  Disk not found (may have been cleaned up already)");
        }

        // 3. Remove cloud-init ISO if it exists (using discovered path)
        let iso_path = self.images_dir.join(format!("{}-seed.iso", self.vm_name));
        if iso_path.exists() {
            if let Err(e) = std::fs::remove_file(&iso_path) {
                warn!("  ⚠️  Failed to remove cloud-init ISO: {}", e);
            } else {
                info!("  ✅ Cloud-init ISO removed");
            }
        }

        // 4. Unregister from registry
        if let Ok(mut registry) = VmRegistry::new() {
            let _ = registry.unregister(&self.vm_name);
        }

        Ok(())
    }
}

impl Drop for VmGuard {
    fn drop(&mut self) {
        // Check environment variable to preserve VM for diagnostics
        let preserve_on_failure = std::env::var("PRESERVE_VM_ON_FAILURE")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);
        
        if preserve_on_failure {
            warn!(
                "🔬 VmGuard: PRESERVE_VM_ON_FAILURE set - VM '{}' will NOT be cleaned up",
                self.vm_name
            );
            return;
        }
        
        if !self.keep_on_drop {
            warn!(
                "⚠️  VmGuard: VM '{}' not preserved, cleaning up on drop...",
                self.vm_name
            );
            if let Err(e) = self.destroy_vm() {
                error!("❌ VmGuard: Failed to cleanup VM '{}': {}", self.vm_name, e);
            } else {
                info!("✅ VmGuard: Successfully cleaned up VM '{}'", self.vm_name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_guard_preserve() {
        // This test just verifies the API compiles
        // Actual cleanup would require a real libvirt connection
    }
}

