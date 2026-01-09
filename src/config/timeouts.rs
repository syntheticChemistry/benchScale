//! Timeout Configuration
//!
//! **Phase 2: Configuration Externalization**
//!
//! This module provides type-safe, configurable timeout values for all
//! operations in benchScale. All timeouts can be:
//! - Set via configuration file (YAML/TOML)
//! - Overridden via environment variables
//! - Given sensible defaults
//!
//! # Philosophy
//! - **Capability-based**: No hardcoded assumptions
//! - **Fractal**: Same config pattern at all scales
//! - **Type-safe**: Compile-time validation

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Timeout configuration for all benchScale operations
///
/// # Examples
///
/// ```rust
/// use benchscale::config::TimeoutConfig;
///
/// // Use defaults
/// let config = TimeoutConfig::default();
/// assert_eq!(config.cloud_init_secs, 1800); // 30 min
///
/// // Create custom config
/// let config = TimeoutConfig {
///     cloud_init_secs: 3600, // 1 hour
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeoutConfig {
    /// Cloud-init completion timeout (seconds)
    ///
    /// **Default**: 1800s (30 minutes)
    ///
    /// This should be generous for complex desktop installations that
    /// include large package downloads (Evolution #21).
    #[serde(default = "default_cloud_init_timeout")]
    pub cloud_init_secs: u64,

    /// DHCP IP discovery timeout (seconds)
    ///
    /// **Default**: 60s (1 minute)
    ///
    /// How long to wait for libvirt DHCP to assign an IP address
    /// to a newly created VM (Evolution #22).
    #[serde(default = "default_dhcp_discovery_timeout")]
    pub dhcp_discovery_secs: u64,

    /// VM boot timeout (seconds)
    ///
    /// **Default**: 600s (10 minutes)
    ///
    /// Maximum time to wait for a VM to become SSH-accessible
    /// after initial creation or reboot (Evolution #9).
    #[serde(default = "default_vm_boot_timeout")]
    pub vm_boot_secs: u64,

    /// SSH connection timeout (seconds)
    ///
    /// **Default**: 30s
    ///
    /// Individual SSH connection attempt timeout.
    #[serde(default = "default_ssh_timeout")]
    pub ssh_connection_secs: u64,

    /// Network ping timeout (seconds)
    ///
    /// **Default**: 5s
    ///
    /// How long to wait for ICMP ping response.
    #[serde(default = "default_ping_timeout")]
    pub ping_timeout_secs: u64,

    /// Post-boot step timeout (seconds)
    ///
    /// **Default**: 600s (10 minutes)
    ///
    /// Maximum time for a single post-boot installation step.
    #[serde(default = "default_post_boot_step_timeout")]
    pub post_boot_step_secs: u64,

    /// VM reboot timeout (seconds)
    ///
    /// **Default**: 600s (10 minutes)
    ///
    /// Maximum time to wait for VM to come back after reboot.
    #[serde(default = "default_reboot_timeout")]
    pub reboot_timeout_secs: u64,
}

// Default value functions (required by serde)
fn default_cloud_init_timeout() -> u64 {
    1800
} // 30 min
fn default_dhcp_discovery_timeout() -> u64 {
    60
}
fn default_vm_boot_timeout() -> u64 {
    600
} // 10 min
fn default_ssh_timeout() -> u64 {
    30
}
fn default_ping_timeout() -> u64 {
    5
}
fn default_post_boot_step_timeout() -> u64 {
    600
} // 10 min
fn default_reboot_timeout() -> u64 {
    600
} // 10 min

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            cloud_init_secs: default_cloud_init_timeout(),
            dhcp_discovery_secs: default_dhcp_discovery_timeout(),
            vm_boot_secs: default_vm_boot_timeout(),
            ssh_connection_secs: default_ssh_timeout(),
            ping_timeout_secs: default_ping_timeout(),
            post_boot_step_secs: default_post_boot_step_timeout(),
            reboot_timeout_secs: default_reboot_timeout(),
        }
    }
}

impl TimeoutConfig {
    /// Convert cloud-init timeout to Duration
    pub fn cloud_init(&self) -> Duration {
        Duration::from_secs(self.cloud_init_secs)
    }

    /// Convert DHCP discovery timeout to Duration
    pub fn dhcp_discovery(&self) -> Duration {
        Duration::from_secs(self.dhcp_discovery_secs)
    }

    /// Convert VM boot timeout to Duration
    pub fn vm_boot(&self) -> Duration {
        Duration::from_secs(self.vm_boot_secs)
    }

    /// Convert SSH connection timeout to Duration
    pub fn ssh_connection(&self) -> Duration {
        Duration::from_secs(self.ssh_connection_secs)
    }

    /// Convert ping timeout to Duration
    pub fn ping_timeout(&self) -> Duration {
        Duration::from_secs(self.ping_timeout_secs)
    }

    /// Convert post-boot step timeout to Duration
    pub fn post_boot_step(&self) -> Duration {
        Duration::from_secs(self.post_boot_step_secs)
    }

    /// Convert reboot timeout to Duration
    pub fn reboot_timeout(&self) -> Duration {
        Duration::from_secs(self.reboot_timeout_secs)
    }

    /// Apply environment variable overrides
    ///
    /// **Phase 3B: Environment Variable Support**
    ///
    /// Overrides configuration values from environment variables with prefix `BENCHSCALE_`.
    ///
    /// # Environment Variables
    ///
    /// - `BENCHSCALE_CLOUD_INIT_TIMEOUT` - Cloud-init timeout in seconds
    /// - `BENCHSCALE_DHCP_DISCOVERY_TIMEOUT` - DHCP discovery timeout in seconds
    /// - `BENCHSCALE_VM_BOOT_TIMEOUT` - VM boot timeout in seconds
    /// - `BENCHSCALE_SSH_TIMEOUT` - SSH connection timeout in seconds
    /// - `BENCHSCALE_PING_TIMEOUT` - Ping timeout in seconds
    /// - `BENCHSCALE_POST_BOOT_STEP_TIMEOUT` - Post-boot step timeout in seconds
    /// - `BENCHSCALE_REBOOT_TIMEOUT` - Reboot timeout in seconds
    ///
    /// # Example
    ///
    /// ```rust
    /// use benchscale::config::TimeoutConfig;
    ///
    /// std::env::set_var("BENCHSCALE_CLOUD_INIT_TIMEOUT", "3600");
    /// let mut config = TimeoutConfig::default();
    /// config.apply_env_overrides();
    /// assert_eq!(config.cloud_init_secs, 3600);
    /// ```
    pub fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("BENCHSCALE_CLOUD_INIT_TIMEOUT") {
            if let Ok(secs) = val.parse::<u64>() {
                self.cloud_init_secs = secs;
            }
        }

        if let Ok(val) = std::env::var("BENCHSCALE_DHCP_DISCOVERY_TIMEOUT") {
            if let Ok(secs) = val.parse::<u64>() {
                self.dhcp_discovery_secs = secs;
            }
        }

        if let Ok(val) = std::env::var("BENCHSCALE_VM_BOOT_TIMEOUT") {
            if let Ok(secs) = val.parse::<u64>() {
                self.vm_boot_secs = secs;
            }
        }

        if let Ok(val) = std::env::var("BENCHSCALE_SSH_TIMEOUT") {
            if let Ok(secs) = val.parse::<u64>() {
                self.ssh_connection_secs = secs;
            }
        }

        if let Ok(val) = std::env::var("BENCHSCALE_PING_TIMEOUT") {
            if let Ok(secs) = val.parse::<u64>() {
                self.ping_timeout_secs = secs;
            }
        }

        if let Ok(val) = std::env::var("BENCHSCALE_POST_BOOT_STEP_TIMEOUT") {
            if let Ok(secs) = val.parse::<u64>() {
                self.post_boot_step_secs = secs;
            }
        }

        if let Ok(val) = std::env::var("BENCHSCALE_REBOOT_TIMEOUT") {
            if let Ok(secs) = val.parse::<u64>() {
                self.reboot_timeout_secs = secs;
            }
        }
    }

    /// Validate configuration values
    ///
    /// Returns an error if any timeout is zero or unreasonably large.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.cloud_init_secs == 0 {
            anyhow::bail!("cloud_init_secs must be > 0");
        }
        if self.dhcp_discovery_secs == 0 {
            anyhow::bail!("dhcp_discovery_secs must be > 0");
        }
        if self.vm_boot_secs == 0 {
            anyhow::bail!("vm_boot_secs must be > 0");
        }
        if self.ssh_connection_secs == 0 {
            anyhow::bail!("ssh_connection_secs must be > 0");
        }
        if self.ping_timeout_secs == 0 {
            anyhow::bail!("ping_timeout_secs must be > 0");
        }
        if self.post_boot_step_secs == 0 {
            anyhow::bail!("post_boot_step_secs must be > 0");
        }
        if self.reboot_timeout_secs == 0 {
            anyhow::bail!("reboot_timeout_secs must be > 0");
        }

        // Sanity check: no timeout should exceed 24 hours
        const MAX_TIMEOUT: u64 = 86400; // 24 hours
        if self.cloud_init_secs > MAX_TIMEOUT {
            anyhow::bail!("cloud_init_secs exceeds 24 hours");
        }
        if self.vm_boot_secs > MAX_TIMEOUT {
            anyhow::bail!("vm_boot_secs exceeds 24 hours");
        }
        if self.post_boot_step_secs > MAX_TIMEOUT {
            anyhow::bail!("post_boot_step_secs exceeds 24 hours");
        }
        if self.reboot_timeout_secs > MAX_TIMEOUT {
            anyhow::bail!("reboot_timeout_secs exceeds 24 hours");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = TimeoutConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_default_values() {
        let config = TimeoutConfig::default();
        assert_eq!(config.cloud_init_secs, 1800); // 30 min
        assert_eq!(config.dhcp_discovery_secs, 60);
        assert_eq!(config.vm_boot_secs, 600); // 10 min
        assert_eq!(config.ssh_connection_secs, 30);
        assert_eq!(config.ping_timeout_secs, 5);
        assert_eq!(config.post_boot_step_secs, 600);
        assert_eq!(config.reboot_timeout_secs, 600);
    }

    #[test]
    fn test_duration_conversions() {
        let config = TimeoutConfig::default();
        assert_eq!(config.cloud_init(), Duration::from_secs(1800));
        assert_eq!(config.dhcp_discovery(), Duration::from_secs(60));
        assert_eq!(config.vm_boot(), Duration::from_secs(600));
        assert_eq!(config.ssh_connection(), Duration::from_secs(30));
        assert_eq!(config.ping_timeout(), Duration::from_secs(5));
        assert_eq!(config.post_boot_step(), Duration::from_secs(600));
        assert_eq!(config.reboot_timeout(), Duration::from_secs(600));
    }

    #[test]
    fn test_custom_values() {
        let config = TimeoutConfig {
            cloud_init_secs: 3600, // 1 hour
            ..Default::default()
        };
        assert_eq!(config.cloud_init_secs, 3600);
        assert_eq!(config.dhcp_discovery_secs, 60); // Still default
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_rejects_zero() {
        let config = TimeoutConfig {
            cloud_init_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_excessive() {
        let config = TimeoutConfig {
            cloud_init_secs: 100000, // > 24 hours
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_serde_yaml_serialization() {
        let config = TimeoutConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("cloud_init_secs"));
        assert!(yaml.contains("1800"));
    }

    #[test]
    fn test_serde_yaml_deserialization() {
        let yaml = r#"
cloud_init_secs: 3600
dhcp_discovery_secs: 120
"#;
        let config: TimeoutConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.cloud_init_secs, 3600);
        assert_eq!(config.dhcp_discovery_secs, 120);
        // Others should use defaults
        assert_eq!(config.vm_boot_secs, 600);
    }

    #[test]
    fn test_partial_deserialization_uses_defaults() {
        let yaml = r#"
cloud_init_secs: 7200
"#;
        let config: TimeoutConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.cloud_init_secs, 7200);
        // All others should be defaults
        assert_eq!(config.dhcp_discovery_secs, 60);
        assert_eq!(config.vm_boot_secs, 600);
        assert_eq!(config.ssh_connection_secs, 30);
    }
}

