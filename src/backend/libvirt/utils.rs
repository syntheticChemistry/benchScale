//! Utility functions for LibvirtBackend
//!
//! This module contains helper functions for template management, IP discovery,
//! and other supporting operations.

use crate::Result;
use std::path::PathBuf;
use tracing::{info, warn};

use super::LibvirtBackend;

impl LibvirtBackend {
    // ========================================================================
    // Template Management API
    // ========================================================================

    /// Register a template with a friendly name
    ///
    /// Templates allow you to create VMs from pre-configured base images
    /// (e.g., from agentReagents) using friendly names instead of full paths.
    ///
    /// # Arguments
    /// * `name` - Friendly name for the template (e.g., "rustdesk-ubuntu-22.04")
    /// * `path` - Full path to the template qcow2 file
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::LibvirtBackend;
    /// # use std::path::PathBuf;
    /// # fn example() -> anyhow::Result<()> {
    /// let mut backend = LibvirtBackend::new()?;
    /// backend.register_template(
    ///     "my-template",
    ///     PathBuf::from("/path/to/template.qcow2")
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn register_template(&mut self, name: impl Into<String>, path: PathBuf) -> Result<()> {
        let name = name.into();

        if !path.exists() {
            return Err(crate::Error::Backend(format!(
                "Template path does not exist: {:?}",
                path
            )));
        }

        if path.extension().and_then(|s| s.to_str()) != Some("qcow2") {
            warn!("Template {:?} does not have .qcow2 extension", path);
        }

        info!("Registered template '{}' -> {:?}", name, path);
        self.templates.insert(name, path);
        Ok(())
    }

    /// Discover templates from the configured template directory
    ///
    /// Scans the template directory (from config or BENCHSCALE_TEMPLATE_DIR)
    /// and registers all .qcow2 files as templates.
    ///
    /// # Returns
    /// Number of templates discovered
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::LibvirtBackend;
    /// # fn example() -> anyhow::Result<()> {
    /// let mut backend = LibvirtBackend::new()?;
    /// let count = backend.discover_templates()?;
    /// println!("Discovered {} templates", count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn discover_templates(&mut self) -> Result<usize> {
        let template_dir = self.config.template_dir
            .as_ref()
            .ok_or_else(|| crate::Error::Backend(
                "No template directory configured. Set BENCHSCALE_TEMPLATE_DIR or ensure agentReagents is in a standard location.".to_string()
            ))?;

        if !template_dir.exists() {
            return Err(crate::Error::Backend(format!(
                "Template directory does not exist: {:?}",
                template_dir
            )));
        }

        let mut count = 0;
        for entry in std::fs::read_dir(template_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only register .qcow2 files
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("qcow2") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    self.templates.insert(name.to_string(), path);
                    count += 1;
                }
            }
        }

        info!("Discovered {} templates from {:?}", count, template_dir);
        Ok(count)
    }

    /// List all registered template names
    ///
    /// Returns a sorted list of template names that can be used with
    /// `create_from_registered_template()`.
    ///
    /// # Example
    /// ```no_run
    /// # use benchscale::LibvirtBackend;
    /// # fn example() -> anyhow::Result<()> {
    /// let backend = LibvirtBackend::new()?;
    /// for template in backend.list_templates() {
    ///     println!("  - {}", template);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_templates(&self) -> Vec<String> {
        let mut names: Vec<_> = self.templates.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get the path for a registered template
    ///
    /// # Arguments
    /// * `name` - Template name (as registered or discovered)
    ///
    /// # Returns
    /// Path to the template file
    ///
    /// # Errors
    /// Returns error if template is not registered
    pub fn get_template_path(&self, name: &str) -> Result<&PathBuf> {
        self.templates.get(name).ok_or_else(|| {
            crate::Error::Backend(format!(
                "Template '{}' not registered. Available templates: {:?}",
                name,
                self.list_templates()
            ))
        })
    }

    // ========================================================================
    // IP Discovery Utilities
    // ========================================================================

    /// Get VM IP address by domain name
    ///
    /// Uses virsh to query libvirt for the VM's IP address from DHCP leases.
    /// This is an internal utility used by various VM creation and management
    /// functions.
    ///
    /// # Arguments
    /// * `name` - VM domain name
    ///
    /// # Returns
    /// IP address as string (e.g., "192.168.122.10")
    ///
    /// # Note
    /// This uses the system virsh command for simplicity and reliability.
    /// It's a pragmatic choice that leverages existing tools rather than
    /// implementing complex libvirt API calls.
    pub(super) async fn get_vm_ip_by_name(&self, name: &str) -> Result<String> {
        // Use virsh command to get IP (simpler than libvirt API for this)
        let output = tokio::process::Command::new("virsh")
            .args(["domifaddr", name, "--source", "lease"])
            .output()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to run virsh: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(format!(
                "virsh domifaddr failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);

        // Parse IP from virsh output
        // Format: Name MAC address Protocol Address
        //         ----------------------------------------------------------------
        //         vnet0 52:54:00:xx:xx:xx ipv4 192.168.122.x/24
        for line in output_str.lines() {
            if line.contains("ipv4") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(ip_with_mask) = parts.last() {
                    if let Some(ip) = ip_with_mask.split('/').next() {
                        info!("Found VM IP: {}", ip);
                        return Ok(ip.to_string());
                    }
                }
            }
        }

        Err(crate::Error::Backend(
            "No IP address found for VM".to_string(),
        ))
    }
}
