//! Configuration system for benchScale
//!
//! Provides centralized configuration with environment variable support
//! and no hardcoded values in production code.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Global configuration for benchScale
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct Config {
    /// Docker configuration
    pub docker: DockerConfig,
    /// Libvirt configuration
    pub libvirt: LibvirtConfig,
    /// Network configuration
    pub network: NetworkConfig,
    /// Lab configuration
    pub lab: LabConfig,
}

/// Docker backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerConfig {
    /// Use hardened images by default
    #[serde(default = "defaults::use_hardened_images")]
    pub use_hardened_images: bool,

    /// Image pull timeout
    #[serde(default = "defaults::image_pull_timeout_secs")]
    pub image_pull_timeout_secs: u64,
}

/// Libvirt backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibvirtConfig {
    /// Connection URI (e.g., "qemu:///system")
    #[serde(default = "defaults::libvirt_uri")]
    pub uri: String,

    /// Base image path for VM disk images
    #[serde(default = "defaults::base_image_path")]
    pub base_image_path: PathBuf,

    /// Overlay directory for copy-on-write disks
    #[serde(default = "defaults::overlay_dir")]
    pub overlay_dir: PathBuf,

    /// SSH configuration for VM access
    pub ssh: SshConfig,
}

/// SSH configuration for VM access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    /// Default SSH port
    #[serde(default = "defaults::ssh_port")]
    pub port: u16,

    /// Connection timeout in seconds
    #[serde(default = "defaults::ssh_timeout_secs")]
    pub timeout_secs: u64,

    /// Private key path (preferred over password)
    pub key_path: Option<PathBuf>,

    /// Default user (can be overridden per-node)
    #[serde(default = "defaults::ssh_user")]
    pub default_user: String,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Default subnet for new networks
    #[serde(default = "defaults::default_subnet")]
    pub default_subnet: String,

    /// Network creation timeout
    #[serde(default = "defaults::network_timeout_secs")]
    pub timeout_secs: u64,
}

/// Lab configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabConfig {
    /// Lab state directory
    #[serde(default = "defaults::lab_state_dir")]
    pub state_dir: PathBuf,

    /// Lab creation timeout
    #[serde(default = "defaults::lab_create_timeout_secs")]
    pub create_timeout_secs: u64,

    /// Auto-cleanup failed labs
    #[serde(default = "defaults::auto_cleanup")]
    pub auto_cleanup: bool,
}

/// Default values module
mod defaults {
    use std::path::PathBuf;

    pub fn use_hardened_images() -> bool {
        std::env::var("BENCHSCALE_USE_HARDENED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(false)
    }

    pub fn image_pull_timeout_secs() -> u64 {
        std::env::var("BENCHSCALE_IMAGE_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300) // 5 minutes
    }

    pub fn libvirt_uri() -> String {
        std::env::var("BENCHSCALE_LIBVIRT_URI").unwrap_or_else(|_| "qemu:///system".to_string())
    }

    pub fn base_image_path() -> PathBuf {
        std::env::var("BENCHSCALE_BASE_IMAGE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/var/lib/libvirt/images"))
    }

    pub fn overlay_dir() -> PathBuf {
        std::env::var("BENCHSCALE_OVERLAY_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let mut path = std::env::temp_dir();
                path.push("benchscale/overlays");
                path
            })
    }

    pub fn ssh_port() -> u16 {
        std::env::var("BENCHSCALE_SSH_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(22)
    }

    pub fn ssh_timeout_secs() -> u64 {
        std::env::var("BENCHSCALE_SSH_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30)
    }

    pub fn ssh_user() -> String {
        std::env::var("BENCHSCALE_SSH_USER").unwrap_or_else(|_| "benchscale".to_string())
    }

    pub fn default_subnet() -> String {
        std::env::var("BENCHSCALE_DEFAULT_SUBNET").unwrap_or_else(|_| "10.100.0.0/24".to_string())
    }

    pub fn network_timeout_secs() -> u64 {
        std::env::var("BENCHSCALE_NETWORK_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60)
    }

    pub fn lab_state_dir() -> PathBuf {
        std::env::var("BENCHSCALE_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
                path.push(".benchscale/labs");
                path
            })
    }

    pub fn lab_create_timeout_secs() -> u64 {
        std::env::var("BENCHSCALE_LAB_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300) // 5 minutes
    }

    pub fn auto_cleanup() -> bool {
        std::env::var("BENCHSCALE_AUTO_CLEANUP")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(true)
    }
}


impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            use_hardened_images: defaults::use_hardened_images(),
            image_pull_timeout_secs: defaults::image_pull_timeout_secs(),
        }
    }
}

impl Default for LibvirtConfig {
    fn default() -> Self {
        Self {
            uri: defaults::libvirt_uri(),
            base_image_path: defaults::base_image_path(),
            overlay_dir: defaults::overlay_dir(),
            ssh: SshConfig::default(),
        }
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            port: defaults::ssh_port(),
            timeout_secs: defaults::ssh_timeout_secs(),
            key_path: std::env::var("BENCHSCALE_SSH_KEY").ok().map(PathBuf::from),
            default_user: defaults::ssh_user(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            default_subnet: defaults::default_subnet(),
            timeout_secs: defaults::network_timeout_secs(),
        }
    }
}

impl Default for LabConfig {
    fn default() -> Self {
        Self {
            state_dir: defaults::lab_state_dir(),
            create_timeout_secs: defaults::lab_create_timeout_secs(),
            auto_cleanup: defaults::auto_cleanup(),
        }
    }
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self::default()
    }

    /// Load configuration from TOML file
    pub fn from_file(path: impl AsRef<std::path::Path>) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| crate::Error::Other(format!("Failed to parse config: {}", e)))
    }

    /// Save configuration to TOML file
    pub fn to_file(&self, path: impl AsRef<std::path::Path>) -> crate::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::Error::Other(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get SSH timeout as Duration
    pub fn ssh_timeout(&self) -> Duration {
        Duration::from_secs(self.libvirt.ssh.timeout_secs)
    }

    /// Get image pull timeout as Duration
    pub fn image_pull_timeout(&self) -> Duration {
        Duration::from_secs(self.docker.image_pull_timeout_secs)
    }

    /// Get network timeout as Duration
    pub fn network_timeout(&self) -> Duration {
        Duration::from_secs(self.network.timeout_secs)
    }

    /// Get lab create timeout as Duration
    pub fn lab_create_timeout(&self) -> Duration {
        Duration::from_secs(self.lab.create_timeout_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        // Clear any environment variables that might interfere
        std::env::remove_var("BENCHSCALE_SSH_PORT");

        let config = Config::default();
        assert_eq!(config.libvirt.ssh.port, 22);
        assert!(!config.docker.use_hardened_images); // default
    }

    #[test]
    fn test_config_from_env() {
        // Clear environment first
        for (key, _) in std::env::vars() {
            if key.starts_with("BENCHSCALE_") {
                std::env::remove_var(&key);
            }
        }

        std::env::set_var("BENCHSCALE_SSH_PORT", "2222");
        let config = Config::from_env();
        assert_eq!(config.libvirt.ssh.port, 2222);

        // Cleanup
        std::env::remove_var("BENCHSCALE_SSH_PORT");
    }

    #[test]
    fn test_ssh_timeout_conversion() {
        let config = Config::default();
        assert_eq!(config.ssh_timeout(), Duration::from_secs(30));
    }
}
