// SPDX-License-Identifier: AGPL-3.0-only
//! Constants and default values for benchScale
//!
//! This module provides capability-based defaults that can be discovered
//! at runtime. These are fallbacks when runtime discovery is not available.

use std::path::PathBuf;

/// Network configuration constants
pub mod network {
    /// Default libvirt network prefix (override with `BENCHSCALE_DEFAULT_NETWORK_PREFIX`)
    pub fn default_network_prefix() -> String {
        std::env::var("BENCHSCALE_DEFAULT_NETWORK_PREFIX")
            .unwrap_or_else(|_| "192.168.122".to_string())
    }

    /// Default gateway (last octet)
    pub const DEFAULT_GATEWAY_SUFFIX: u8 = 1;

    /// Default subnet mask
    pub const DEFAULT_SUBNET_MASK: u8 = 24;

    /// Default subnet for benchScale networks (override with `BENCHSCALE_DEFAULT_SUBNET`)
    pub fn default_subnet() -> String {
        std::env::var("BENCHSCALE_DEFAULT_SUBNET")
            .unwrap_or_else(|_| "10.100.0.0/24".to_string())
    }

    /// IP pool start (for deterministic allocation)
    pub const IP_POOL_START: u8 = 10;

    /// IP pool end (for deterministic allocation)
    pub const IP_POOL_END: u8 = 250;

    /// Primary DNS nameserver (override with `BENCHSCALE_DNS_PRIMARY`)
    pub fn default_dns_primary() -> String {
        std::env::var("BENCHSCALE_DNS_PRIMARY").unwrap_or_else(|_| "8.8.8.8".to_string())
    }

    /// Secondary DNS nameserver (override with `BENCHSCALE_DNS_SECONDARY`)
    pub fn default_dns_secondary() -> String {
        std::env::var("BENCHSCALE_DNS_SECONDARY").unwrap_or_else(|_| "8.8.4.4".to_string())
    }

    /// VNC listen on all interfaces (for remote access); override with `BENCHSCALE_VNC_LISTEN_ALL`
    pub fn vnc_listen_all() -> String {
        std::env::var("BENCHSCALE_VNC_LISTEN_ALL").unwrap_or_else(|_| "0.0.0.0".to_string())
    }

    /// VNC listen on localhost only (for local access); override with `BENCHSCALE_VNC_LISTEN_LOCAL`
    pub fn vnc_listen_local() -> String {
        std::env::var("BENCHSCALE_VNC_LISTEN_LOCAL").unwrap_or_else(|_| "127.0.0.1".to_string())
    }
}

/// Storage and path constants
pub mod paths {
    use super::PathBuf;

    /// Conventional default libvirt default-pool path (same fallback as [`crate::config::StorageConfig::vm_images_dir_or_default`]).
    pub fn default_system_vm_images_dir() -> PathBuf {
        PathBuf::from("/var/lib/libvirt/images")
    }

    /// VM / libvirt disk images directory (`BENCHSCALE_VM_IMAGES_DIR`, else [`default_system_vm_images_dir`]).
    pub fn vm_images_dir() -> PathBuf {
        std::env::var("BENCHSCALE_VM_IMAGES_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_system_vm_images_dir())
    }

    /// Get libvirt images directory with capability discovery
    ///
    /// Order: `BENCHSCALE_VM_IMAGES_DIR`, `LIBVIRT_IMAGES_DIR`, XDG `libvirt/images` if present,
    /// then [`vm_images_dir`] (default system path).
    pub fn libvirt_images_dir() -> PathBuf {
        if let Ok(path) = std::env::var("BENCHSCALE_VM_IMAGES_DIR") {
            return PathBuf::from(path);
        }

        if let Ok(path) = std::env::var("LIBVIRT_IMAGES_DIR") {
            return PathBuf::from(path);
        }

        if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
            let path = PathBuf::from(xdg_data).join("libvirt/images");
            if path.exists() {
                return path;
            }
        }

        vm_images_dir()
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

    /// Default VM architecture (override with `BENCHSCALE_DEFAULT_VM_ARCH`)
    pub fn default_arch() -> String {
        std::env::var("BENCHSCALE_DEFAULT_VM_ARCH").unwrap_or_else(|_| "x86_64".to_string())
    }
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
        assert_eq!(network::default_network_prefix(), "192.168.122");
        assert_eq!(network::default_subnet(), "10.100.0.0/24");
        assert_eq!(network::default_dns_primary(), "8.8.8.8");
        assert_eq!(network::default_dns_secondary(), "8.8.4.4");
        assert_eq!(network::vnc_listen_local(), "127.0.0.1");
        assert_eq!(network::vnc_listen_all(), "0.0.0.0");
        assert_eq!(network::IP_POOL_START, 10);
        assert_eq!(network::IP_POOL_END, 250);
    }

    #[test]
    fn test_path_discovery() {
        assert_eq!(
            paths::default_system_vm_images_dir(),
            paths::vm_images_dir()
        );
        let images_dir = paths::libvirt_images_dir();
        assert!(images_dir.to_string_lossy().contains("libvirt"));

        let temp = paths::temp_dir();
        assert!(temp.exists() || temp.to_string_lossy().contains("tmp"));
    }

    #[test]
    fn test_vm_defaults() {
        assert_eq!(vm::default_arch(), "x86_64");
        assert_eq!(vm::DEFAULT_MEMORY_MB, 2048);
        assert_eq!(vm::DEFAULT_VCPUS, 2);
        assert_eq!(vm::DEFAULT_DISK_GB, 20);
    }
}
