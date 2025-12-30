//! Constants and default values for benchScale
//!
//! This module provides capability-based defaults that can be discovered
//! at runtime. These are fallbacks when runtime discovery is not available.

use std::path::PathBuf;

/// Network configuration constants
pub mod network {
    /// Default libvirt network prefix
    pub const DEFAULT_NETWORK_PREFIX: &str = "192.168.122";
    
    /// Default gateway (last octet)
    pub const DEFAULT_GATEWAY_SUFFIX: u8 = 1;
    
    /// Default subnet mask
    pub const DEFAULT_SUBNET_MASK: u8 = 24;
    
    /// Default subnet for benchScale networks
    pub const DEFAULT_SUBNET: &str = "10.100.0.0/24";
    
    /// IP pool start (for deterministic allocation)
    pub const IP_POOL_START: u8 = 10;
    
    /// IP pool end (for deterministic allocation)
    pub const IP_POOL_END: u8 = 250;
}

/// Storage and path constants
pub mod paths {
    use super::*;
    
    /// Get libvirt images directory with capability discovery
    ///
    /// Attempts to discover the actual libvirt images directory,
    /// falling back to standard locations if discovery fails.
    pub fn libvirt_images_dir() -> PathBuf {
        // Try to discover from environment
        if let Ok(path) = std::env::var("LIBVIRT_IMAGES_DIR") {
            return PathBuf::from(path);
        }
        
        // Try XDG data home
        if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
            let path = PathBuf::from(xdg_data).join("libvirt/images");
            if path.exists() {
                return path;
            }
        }
        
        // Standard system location
        PathBuf::from("/var/lib/libvirt/images")
    }
    
    /// Get temporary directory with XDG support
    pub fn temp_dir() -> PathBuf {
        // Try XDG runtime dir first
        if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(xdg_runtime);
        }
        
        // Fall back to system temp
        std::env::temp_dir()
    }
    
    /// Get cloud-init working directory
    pub fn cloud_init_dir() -> PathBuf {
        temp_dir().join("benchscale-cloud-init")
    }
}

/// VM configuration constants
pub mod vm {
    /// Default VM memory in MB
    pub const DEFAULT_MEMORY_MB: u32 = 2048;
    
    /// Default number of vCPUs
    pub const DEFAULT_VCPUS: u32 = 2;
    
    /// Default disk size in GB
    pub const DEFAULT_DISK_GB: u32 = 20;
    
    /// Default VM architecture
    pub const DEFAULT_ARCH: &str = "x86_64";
}

/// Timeout constants
pub mod timeouts {
    use std::time::Duration;
    
    /// Default timeout for VM boot
    pub const VM_BOOT: Duration = Duration::from_secs(120);
    
    /// Default timeout for cloud-init completion
    pub const CLOUD_INIT: Duration = Duration::from_secs(600);
    
    /// Default timeout for SSH connection
    pub const SSH_CONNECT: Duration = Duration::from_secs(30);
    
    /// Default timeout for command execution
    pub const COMMAND_EXEC: Duration = Duration::from_secs(300);
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_network_constants() {
        assert_eq!(network::DEFAULT_NETWORK_PREFIX, "192.168.122");
        assert_eq!(network::IP_POOL_START, 10);
        assert_eq!(network::IP_POOL_END, 250);
    }
    
    #[test]
    fn test_path_discovery() {
        let images_dir = paths::libvirt_images_dir();
        assert!(images_dir.to_string_lossy().contains("libvirt"));
        
        let temp = paths::temp_dir();
        assert!(temp.exists() || temp.to_string_lossy().contains("tmp"));
    }
    
    #[test]
    fn test_vm_defaults() {
        assert_eq!(vm::DEFAULT_MEMORY_MB, 2048);
        assert_eq!(vm::DEFAULT_VCPUS, 2);
        assert_eq!(vm::DEFAULT_DISK_GB, 20);
    }
}

