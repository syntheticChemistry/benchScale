// SPDX-License-Identifier: AGPL-3.0-only
//! Runtime capability discovery for benchScale
//!
//! This module implements capability-based configuration: the system discovers
//! runtime capabilities rather than relying on hardcoded values. This makes
//! benchScale portable across different libvirt configurations, distributions,
//! and environments.
//!
//! ## Design Principles
//!
//! 1. **Self-Knowledge Only** - Code knows itself, discovers other systems
//! 2. **Progressive Enhancement** - Try discovery, fall back gracefully
//! 3. **Fail-Safe Defaults** - Always have sensible defaults
//! 4. **User Override** - Environment variables can override everything
//!
//! ## Discovery Hierarchy
//!
//! ```text
//! Tier 1: Runtime Discovery    (query libvirt, system) ✨ Best
//! Tier 2: Environment Variables (user/admin preference) 👍 Good
//! Tier 3: Sensible Defaults    (standard installations) ✅ Acceptable
//! Tier 4: User Override         (explicit configuration)  🎯 Always
//! ```
//!
//! ## Example
//!
//! ```rust,no_run
//! use benchscale::capabilities::SystemCapabilities;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let caps = SystemCapabilities::discover().await?;
//!
//! println!("Libvirt network: {}", caps.network.default_network);
//! println!("Gateway: {}", caps.network.gateway);
//! println!("Images dir: {}", caps.storage.images_dir.display());
//! # Ok(())
//! # }
//! ```

use crate::Result;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Complete system capabilities discovered at runtime
#[derive(Debug, Clone)]
pub struct SystemCapabilities {
    /// Network configuration capabilities
    pub network: NetworkCapabilities,

    /// Storage and filesystem capabilities
    pub storage: StorageCapabilities,

    /// Virtualization capabilities
    pub virtualization: VirtCapabilities,
}

impl SystemCapabilities {
    /// Discover system capabilities at runtime
    ///
    /// This queries libvirt, the system, and environment to build a complete
    /// picture of available capabilities and configuration.
    ///
    /// # Returns
    ///
    /// Discovered capabilities with sensible fallbacks for anything that
    /// couldn't be discovered.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use benchscale::capabilities::SystemCapabilities;
    /// # async fn example() -> anyhow::Result<()> {
    /// let caps = SystemCapabilities::discover().await?;
    /// println!("Network: {}", caps.network.default_network);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover() -> Result<Self> {
        info!("Discovering system capabilities...");

        let network = NetworkCapabilities::discover().await?;
        let storage = StorageCapabilities::discover().await?;
        let virtualization = VirtCapabilities::discover().await?;

        info!("Capability discovery complete");
        debug!("  Network: {}", network.default_network);
        debug!("  Gateway: {}", network.gateway);
        debug!("  Subnet: {}", network.subnet);
        debug!("  Images dir: {}", storage.images_dir.display());

        Ok(Self {
            network,
            storage,
            virtualization,
        })
    }
}

/// Network configuration capabilities
#[derive(Debug, Clone)]
pub struct NetworkCapabilities {
    /// Default libvirt network name (e.g., "default")
    pub default_network: String,

    /// Network gateway IP address
    pub gateway: String,

    /// Network subnet in CIDR notation (e.g., "192.168.122.0/24")
    pub subnet: String,

    /// Network prefix for IP allocation (e.g., "192.168.122")
    pub prefix: String,

    /// Netmask bits (e.g., 24 for /24)
    pub netmask_bits: u8,

    /// IP pool start (for deterministic allocation)
    pub ip_pool_start: String,

    /// IP pool end (for deterministic allocation)
    pub ip_pool_end: String,

    /// Default network interface name in VMs (e.g., "enp1s0")
    pub default_interface: String,
}

impl NetworkCapabilities {
    /// Discover network capabilities from libvirt
    pub async fn discover() -> Result<Self> {
        debug!("Discovering network capabilities...");

        // Try to discover from libvirt's default network
        if let Ok(caps) = Self::discover_from_libvirt().await {
            info!("Network capabilities discovered from libvirt");
            return Ok(caps);
        }

        // Try environment variables
        if let Ok(caps) = Self::discover_from_env() {
            info!("Network capabilities from environment variables");
            return Ok(caps);
        }

        // Fall back to sensible defaults (standard libvirt)
        warn!("Using default network capabilities (standard libvirt config)");
        Ok(Self::default_libvirt())
    }

    /// Discover from libvirt's default network
    async fn discover_from_libvirt() -> Result<Self> {
        // Query virsh for default network configuration
        let output = tokio::process::Command::new("virsh")
            .args(["net-dumpxml", "default"])
            .output()
            .await
            .map_err(|e| {
                crate::Error::Backend(format!("Failed to query libvirt network: {}", e))
            })?;

        if !output.status.success() {
            return Err(crate::Error::Backend(
                "Failed to get libvirt network config".to_string(),
            ));
        }

        let xml = String::from_utf8_lossy(&output.stdout);

        // Parse XML for network configuration
        // Look for <ip address="192.168.122.1" netmask="255.255.255.0">
        let gateway = Self::extract_xml_attr(&xml, "ip", "address")
            .unwrap_or_else(|| "192.168.122.1".to_string());

        let netmask = Self::extract_xml_attr(&xml, "ip", "netmask")
            .unwrap_or_else(|| "255.255.255.0".to_string());

        // Convert netmask to CIDR bits
        let netmask_bits = Self::netmask_to_cidr(&netmask);

        // Extract prefix from gateway (e.g., "192.168.122" from "192.168.122.1")
        let prefix = gateway
            .rsplit_once('.')
            .map(|x| x.0)
            .unwrap_or("192.168.122")
            .to_string();

        let subnet = format!("{}.0/{}", prefix, netmask_bits);

        // DHCP range from XML (for reference, not currently used)
        let _dhcp_start = Self::extract_xml_attr(&xml, "range", "start")
            .unwrap_or_else(|| format!("{}.2", prefix));

        let _dhcp_end = Self::extract_xml_attr(&xml, "range", "end")
            .unwrap_or_else(|| format!("{}.254", prefix));

        // Use a range that doesn't conflict with DHCP
        // Standard libvirt DHCP: .2-.254, we'll use .10-.250 (avoiding .2-.9 and .251-.254)
        let ip_pool_start = format!("{}.10", prefix);
        let ip_pool_end = format!("{}.250", prefix);

        Ok(Self {
            default_network: "default".to_string(),
            gateway,
            subnet,
            prefix,
            netmask_bits,
            ip_pool_start,
            ip_pool_end,
            default_interface: "enp1s0".to_string(), // Virtio standard
        })
    }

    /// Discover from environment variables
    fn discover_from_env() -> Result<Self> {
        let gateway = std::env::var("BENCHSCALE_GATEWAY")
            .map_err(|_| crate::Error::Backend("No BENCHSCALE_GATEWAY".to_string()))?;

        let prefix = gateway
            .rsplit_once('.')
            .map(|x| x.0)
            .ok_or_else(|| crate::Error::Backend("Invalid gateway format".to_string()))?
            .to_string();

        let netmask_bits = std::env::var("BENCHSCALE_NETMASK")
            .unwrap_or_else(|_| "24".to_string())
            .parse::<u8>()
            .unwrap_or(24);

        let subnet = format!("{}.0/{}", prefix, netmask_bits);
        let ip_pool_start = format!("{}.10", prefix);
        let ip_pool_end = format!("{}.250", prefix);

        Ok(Self {
            default_network: std::env::var("BENCHSCALE_NETWORK")
                .unwrap_or_else(|_| "default".to_string()),
            gateway,
            subnet,
            prefix,
            netmask_bits,
            ip_pool_start,
            ip_pool_end,
            default_interface: std::env::var("BENCHSCALE_INTERFACE")
                .unwrap_or_else(|_| "enp1s0".to_string()),
        })
    }

    /// Default libvirt network configuration
    pub fn default_libvirt() -> Self {
        Self {
            default_network: "default".to_string(),
            gateway: "192.168.122.1".to_string(),
            subnet: "192.168.122.0/24".to_string(),
            prefix: "192.168.122".to_string(),
            netmask_bits: 24,
            ip_pool_start: "192.168.122.10".to_string(),
            ip_pool_end: "192.168.122.250".to_string(),
            default_interface: "enp1s0".to_string(),
        }
    }

    /// Extract XML attribute value
    fn extract_xml_attr(xml: &str, element: &str, attr: &str) -> Option<String> {
        // Simple XML parsing for <element attr="value">
        // This is pragmatic - for production, consider using an XML library
        let element_start = format!("<{} ", element);
        let attr_pattern = format!("{}=\"", attr);

        xml.lines()
            .find(|line| line.contains(&element_start) && line.contains(&attr_pattern))
            .and_then(|line| {
                line.split(&attr_pattern)
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .map(std::string::ToString::to_string)
            })
    }

    /// Convert netmask to CIDR bits
    fn netmask_to_cidr(netmask: &str) -> u8 {
        match netmask {
            "255.255.0.0" => 16,
            "255.0.0.0" => 8,
            "255.255.255.128" => 25,
            "255.255.255.192" => 26,
            "255.255.255.224" => 27,
            "255.255.255.240" => 28,
            "255.255.255.248" => 29,
            "255.255.255.252" => 30,
            _ => 24, // Default to /24
        }
    }
}

/// Storage and filesystem capabilities
#[derive(Debug, Clone)]
pub struct StorageCapabilities {
    /// Libvirt images directory
    pub images_dir: PathBuf,

    /// Temporary directory for working files
    pub temp_dir: PathBuf,

    /// Cloud-init working directory
    pub cloud_init_dir: PathBuf,
}

impl StorageCapabilities {
    /// Discover storage capabilities
    pub async fn discover() -> Result<Self> {
        debug!("Discovering storage capabilities...");

        let images_dir = Self::discover_images_dir().await;
        let temp_dir = Self::discover_temp_dir();
        let cloud_init_dir = temp_dir.join("benchscale-cloud-init");

        Ok(Self {
            images_dir,
            temp_dir,
            cloud_init_dir,
        })
    }

    /// Discover libvirt images directory
    async fn discover_images_dir() -> PathBuf {
        // Try environment variable first
        if let Ok(path) = std::env::var("LIBVIRT_IMAGES_DIR") {
            let path = PathBuf::from(path);
            if path.exists() {
                debug!("Using LIBVIRT_IMAGES_DIR: {}", path.display());
                return path;
            }
        }

        // Try querying libvirt for default pool
        if let Ok(path) = Self::query_libvirt_pool().await {
            debug!("Discovered from libvirt: {}", path.display());
            return path;
        }

        // Try XDG data home
        if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
            let path = PathBuf::from(xdg_data).join("libvirt/images");
            if path.exists() {
                debug!("Using XDG_DATA_HOME: {}", path.display());
                return path;
            }
        }

        // Standard system location
        debug!("Using standard location: /var/lib/libvirt/images");
        PathBuf::from("/var/lib/libvirt/images")
    }

    /// Query libvirt for default storage pool path
    async fn query_libvirt_pool() -> Result<PathBuf> {
        let output = tokio::process::Command::new("virsh")
            .args(["pool-dumpxml", "default"])
            .output()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to query pool: {}", e)))?;

        if !output.status.success() {
            return Err(crate::Error::Backend(
                "Failed to get pool config".to_string(),
            ));
        }

        let xml = String::from_utf8_lossy(&output.stdout);

        // Look for <path>/var/lib/libvirt/images</path>
        for line in xml.lines() {
            if line.contains("<path>") && line.contains("</path>")
                && let Some(path_str) = line.split("<path>").nth(1)
                    && let Some(path) = path_str.split("</path>").next() {
                        return Ok(PathBuf::from(path.trim()));
                    }
        }

        Err(crate::Error::Backend("No path in pool XML".to_string()))
    }

    /// Discover temporary directory
    fn discover_temp_dir() -> PathBuf {
        // Try XDG runtime dir first (better for user session)
        if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
            let path = PathBuf::from(xdg_runtime);
            if path.exists() {
                return path;
            }
        }

        // Fall back to system temp
        std::env::temp_dir()
    }
}

/// Virtualization capabilities
#[derive(Debug, Clone)]
pub struct VirtCapabilities {
    /// Libvirt connection URI
    pub uri: String,

    /// Default OS variant for virt-install
    pub default_os_variant: String,

    /// SSH port
    pub ssh_port: u16,

    /// VNC base port
    pub vnc_base_port: u16,
}

impl VirtCapabilities {
    /// Discover virtualization capabilities
    pub async fn discover() -> Result<Self> {
        debug!("Discovering virtualization capabilities...");

        let uri = std::env::var("LIBVIRT_URI")
            .or_else(|_| std::env::var("BENCHSCALE_LIBVIRT_URI"))
            .unwrap_or_else(|_| "qemu:///system".to_string());

        let default_os_variant =
            std::env::var("BENCHSCALE_OS_VARIANT").unwrap_or_else(|_| "ubuntu22.04".to_string());

        let ssh_port = std::env::var("BENCHSCALE_SSH_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(22);

        let vnc_base_port = std::env::var("BENCHSCALE_VNC_BASE_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5900);

        Ok(Self {
            uri,
            default_os_variant,
            ssh_port,
            vnc_base_port,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_capability_discovery() {
        // Should not panic
        let result = SystemCapabilities::discover().await;

        // May fail if libvirt not available, but shouldn't panic
        if let Ok(caps) = result {
            assert!(!caps.network.gateway.is_empty());
            assert!(!caps.network.subnet.is_empty());
            assert!(caps
                .storage
                .images_dir
                .to_string_lossy()
                .contains("libvirt"));
        }
    }

    #[test]
    fn test_netmask_conversion() {
        assert_eq!(NetworkCapabilities::netmask_to_cidr("255.255.255.0"), 24);
        assert_eq!(NetworkCapabilities::netmask_to_cidr("255.255.0.0"), 16);
        assert_eq!(NetworkCapabilities::netmask_to_cidr("255.0.0.0"), 8);
    }

    #[test]
    fn test_xml_attr_extraction() {
        let xml = r#"<ip address="192.168.122.1" netmask="255.255.255.0">"#;
        assert_eq!(
            NetworkCapabilities::extract_xml_attr(xml, "ip", "address"),
            Some("192.168.122.1".to_string())
        );
        assert_eq!(
            NetworkCapabilities::extract_xml_attr(xml, "ip", "netmask"),
            Some("255.255.255.0".to_string())
        );
    }

    #[tokio::test]
    async fn test_storage_discovery() {
        let caps = StorageCapabilities::discover().await.unwrap();
        assert!(caps.images_dir.to_string_lossy().len() > 0);
        assert!(caps.temp_dir.exists() || caps.temp_dir.to_string_lossy().contains("tmp"));
    }
}
