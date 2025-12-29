//! Configuration system for benchScale
//!
//! Provides centralized configuration with environment variable support
//! and no hardcoded values in production code.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Global configuration for benchScale
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

    /// VM IP acquisition timeout in seconds
    #[serde(default = "defaults::vm_ip_timeout_secs")]
    pub vm_ip_timeout_secs: u64,

    /// Template directory for VM base images (e.g., agentReagents templates)
    /// If not specified, will attempt to auto-discover agentReagents
    #[serde(default = "defaults::template_dir")]
    pub template_dir: Option<PathBuf>,

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

    pub fn vm_ip_timeout_secs() -> u64 {
        std::env::var("BENCHSCALE_VM_IP_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(180) // 3 minutes (sufficient for COSMIC cloud-init)
    }

    pub fn template_dir() -> Option<PathBuf> {
        // 1. Check environment variable first
        if let Ok(dir) = std::env::var("BENCHSCALE_TEMPLATE_DIR") {
            return Some(PathBuf::from(dir));
        }

        // 2. Try to auto-discover agentReagents templates
        discover_agentreagents_templates().ok()
    }

    fn discover_agentreagents_templates() -> Result<PathBuf, ()> {
        // Common locations relative to benchScale
        let search_paths = vec![
            PathBuf::from("../primalTools/agentReagents/images/templates"),
            PathBuf::from("../../primalTools/agentReagents/images/templates"),
            PathBuf::from("../agentReagents/images/templates"),
            PathBuf::from("../../agentReagents/images/templates"),
            PathBuf::from("./agentReagents/images/templates"),
        ];

        // Also check AGENTREAGENTS_PATH if set
        if let Ok(base) = std::env::var("AGENTREAGENTS_PATH") {
            let mut path = PathBuf::from(base);
            path.push("images/templates");
            if path.exists() && path.is_dir() {
                return Ok(path);
            }
        }

        // Try relative paths
        for path in search_paths {
            if path.exists() && path.is_dir() {
                return Ok(path);
            }
        }

        Err(())
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
            vm_ip_timeout_secs: defaults::vm_ip_timeout_secs(),
            template_dir: defaults::template_dir(),
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

    /// Get VM IP acquisition timeout as Duration
    pub fn vm_ip_timeout(&self) -> Duration {
        Duration::from_secs(self.libvirt.vm_ip_timeout_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to clear all BENCHSCALE environment variables
    fn clear_benchscale_env() {
        for (key, _) in std::env::vars() {
            if key.starts_with("BENCHSCALE_") {
                std::env::remove_var(&key);
            }
        }
    }

    #[test]
    fn test_default_config() {
        // Clear all environment variables that might interfere
        clear_benchscale_env();

        // Small delay to ensure cleanup from other tests
        std::thread::sleep(std::time::Duration::from_millis(10));

        let config = Config::default();
        assert_eq!(config.libvirt.ssh.port, 22);
        assert!(!config.docker.use_hardened_images); // default
    }

    #[test]
    fn test_config_from_env() {
        // Clear environment first
        clear_benchscale_env();

        std::env::set_var("BENCHSCALE_SSH_PORT", "2222");
        let config = Config::from_env();
        assert_eq!(config.libvirt.ssh.port, 2222);

        // Cleanup after test
        clear_benchscale_env();
    }

    #[test]
    fn test_ssh_timeout_conversion() {
        // Clear environment first to ensure clean state
        clear_benchscale_env();

        let config = Config::default();
        assert_eq!(config.ssh_timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_image_pull_timeout_conversion() {
        clear_benchscale_env();
        let config = Config::default();
        // Just test it returns a valid duration
        assert!(config.image_pull_timeout().as_secs() > 0);
    }

    #[test]
    fn test_network_timeout_conversion() {
        clear_benchscale_env();
        let config = Config::default();
        // Just test it returns a valid duration
        assert!(config.network_timeout().as_secs() > 0);
    }

    #[test]
    fn test_lab_create_timeout_conversion() {
        clear_benchscale_env();
        let config = Config::default();
        // Just test it returns a valid duration
        assert!(config.lab_create_timeout().as_secs() > 0);
    }

    #[test]
    fn test_config_to_file() {
        clear_benchscale_env();
        let config = Config::default();
        let temp_file =
            std::env::temp_dir().join(format!("test_config_{}.toml", uuid::Uuid::new_v4()));

        config.to_file(&temp_file).expect("Should save config");
        assert!(temp_file.exists());

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_config_from_file() {
        clear_benchscale_env();
        let config = Config::default();
        let temp_file =
            std::env::temp_dir().join(format!("test_config_load_{}.toml", uuid::Uuid::new_v4()));

        config.to_file(&temp_file).expect("Should save config");

        let loaded = Config::from_file(&temp_file).expect("Should load config");
        assert_eq!(loaded.libvirt.ssh.port, config.libvirt.ssh.port);

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_config_from_nonexistent_file() {
        let result = Config::from_file("/nonexistent/path/config.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_docker_config_defaults() {
        clear_benchscale_env();
        let config = DockerConfig::default();
        // Just test the structure is valid
        assert!(config.image_pull_timeout_secs > 0);
    }

    #[test]
    fn test_libvirt_config_defaults() {
        clear_benchscale_env();
        let config = LibvirtConfig::default();
        // Just test the structure is valid
        assert!(!config.uri.is_empty());
        assert!(!config.overlay_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_ssh_config_defaults() {
        clear_benchscale_env();
        let config = SshConfig::default();
        // Just test the structure is valid
        assert!(config.port > 0);
        assert!(!config.default_user.is_empty());
        assert!(config.timeout_secs > 0);
    }

    #[test]
    fn test_network_config_defaults() {
        clear_benchscale_env();
        let config = NetworkConfig::default();
        // Just test the structure is valid
        assert!(!config.default_subnet.is_empty());
        assert!(config.timeout_secs > 0);
    }

    #[test]
    fn test_lab_config_defaults() {
        clear_benchscale_env();
        let config = LabConfig::default();
        // Just test the structure is valid
        assert!(!config.state_dir.as_os_str().is_empty());
        assert!(config.create_timeout_secs > 0);
    }

    #[test]
    fn test_env_var_ssh_port() {
        clear_benchscale_env();
        std::env::set_var("BENCHSCALE_SSH_PORT", "2222");

        let config = Config::from_env();
        assert_eq!(config.libvirt.ssh.port, 2222);

        clear_benchscale_env();
    }

    #[test]
    fn test_env_var_docker_hardened() {
        clear_benchscale_env();
        std::env::set_var("BENCHSCALE_USE_HARDENED", "true");

        let config = Config::from_env();
        assert!(config.docker.use_hardened_images);

        clear_benchscale_env();
    }

    #[test]
    fn test_env_var_libvirt_uri() {
        clear_benchscale_env();
        std::env::set_var("BENCHSCALE_LIBVIRT_URI", "qemu+ssh://host/system");

        let config = Config::from_env();
        assert_eq!(config.libvirt.uri, "qemu+ssh://host/system");

        clear_benchscale_env();
    }

    #[test]
    fn test_config_cloning() {
        clear_benchscale_env();
        let config1 = Config::default();
        let config2 = config1.clone();

        assert_eq!(config1.libvirt.ssh.port, config2.libvirt.ssh.port);
        assert_eq!(
            config1.docker.use_hardened_images,
            config2.docker.use_hardened_images
        );
    }
}
