// SPDX-License-Identifier: AGPL-3.0-or-later
//! Network Configuration
//!
//! **Phase 2C: Configuration Externalization**
//!
//! Configuration for network discovery, DHCP, and connectivity settings.
//!
//! # Philosophy
//! - **Runtime Discovery**: Discover DHCP ranges and interfaces at runtime
//! - **Capability-Based**: Request what you need, discover what's available
//! - **Agnostic**: No assumptions about network topology

use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// Network configuration for VM connectivity and discovery
///
/// # Examples
///
/// ```rust
/// use benchscale::config::NetworkConfig;
///
/// // Use defaults (auto-discovery)
/// let config = NetworkConfig::default();
/// assert_eq!(config.network_name, "default");
/// assert_eq!(config.ssh_port, 22);
///
/// // Explicit configuration
/// let config = NetworkConfig {
///     network_name: "vmnet".to_string(),
///     ssh_port: 2222,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Libvirt network name
    ///
    /// **Default**: "default"
    ///
    /// The name of the libvirt network to use for VMs.
    /// This is typically "default" but can be customized for
    /// isolated network environments.
    #[serde(default = "default_network_name")]
    pub network_name: String,

    /// DHCP IP range
    ///
    /// **Default**: None (discovered at runtime)
    ///
    /// **Evolution #22**: DHCP discovery from libvirt.
    /// If not specified, will be discovered from the libvirt network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp_range: Option<DhcpRange>,

    /// DNS servers
    ///
    /// **Default**: Empty (use network defaults)
    ///
    /// DNS servers to configure in VMs. If empty, uses the
    /// network's default DNS configuration.
    #[serde(default)]
    pub dns_servers: Vec<IpAddr>,

    /// SSH port
    ///
    /// **Default**: 22
    ///
    /// Port to use for SSH connections to VMs.
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,

    /// Network interface to use
    ///
    /// **Default**: None (discovered at runtime)
    ///
    /// If specified, use this specific network interface.
    /// Otherwise, discover from SystemCapabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,

    /// Enable DHCP discovery
    ///
    /// **Default**: true
    ///
    /// **Evolution #22**: Enable MAC-based DHCP IP discovery.
    #[serde(default = "default_enable_dhcp_discovery")]
    pub enable_dhcp_discovery: bool,

    /// DHCP discovery timeout (seconds)
    ///
    /// **Default**: 60s
    ///
    /// How long to wait for DHCP to assign an IP address.
    #[serde(default = "default_dhcp_timeout")]
    pub dhcp_discovery_timeout_secs: u64,
}

/// DHCP range configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DhcpRange {
    /// Start IP address
    pub start: IpAddr,
    /// End IP address
    pub end: IpAddr,
    /// Network mask (e.g., "255.255.255.0" or "24")
    pub netmask: String,
}

// Default value functions
fn default_network_name() -> String {
    "default".to_string()
}
fn default_ssh_port() -> u16 {
    22
}
fn default_enable_dhcp_discovery() -> bool {
    true
}
fn default_dhcp_timeout() -> u64 {
    60
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            network_name: default_network_name(),
            dhcp_range: None, // Discovered at runtime
            dns_servers: Vec::new(),
            ssh_port: default_ssh_port(),
            interface: None, // Discovered at runtime
            enable_dhcp_discovery: default_enable_dhcp_discovery(),
            dhcp_discovery_timeout_secs: default_dhcp_timeout(),
        }
    }
}

impl NetworkConfig {
    /// Convert DHCP timeout to Duration
    pub fn dhcp_discovery_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.dhcp_discovery_timeout_secs)
    }

    /// Validate network configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        // Network name validation
        if self.network_name.is_empty() {
            anyhow::bail!("network_name cannot be empty");
        }
        if self.network_name.len() > 255 {
            anyhow::bail!("network_name exceeds 255 characters");
        }

        // SSH port validation
        if self.ssh_port == 0 {
            anyhow::bail!("ssh_port cannot be 0");
        }

        // DHCP timeout validation
        if self.dhcp_discovery_timeout_secs == 0 {
            anyhow::bail!("dhcp_discovery_timeout_secs must be > 0");
        }
        if self.dhcp_discovery_timeout_secs > 600 {
            anyhow::bail!("dhcp_discovery_timeout_secs > 10min is unreasonably long");
        }

        // DHCP range validation (if specified)
        if let Some(ref range) = self.dhcp_range {
            // Validate that start and end are same address family
            match (&range.start, &range.end) {
                (IpAddr::V4(_), IpAddr::V4(_)) | (IpAddr::V6(_), IpAddr::V6(_)) => {}
                _ => anyhow::bail!("DHCP range start and end must be same address family"),
            }

            // Validate netmask is not empty
            if range.netmask.is_empty() {
                anyhow::bail!("DHCP range netmask cannot be empty");
            }
        }

        // DNS servers validation
        if self.dns_servers.len() > 10 {
            anyhow::bail!("More than 10 DNS servers is unreasonable");
        }

        Ok(())
    }

    /// Check if DHCP discovery is enabled
    pub fn should_discover_dhcp(&self) -> bool {
        self.enable_dhcp_discovery && self.dhcp_range.is_none()
    }

    /// Check if network interface should be discovered
    pub fn should_discover_interface(&self) -> bool {
        self.interface.is_none()
    }

    /// Merge with discovered network capabilities
    ///
    /// **Phase 3A: SystemCapabilities Integration**
    ///
    /// Priority: explicit config > discovered > defaults
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use benchscale::config::NetworkConfig;
    /// # use benchscale::capabilities::NetworkCapabilities;
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut config = NetworkConfig::default();
    /// let capabilities = NetworkCapabilities::discover().await?;
    /// config.merge_with_capabilities(&capabilities);
    /// // Now config has discovered values where not explicitly set
    /// # Ok(())
    /// # }
    /// ```
    pub fn merge_with_capabilities(
        &mut self,
        capabilities: &crate::capabilities::NetworkCapabilities,
    ) {
        // If network_name is default and capabilities has a different one, use it
        if self.network_name == "default" && capabilities.default_network != "default" {
            self.network_name.clone_from(&capabilities.default_network);
        }

        // If DHCP range not set, populate from capabilities
        if self.dhcp_range.is_none() {
            use std::net::IpAddr;
            use std::str::FromStr;

            if let (Ok(start), Ok(end)) = (
                IpAddr::from_str(&capabilities.ip_pool_start),
                IpAddr::from_str(&capabilities.ip_pool_end),
            ) {
                self.dhcp_range = Some(DhcpRange {
                    start,
                    end,
                    netmask: format!("{}", capabilities.netmask_bits),
                });
            }
        }

        // If interface not set, use discovered
        if self.interface.is_none() {
            self.interface = Some(capabilities.default_interface.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = NetworkConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_default_values() {
        let config = NetworkConfig::default();
        assert_eq!(config.network_name, "default");
        assert_eq!(config.ssh_port, 22);
        assert_eq!(config.dhcp_discovery_timeout_secs, 60);
        assert!(config.enable_dhcp_discovery);
        assert!(config.dhcp_range.is_none());
        assert!(config.interface.is_none());
        assert!(config.dns_servers.is_empty());
    }

    #[test]
    fn test_dhcp_timeout_conversion() {
        let config = NetworkConfig::default();
        assert_eq!(
            config.dhcp_discovery_timeout(),
            std::time::Duration::from_secs(60)
        );
    }

    #[test]
    fn test_custom_values() {
        let config = NetworkConfig {
            network_name: "vmnet".to_string(),
            ssh_port: 2222,
            dhcp_discovery_timeout_secs: 120,
            ..Default::default()
        };
        assert_eq!(config.network_name, "vmnet");
        assert_eq!(config.ssh_port, 2222);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_rejects_empty_network_name() {
        let config = NetworkConfig {
            network_name: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_zero_ssh_port() {
        let config = NetworkConfig {
            ssh_port: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_zero_dhcp_timeout() {
        let config = NetworkConfig {
            dhcp_discovery_timeout_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_excessive_dhcp_timeout() {
        let config = NetworkConfig {
            dhcp_discovery_timeout_secs: 1000, // > 10 min
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_dhcp_range_validation() {
        use std::str::FromStr;

        let config = NetworkConfig {
            dhcp_range: Some(DhcpRange {
                start: IpAddr::from_str("192.168.122.10").unwrap(),
                end: IpAddr::from_str("192.168.122.100").unwrap(),
                netmask: "255.255.255.0".to_string(),
            }),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_dhcp_range_mixed_address_family_rejected() {
        use std::str::FromStr;

        let config = NetworkConfig {
            dhcp_range: Some(DhcpRange {
                start: IpAddr::from_str("192.168.122.10").unwrap(),
                end: IpAddr::from_str("::1").unwrap(), // IPv6!
                netmask: "255.255.255.0".to_string(),
            }),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_should_discover_dhcp() {
        let config = NetworkConfig::default();
        assert!(config.should_discover_dhcp()); // No range specified

        let config = NetworkConfig {
            enable_dhcp_discovery: false,
            ..Default::default()
        };
        assert!(!config.should_discover_dhcp()); // Disabled

        use std::str::FromStr;
        let config = NetworkConfig {
            dhcp_range: Some(DhcpRange {
                start: IpAddr::from_str("192.168.122.10").unwrap(),
                end: IpAddr::from_str("192.168.122.100").unwrap(),
                netmask: "255.255.255.0".to_string(),
            }),
            ..Default::default()
        };
        assert!(!config.should_discover_dhcp()); // Range specified
    }

    #[test]
    fn test_should_discover_interface() {
        let config = NetworkConfig::default();
        assert!(config.should_discover_interface()); // Not specified

        let config = NetworkConfig {
            interface: Some("virbr0".to_string()),
            ..Default::default()
        };
        assert!(!config.should_discover_interface()); // Specified
    }

    #[test]
    fn test_serde_yaml_serialization() {
        let config = NetworkConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("network_name"));
        assert!(yaml.contains("default"));
    }

    #[test]
    fn test_serde_yaml_deserialization() {
        let yaml = r#"
network_name: "vmnet"
ssh_port: 2222
enable_dhcp_discovery: false
"#;
        let config: NetworkConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.network_name, "vmnet");
        assert_eq!(config.ssh_port, 2222);
        assert!(!config.enable_dhcp_discovery);
        // Others should use defaults
        assert_eq!(config.dhcp_discovery_timeout_secs, 60);
    }

    #[test]
    fn test_dns_servers() {
        use std::str::FromStr;

        let config = NetworkConfig {
            dns_servers: vec![
                IpAddr::from_str("8.8.8.8").unwrap(),
                IpAddr::from_str("8.8.4.4").unwrap(),
            ],
            ..Default::default()
        };
        assert!(config.validate().is_ok());
        assert_eq!(config.dns_servers.len(), 2);
    }

    #[test]
    fn test_validation_rejects_too_many_dns_servers() {
        use std::str::FromStr;

        let config = NetworkConfig {
            dns_servers: (0..15)
                .map(|i| IpAddr::from_str(&format!("192.168.1.{}", i)).unwrap())
                .collect(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_evolution_22_dhcp_discovery() {
        // Evolution #22: DHCP discovery enabled by default
        let config = NetworkConfig::default();
        assert!(config.enable_dhcp_discovery);
        assert_eq!(config.dhcp_discovery_timeout_secs, 60);
    }
}
