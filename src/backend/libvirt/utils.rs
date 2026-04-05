// SPDX-License-Identifier: AGPL-3.0-or-later
//! Utility functions for LibvirtBackend
//!
//! This module contains helper functions for template management, IP discovery,
//! and other supporting operations.

use crate::Result;
use std::path::PathBuf;
use tracing::{info, warn};
use virt::domain::Domain;

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
    /// Queries libvirt for the VM's IP address from DHCP lease source data.
    /// This is an internal utility used by various VM creation and management
    /// functions.
    ///
    /// # Arguments
    /// * `name` - VM domain name
    ///
    /// # Returns
    /// IP address as string (e.g., "192.168.122.10")
    ///
    pub(super) async fn get_vm_ip_by_name(&self, name: &str) -> Result<String> {
        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_name(&*conn, name)
            .map_err(|e| crate::Error::Backend(format!("Failed to look up domain: {}", e)))?;
        let interfaces = domain
            .interface_addresses(virt::sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE, 0)
            .map_err(|e| {
                crate::Error::Backend(format!("Failed to query interface addresses: {}", e))
            })?;
        for iface in interfaces {
            for addr in iface.addrs {
                if addr.typed == virt::sys::VIR_IP_ADDR_TYPE_IPV4 as i64 {
                    info!("Found VM IP: {}", addr.addr);
                    return Ok(addr.addr);
                }
            }
        }

        Err(crate::Error::Backend(
            "No IP address found for VM".to_string(),
        ))
    }
}
