// SPDX-License-Identifier: AGPL-3.0-only
//! Configuration Module
//!
//! **Phase 2: Configuration Externalization**
//!
//! This module provides a comprehensive, type-safe configuration system for benchScale.
//!
//! # Architecture
//!
//! Configuration follows a layered approach:
//!
//! 1. **System Capabilities** (Runtime Discovery)
//!    - Detected at startup from environment
//!    - Paths, network interfaces, resources
//!
//! 2. **Static Configuration** (File-based)
//!    - YAML/TOML config files
//!    - Timeouts, limits, defaults
//!
//! 3. **Environment Overrides** (Highest Priority)
//!    - Environment variables
//!    - Runtime adjustments
//!
//! 4. **Resolved Configuration** (Merged)
//!    - Final configuration used by components
//!    - Validated, type-safe, ready to use
//!
//! # Philosophy
//!
//! - **Runtime Discovery**: Discover capabilities, don't assume
//! - **Fractal Patterns**: Same config structure at all scales
//! - **Type Safety**: Strongly-typed, validated at load-time
//! - **Zero Hardcoding**: All values configurable or discovered
//!
//! # Examples
//!
//! ## Use Defaults (Zero Config)
//!
//! ```rust
//! use benchscale::config::BenchScaleConfig;
//!
//! // Just works - sensible defaults
//! let config = BenchScaleConfig::default();
//! ```
//!
//! ## Load from File
//!
//! ```rust,no_run
//! use benchscale::config::BenchScaleConfig;
//!
//! let config = BenchScaleConfig::from_file("benchscale.yaml")?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Programmatic Configuration
//!
//! ```rust
//! use benchscale::config::{BenchScaleConfig, TimeoutConfig};
//!
//! let config = BenchScaleConfig {
//!     timeouts: TimeoutConfig {
//!         cloud_init_secs: 3600, // 1 hour
//!         ..Default::default()
//!     },
//!     ..Default::default()
//! };
//! ```

pub mod timeouts;
pub mod monitoring;
pub mod network;
pub mod storage;

pub use timeouts::TimeoutConfig;
pub use monitoring::MonitoringConfig;
pub use network::{DhcpRange, NetworkConfig};
pub use storage::StorageConfig;

use serde::{Deserialize, Serialize};

/// Main benchScale configuration
///
/// This is the top-level configuration struct that contains all
/// configuration categories.
///
/// # Examples
///
/// ```rust
/// use benchscale::config::BenchScaleConfig;
///
/// // Use defaults
/// let config = BenchScaleConfig::default();
/// assert!(config.validate().is_ok());
///
/// // Customize timeouts
/// let mut config = BenchScaleConfig::default();
/// config.timeouts.cloud_init_secs = 3600; // 1 hour
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchScaleConfig {
    /// Timeout settings
    #[serde(default)]
    pub timeouts: TimeoutConfig,

    /// Monitoring settings
    #[serde(default)]
    pub monitoring: MonitoringConfig,

    /// Network settings (Phase 2C)
    #[serde(default)]
    pub network: NetworkConfig,

    /// Storage settings (Phase 2C)
    #[serde(default)]
    pub storage: StorageConfig,
}

impl BenchScaleConfig {
    /// Storage settings ([`StorageConfig::vm_images_dir_or_default`] / [`StorageConfig::images_dir`]).
    #[must_use]
    pub fn storage(&self) -> &StorageConfig {
        &self.storage
    }

    /// Load configuration from a YAML file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use benchscale::config::BenchScaleConfig;
    ///
    /// let config = BenchScaleConfig::from_file("benchscale.yaml")?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn from_file(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path.as_ref().display(), e))?;
        
        let config: Self = serde_yaml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file {}: {}", path.as_ref().display(), e))?;
        
        config.validate()?;
        Ok(config)
    }

    /// Save configuration to a YAML file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use benchscale::config::BenchScaleConfig;
    ///
    /// let config = BenchScaleConfig::default();
    /// config.to_file("benchscale.yaml")?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn to_file(&self, path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
        self.validate()?;
        
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config: {}", e))?;
        
        std::fs::write(path.as_ref(), yaml)
            .map_err(|e| anyhow::anyhow!("Failed to write config file {}: {}", path.as_ref().display(), e))?;
        
        Ok(())
    }

    /// Validate all configuration sections
    ///
    /// Returns an error if any section has invalid values.
    pub fn validate(&self) -> anyhow::Result<()> {
        self.timeouts
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid timeout config: {}", e))?;
        
        self.monitoring
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid monitoring config: {}", e))?;
        
        self.network
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid network config: {}", e))?;
        
        self.storage
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid storage config: {}", e))?;
        
        Ok(())
    }

    /// Merge with system capabilities
    ///
    /// **Phase 3A: SystemCapabilities Integration**
    ///
    /// This method fills in any unspecified configuration values with
    /// runtime-discovered capabilities. Priority order:
    ///
    /// 1. Explicit configuration (highest priority)
    /// 2. Discovered capabilities
    /// 3. Defaults (fallback)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use benchscale::config::BenchScaleConfig;
    /// # use benchscale::capabilities::SystemCapabilities;
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut config = BenchScaleConfig::default();
    /// let capabilities = SystemCapabilities::discover().await?;
    /// config.merge_with_capabilities(&capabilities);
    /// // Now config has discovered values for network and storage
    /// # Ok(())
    /// # }
    /// ```
    pub fn merge_with_capabilities(&mut self, capabilities: &crate::capabilities::SystemCapabilities) {
        // Merge network configuration
        self.network.merge_with_capabilities(&capabilities.network);

        // Merge storage configuration
        self.storage.merge_with_capabilities(&capabilities.storage);

        // Timeouts and monitoring don't need merging - they're fully configurable
    }

    // NOTE: Optional future enhancement — `from_env()` with prefix `BENCHSCALE_` if needed.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = BenchScaleConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_default_values() {
        let config = BenchScaleConfig::default();
        assert_eq!(config.timeouts.cloud_init_secs, 1800);
        assert_eq!(config.monitoring.max_failures, 10);
    }

    #[test]
    fn test_serde_yaml_round_trip() {
        let original = BenchScaleConfig::default();
        let yaml = serde_yaml::to_string(&original).unwrap();
        let deserialized: BenchScaleConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_partial_yaml_uses_defaults() {
        let yaml = r#"
timeouts:
  cloud_init_secs: 3600
"#;
        let config: BenchScaleConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.timeouts.cloud_init_secs, 3600);
        // Others should be defaults
        assert_eq!(config.timeouts.dhcp_discovery_secs, 60);
        assert_eq!(config.monitoring.max_failures, 10);
    }

    #[test]
    fn test_file_write_and_read() {
        let mut temp_file = std::env::temp_dir();
        temp_file.push("benchscale_test_config.yaml");

        // Write config
        let original = BenchScaleConfig::default();
        original.to_file(&temp_file).unwrap();

        // Read it back
        let loaded = BenchScaleConfig::from_file(&temp_file).unwrap();
        assert_eq!(original, loaded);

        // Cleanup
        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_validation_catches_invalid_timeout() {
        let yaml = r#"
timeouts:
  cloud_init_secs: 0
"#;
        let config: BenchScaleConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_catches_invalid_monitoring() {
        let yaml = r#"
monitoring:
  max_failures: 0
"#;
        let config: BenchScaleConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_custom_configuration() {
        let config = BenchScaleConfig {
            timeouts: TimeoutConfig {
                cloud_init_secs: 7200, // 2 hours
                ..Default::default()
            },
            monitoring: MonitoringConfig {
                max_failures: 180, // 30 min tolerance
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(config.validate().is_ok());
        assert_eq!(config.timeouts.cloud_init_secs, 7200);
        assert_eq!(config.monitoring.max_failures, 180);
    }

    #[test]
    fn test_evolution_21_cloud_init_config() {
        // Evolution #21: Config for long cloud-init builds
        let config = BenchScaleConfig {
            timeouts: TimeoutConfig {
                cloud_init_secs: 1800, // 30 min
                ..Default::default()
            },
            monitoring: MonitoringConfig::for_cloud_init_packages(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
        assert_eq!(config.monitoring.failure_tolerance().as_secs(), 1800);
    }

    #[test]
    fn test_evolution_22_ip_tracking_config() {
        // Evolution #22: IP re-discovery enabled by default
        let config = BenchScaleConfig::default();
        assert!(config.monitoring.enable_ip_rediscovery);
        assert_eq!(config.monitoring.ip_rediscovery_interval, 10);
    }

    #[test]
    fn test_phase_2c_network_config() {
        // Phase 2C: NetworkConfig integration
        let config = BenchScaleConfig::default();
        assert_eq!(config.network.network_name, "default");
        assert_eq!(config.network.ssh_port, 22);
        assert!(config.network.enable_dhcp_discovery);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_phase_2c_storage_config() {
        // Phase 2C: StorageConfig integration
        let config = BenchScaleConfig::default();
        assert_eq!(config.storage.max_disk_size_gb, 100);
        assert_eq!(config.storage.min_free_space_gb, 10);
        assert!(config.storage.enable_cow);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_network_yaml_serialization() {
        let yaml = r#"
network:
  network_name: "vmnet"
  ssh_port: 2222
  enable_dhcp_discovery: false
"#;
        let config: BenchScaleConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.network.network_name, "vmnet");
        assert_eq!(config.network.ssh_port, 2222);
        assert!(!config.network.enable_dhcp_discovery);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_storage_yaml_serialization() {
        let yaml = r#"
storage:
  max_disk_size_gb: 200
  min_free_space_gb: 20
  enable_cow: false
"#;
        let config: BenchScaleConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.storage.max_disk_size_gb, 200);
        assert_eq!(config.storage.min_free_space_gb, 20);
        assert!(!config.storage.enable_cow);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_catches_invalid_network() {
        let yaml = r#"
network:
  ssh_port: 0
"#;
        let config: BenchScaleConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_catches_invalid_storage() {
        let yaml = r#"
storage:
  max_disk_size_gb: 0
"#;
        let config: BenchScaleConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_full_config_yaml() {
        let yaml = r#"
timeouts:
  cloud_init_secs: 3600
monitoring:
  max_failures: 180
network:
  network_name: "production"
  ssh_port: 22
storage:
  max_disk_size_gb: 150
  enable_cow: true
"#;
        let config: BenchScaleConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.timeouts.cloud_init_secs, 3600);
        assert_eq!(config.monitoring.max_failures, 180);
        assert_eq!(config.network.network_name, "production");
        assert_eq!(config.storage.max_disk_size_gb, 150);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_phase_3a_merge_with_capabilities() {
        use crate::capabilities::{NetworkCapabilities, StorageCapabilities, SystemCapabilities, VirtCapabilities};
        use std::path::PathBuf;

        // Create capabilities with discovered values
        let network_caps = NetworkCapabilities {
            default_network: "virbr0".to_string(),
            gateway: "192.168.100.1".to_string(),
            subnet: "192.168.100.0/24".to_string(),
            prefix: "192.168.100".to_string(),
            netmask_bits: 24,
            ip_pool_start: "192.168.100.10".to_string(),
            ip_pool_end: "192.168.100.250".to_string(),
            default_interface: "eth0".to_string(),
        };

        let storage_caps = StorageCapabilities {
            images_dir: PathBuf::from("/custom/libvirt/images"),
            temp_dir: PathBuf::from("/tmp"),
            cloud_init_dir: PathBuf::from("/tmp/cloud-init"),
        };

        let virt_caps = VirtCapabilities {
            uri: "qemu:///system".to_string(),
            default_os_variant: "ubuntu24.04".to_string(),
            ssh_port: 22,
            vnc_base_port: 5900,
        };

        let capabilities = SystemCapabilities {
            network: network_caps,
            storage: storage_caps,
            virtualization: virt_caps,
        };

        // Start with default config
        let mut config = BenchScaleConfig::default();
        assert_eq!(config.network.network_name, "default");
        assert!(config.network.interface.is_none());
        assert!(config.storage.vm_images_dir.is_none());

        // Merge with capabilities
        config.merge_with_capabilities(&capabilities);

        // Network should now have discovered values
        assert_eq!(config.network.network_name, "virbr0"); // Changed from default
        assert_eq!(config.network.interface, Some("eth0".to_string()));

        // Storage should have discovered path
        assert_eq!(config.storage.vm_images_dir, Some(PathBuf::from("/custom/libvirt/images")));
        assert_eq!(config.storage.cloud_init_dir, Some(PathBuf::from("/tmp/cloud-init")));
    }

    #[test]
    fn test_phase_3a_explicit_config_takes_priority() {
        use crate::capabilities::{NetworkCapabilities, StorageCapabilities, SystemCapabilities, VirtCapabilities};
        use std::path::PathBuf;

        // Create capabilities
        let network_caps = NetworkCapabilities {
            default_network: "virbr0".to_string(),
            gateway: "192.168.100.1".to_string(),
            subnet: "192.168.100.0/24".to_string(),
            prefix: "192.168.100".to_string(),
            netmask_bits: 24,
            ip_pool_start: "192.168.100.10".to_string(),
            ip_pool_end: "192.168.100.250".to_string(),
            default_interface: "eth0".to_string(),
        };

        let storage_caps = StorageCapabilities {
            images_dir: PathBuf::from("/custom/libvirt/images"),
            temp_dir: PathBuf::from("/tmp"),
            cloud_init_dir: PathBuf::from("/tmp/cloud-init"),
        };

        let virt_caps = VirtCapabilities {
            uri: "qemu:///system".to_string(),
            default_os_variant: "ubuntu24.04".to_string(),
            ssh_port: 22,
            vnc_base_port: 5900,
        };

        let capabilities = SystemCapabilities {
            network: network_caps,
            storage: storage_caps,
            virtualization: virt_caps,
        };

        // Start with explicit config
        let mut config = BenchScaleConfig {
            network: NetworkConfig {
                network_name: "production".to_string(), // Explicit value
                interface: Some("enp0s3".to_string()),  // Explicit value
                ..Default::default()
            },
            storage: StorageConfig {
                vm_images_dir: Some(PathBuf::from("/my/custom/path")), // Explicit value
                ..Default::default()
            },
            ..Default::default()
        };

        // Merge with capabilities
        config.merge_with_capabilities(&capabilities);

        // Explicit values should be preserved
        assert_eq!(config.network.network_name, "production"); // NOT "virbr0"
        assert_eq!(config.network.interface, Some("enp0s3".to_string())); // NOT "eth0"
        assert_eq!(config.storage.vm_images_dir, Some(PathBuf::from("/my/custom/path"))); // NOT discovered
    }
}

