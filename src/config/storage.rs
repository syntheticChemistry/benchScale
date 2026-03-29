// SPDX-License-Identifier: AGPL-3.0-only
//! Storage Configuration
//!
//! **Phase 2C: Configuration Externalization**
//!
//! Configuration for VM disk storage, base images, and intermediate saves.
//!
//! # Philosophy
//! - **Runtime Discovery**: Discover storage paths from SystemCapabilities
//! - **Capability-Based**: No hardcoded assumptions about filesystem layout
//! - **Flexible**: Support various libvirt storage configurations

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Storage configuration for VM disks and images
///
/// # Examples
///
/// ```rust
/// use benchscale::config::StorageConfig;
///
/// // Use defaults (auto-discovery)
/// let config = StorageConfig::default();
/// assert_eq!(config.max_disk_size_gb, 100);
///
/// // Explicit configuration
/// let config = StorageConfig {
///     max_disk_size_gb: 200,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageConfig {
    /// VM disk images directory
    ///
    /// **Default**: None (discovered from SystemCapabilities)
    ///
    /// The directory where VM disk images are stored.
    /// Typically `/var/lib/libvirt/images` but can vary.
    /// If not specified, will be discovered at runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_images_dir: Option<PathBuf>,

    /// Base images directory
    ///
    /// **Default**: None (discovered or defaults to vm_images_dir)
    ///
    /// Directory containing base OS images for CoW (copy-on-write) operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_images_dir: Option<PathBuf>,

    /// Intermediate saves directory
    ///
    /// **Default**: None (discovered or defaults to vm_images_dir/intermediate)
    ///
    /// Directory for saving intermediate VM states during builds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intermediate_dir: Option<PathBuf>,

    /// Cloud-init ISO directory
    ///
    /// **Default**: None (discovered or defaults to /tmp)
    ///
    /// Directory for temporary cloud-init ISO files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_init_dir: Option<PathBuf>,

    /// Maximum disk size (GB)
    ///
    /// **Default**: 100 GB
    ///
    /// Maximum size for VM disk images. Used for validation
    /// and preventing resource exhaustion.
    #[serde(default = "default_max_disk_size")]
    pub max_disk_size_gb: u64,

    /// Minimum free space required (GB)
    ///
    /// **Default**: 10 GB
    ///
    /// Minimum free space required before creating new VMs.
    /// Prevents disk exhaustion.
    #[serde(default = "default_min_free_space")]
    pub min_free_space_gb: u64,

    /// Enable CoW (copy-on-write) disks
    ///
    /// **Default**: true
    ///
    /// Use qcow2 CoW for faster VM creation and space savings.
    #[serde(default = "default_enable_cow")]
    pub enable_cow: bool,
}

// Default value functions
fn default_max_disk_size() -> u64 {
    100
}
fn default_min_free_space() -> u64 {
    10
}
fn default_enable_cow() -> bool {
    true
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            vm_images_dir: None, // Discovered from SystemCapabilities
            base_images_dir: None,
            intermediate_dir: None,
            cloud_init_dir: None,
            max_disk_size_gb: default_max_disk_size(),
            min_free_space_gb: default_min_free_space(),
            enable_cow: default_enable_cow(),
        }
    }
}

impl StorageConfig {
    /// Validate storage configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        // Max disk size validation
        if self.max_disk_size_gb == 0 {
            anyhow::bail!("max_disk_size_gb must be > 0");
        }
        if self.max_disk_size_gb > 10000 {
            // 10 TB
            anyhow::bail!("max_disk_size_gb > 10TB is unreasonably large");
        }

        // Min free space validation
        if self.min_free_space_gb == 0 {
            anyhow::bail!("min_free_space_gb must be > 0");
        }
        if self.min_free_space_gb > self.max_disk_size_gb {
            anyhow::bail!("min_free_space_gb cannot exceed max_disk_size_gb");
        }

        // Path validation (if specified)
        if let Some(ref path) = self.vm_images_dir
            && !path.is_absolute() {
                anyhow::bail!("vm_images_dir must be an absolute path");
            }
        if let Some(ref path) = self.base_images_dir
            && !path.is_absolute() {
                anyhow::bail!("base_images_dir must be an absolute path");
            }
        if let Some(ref path) = self.intermediate_dir
            && !path.is_absolute() {
                anyhow::bail!("intermediate_dir must be an absolute path");
            }
        if let Some(ref path) = self.cloud_init_dir
            && !path.is_absolute() {
                anyhow::bail!("cloud_init_dir must be an absolute path");
            }

        Ok(())
    }

    /// Check if vm_images_dir should be discovered
    pub fn should_discover_vm_images_dir(&self) -> bool {
        self.vm_images_dir.is_none()
    }

    /// Get vm_images_dir or fallback path
    pub fn vm_images_dir_or_default(&self) -> PathBuf {
        self.vm_images_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("/var/lib/libvirt/images"))
    }

    /// Get base_images_dir or fallback to vm_images_dir
    pub fn base_images_dir_or_default(&self) -> PathBuf {
        self.base_images_dir
            .clone()
            .or_else(|| self.vm_images_dir.clone())
            .unwrap_or_else(|| PathBuf::from("/var/lib/libvirt/images"))
    }

    /// Get intermediate_dir or fallback
    pub fn intermediate_dir_or_default(&self) -> PathBuf {
        self.intermediate_dir
            .clone()
            .unwrap_or_else(|| self.vm_images_dir_or_default().join("intermediate"))
    }

    /// Get cloud_init_dir or fallback to /tmp
    pub fn cloud_init_dir_or_default(&self) -> PathBuf {
        self.cloud_init_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
    }

    /// Merge with discovered storage capabilities
    ///
    /// **Phase 3A: SystemCapabilities Integration**
    ///
    /// Priority: explicit config > discovered > defaults
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use benchscale::config::StorageConfig;
    /// # use benchscale::capabilities::StorageCapabilities;
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut config = StorageConfig::default();
    /// let capabilities = StorageCapabilities::discover().await?;
    /// config.merge_with_capabilities(&capabilities);
    /// // Now config has discovered paths where not explicitly set
    /// # Ok(())
    /// # }
    /// ```
    pub fn merge_with_capabilities(&mut self, capabilities: &crate::capabilities::StorageCapabilities) {
        // If vm_images_dir not set, use discovered
        if self.vm_images_dir.is_none() {
            self.vm_images_dir = Some(capabilities.images_dir.clone());
        }

        // If base_images_dir not set, default to vm_images_dir
        // (already handled by vm_images_dir_or_default(), but set it explicitly)
        if self.base_images_dir.is_none() {
            self.base_images_dir = self.vm_images_dir.clone();
        }

        // If intermediate_dir not set, default to vm_images_dir/intermediate
        if self.intermediate_dir.is_none()
            && let Some(ref images_dir) = self.vm_images_dir {
                self.intermediate_dir = Some(images_dir.join("intermediate"));
            }

        // If cloud_init_dir not set, use discovered
        if self.cloud_init_dir.is_none() {
            self.cloud_init_dir = Some(capabilities.cloud_init_dir.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = StorageConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_default_values() {
        let config = StorageConfig::default();
        assert_eq!(config.max_disk_size_gb, 100);
        assert_eq!(config.min_free_space_gb, 10);
        assert!(config.enable_cow);
        assert!(config.vm_images_dir.is_none());
        assert!(config.base_images_dir.is_none());
        assert!(config.intermediate_dir.is_none());
        assert!(config.cloud_init_dir.is_none());
    }

    #[test]
    fn test_custom_values() {
        let config = StorageConfig {
            vm_images_dir: Some(PathBuf::from("/custom/path")),
            max_disk_size_gb: 200,
            min_free_space_gb: 20,
            enable_cow: false,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
        assert_eq!(config.max_disk_size_gb, 200);
        assert_eq!(config.min_free_space_gb, 20);
        assert!(!config.enable_cow);
    }

    #[test]
    fn test_validation_rejects_zero_max_disk_size() {
        let config = StorageConfig {
            max_disk_size_gb: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_excessive_max_disk_size() {
        let config = StorageConfig {
            max_disk_size_gb: 20000, // > 10 TB
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_zero_min_free_space() {
        let config = StorageConfig {
            min_free_space_gb: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_min_free_space_exceeding_max() {
        let config = StorageConfig {
            max_disk_size_gb: 50,
            min_free_space_gb: 100,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_relative_paths() {
        let config = StorageConfig {
            vm_images_dir: Some(PathBuf::from("relative/path")),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_should_discover_vm_images_dir() {
        let config = StorageConfig::default();
        assert!(config.should_discover_vm_images_dir());

        let config = StorageConfig {
            vm_images_dir: Some(PathBuf::from("/custom/path")),
            ..Default::default()
        };
        assert!(!config.should_discover_vm_images_dir());
    }

    #[test]
    fn test_vm_images_dir_or_default() {
        let config = StorageConfig::default();
        assert_eq!(
            config.vm_images_dir_or_default(),
            PathBuf::from("/var/lib/libvirt/images")
        );

        let config = StorageConfig {
            vm_images_dir: Some(PathBuf::from("/custom/path")),
            ..Default::default()
        };
        assert_eq!(config.vm_images_dir_or_default(), PathBuf::from("/custom/path"));
    }

    #[test]
    fn test_base_images_dir_or_default() {
        // Falls back to vm_images_dir
        let config = StorageConfig {
            vm_images_dir: Some(PathBuf::from("/var/lib/libvirt/images")),
            ..Default::default()
        };
        assert_eq!(
            config.base_images_dir_or_default(),
            PathBuf::from("/var/lib/libvirt/images")
        );

        // Uses explicit base_images_dir
        let config = StorageConfig {
            base_images_dir: Some(PathBuf::from("/custom/base")),
            ..Default::default()
        };
        assert_eq!(config.base_images_dir_or_default(), PathBuf::from("/custom/base"));
    }

    #[test]
    fn test_intermediate_dir_or_default() {
        let config = StorageConfig::default();
        assert_eq!(
            config.intermediate_dir_or_default(),
            PathBuf::from("/var/lib/libvirt/images/intermediate")
        );

        let config = StorageConfig {
            intermediate_dir: Some(PathBuf::from("/custom/intermediate")),
            ..Default::default()
        };
        assert_eq!(
            config.intermediate_dir_or_default(),
            PathBuf::from("/custom/intermediate")
        );
    }

    #[test]
    fn test_cloud_init_dir_or_default() {
        let config = StorageConfig::default();
        assert_eq!(config.cloud_init_dir_or_default(), PathBuf::from("/tmp"));

        let config = StorageConfig {
            cloud_init_dir: Some(PathBuf::from("/custom/cloud-init")),
            ..Default::default()
        };
        assert_eq!(
            config.cloud_init_dir_or_default(),
            PathBuf::from("/custom/cloud-init")
        );
    }

    #[test]
    fn test_serde_yaml_serialization() {
        let config = StorageConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("max_disk_size_gb"));
        assert!(yaml.contains("100"));
    }

    #[test]
    fn test_serde_yaml_deserialization() {
        let yaml = r#"
max_disk_size_gb: 200
min_free_space_gb: 20
enable_cow: false
"#;
        let config: StorageConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.max_disk_size_gb, 200);
        assert_eq!(config.min_free_space_gb, 20);
        assert!(!config.enable_cow);
        // Others should be None (discovered)
        assert!(config.vm_images_dir.is_none());
    }

    #[test]
    fn test_serde_yaml_with_paths() {
        let yaml = r#"
vm_images_dir: "/var/lib/libvirt/images"
base_images_dir: "/mnt/base-images"
max_disk_size_gb: 150
"#;
        let config: StorageConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.vm_images_dir,
            Some(PathBuf::from("/var/lib/libvirt/images"))
        );
        assert_eq!(config.base_images_dir, Some(PathBuf::from("/mnt/base-images")));
        assert_eq!(config.max_disk_size_gb, 150);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cow_enabled_by_default() {
        let config = StorageConfig::default();
        assert!(config.enable_cow);
    }
}

