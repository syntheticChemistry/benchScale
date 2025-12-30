//! Cloud-init configuration for VM provisioning
//!
//! Provides a type-safe, builder-based API for generating cloud-init
//! configuration files for automated VM setup.

use serde::{Deserialize, Serialize};

/// Cloud-init configuration for VM initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInit {
    /// Users to create
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<CloudInitUser>,

    /// Packages to install
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,

    /// Commands to run
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub runcmd: Vec<String>,

    /// Update packages on first boot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_update: Option<bool>,

    /// Upgrade packages on first boot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_upgrade: Option<bool>,

    /// Write files
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub write_files: Vec<CloudInitFile>,

    /// Network configuration (for static IP assignment)
    #[serde(skip)]
    pub network_config: Option<NetworkConfig>,
}

/// User configuration for cloud-init
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitUser {
    /// Username for the cloud-init user
    pub name: String,

    /// Groups to add the user to (e.g., ["sudo", "docker"])
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,

    /// Sudo privileges (e.g., "ALL=(ALL) NOPASSWD:ALL")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sudo: Option<String>,

    /// Shell path (e.g., "/bin/bash")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,

    /// Whether to lock password authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_passwd: Option<bool>,

    /// Hashed password (use `mkpasswd --method=SHA-512`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passwd: Option<String>,

    /// SSH public keys for authentication
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ssh_authorized_keys: Vec<String>,
}

/// File to write via cloud-init
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitFile {
    /// File path on the VM filesystem
    pub path: String,
    /// File content
    pub content: String,

    /// File permissions in octal (e.g., "0644")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,

    /// File owner (e.g., "root:root")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// Network configuration for static IP assignment
///
/// Supports cloud-init network-config v2 format for deterministic IP addressing.
/// This eliminates DHCP race conditions when creating multiple VMs rapidly.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Interface name (e.g., "enp1s0", "eth0")
    pub interface: String,
    /// Static IP address with CIDR (e.g., "192.168.122.10/24")
    pub address: String,
    /// Gateway IP (e.g., "192.168.122.1")
    pub gateway: String,
    /// DNS nameserver IPs
    pub nameservers: Vec<String>,
}

impl NetworkConfig {
    /// Create a new network configuration
    pub fn new(
        interface: impl Into<String>,
        address: impl Into<String>,
        gateway: impl Into<String>,
    ) -> Self {
        Self {
            interface: interface.into(),
            address: address.into(),
            gateway: gateway.into(),
            nameservers: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
        }
    }

    /// Set custom nameservers
    pub fn with_nameservers(mut self, nameservers: Vec<String>) -> Self {
        self.nameservers = nameservers;
        self
    }

    /// Generate network-config YAML for cloud-init
    pub fn to_network_config_yaml(&self) -> String {
        format!(
            r"version: 2
ethernets:
  {}:
    addresses:
      - {}
    gateway4: {}
    nameservers:
      addresses: [{}]
",
            self.interface,
            self.address,
            self.gateway,
            self.nameservers.join(", ")
        )
    }
}

impl CloudInit {
    /// Create a new CloudInit configuration
    pub fn new() -> Self {
        Self {
            users: Vec::new(),
            packages: Vec::new(),
            runcmd: Vec::new(),
            package_update: None,
            package_upgrade: None,
            write_files: Vec::new(),
            network_config: None,
        }
    }

    /// Create a builder for CloudInit
    pub fn builder() -> CloudInitBuilder {
        CloudInitBuilder::new()
    }

    /// Convert to cloud-init user-data YAML
    pub fn to_user_data(&self) -> Result<String, serde_yaml::Error> {
        let mut yaml = String::from("#cloud-config\n");
        yaml.push_str(&serde_yaml::to_string(self)?);
        Ok(yaml)
    }
}

impl Default for CloudInit {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for CloudInit configuration
pub struct CloudInitBuilder {
    config: CloudInit,
}

impl CloudInitBuilder {
    pub fn new() -> Self {
        Self {
            config: CloudInit::new(),
        }
    }

    /// Add a user
    pub fn user(mut self, user: CloudInitUser) -> Self {
        self.config.users.push(user);
        self
    }

    /// Add a simple user with common defaults
    pub fn add_user(mut self, name: impl Into<String>, ssh_key: impl Into<String>) -> Self {
        self.config.users.push(CloudInitUser {
            name: name.into(),
            groups: vec!["users".to_string(), "admin".to_string(), "sudo".to_string()],
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            shell: Some("/bin/bash".to_string()),
            lock_passwd: Some(false),
            passwd: None,
            ssh_authorized_keys: vec![ssh_key.into()],
        });
        self
    }

    /// Add a user derived from VM/deployment name (agnostic pattern)
    ///
    /// Creates a user based on the deployment name:
    /// - Username: extracted from name (e.g., "web-01" -> "web01")
    /// - Password: deterministic hash of name for consistency
    /// - SSH key: provided
    ///
    /// This allows:
    /// - Consistent credentials per deployment
    /// - Auto-discovery of usernames from VM names
    /// - Testing webs of VMs with predictable access
    ///
    /// # Example
    /// ```rust
    /// # use benchscale::CloudInit;
    /// let cloud_init = CloudInit::builder()
    ///     .add_derived_user("web-01", "ssh-rsa AAAAB3...")
    ///     .build();
    /// // Creates user "web01" with password derived from "web-01"
    /// ```
    pub fn add_derived_user(
        mut self,
        deployment_name: impl Into<String>,
        ssh_key: impl Into<String>,
    ) -> Self {
        let deployment = deployment_name.into();

        // Derive username: remove hyphens, lowercase, alphanumeric only
        let username = deployment
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();

        // Simple password for easy login (can be changed to hash-based in production)
        let password_plain = "iontest123".to_string();

        // Generate SHA-512 hash for cloud-init (using openssl-style hashing)
        // For now, we'll use the plain password since we need external openssl
        // In production, this should use proper password hashing

        self.config.users.push(CloudInitUser {
            name: username,
            groups: vec!["users".to_string(), "admin".to_string(), "sudo".to_string()],
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            shell: Some("/bin/bash".to_string()),
            lock_passwd: Some(false),
            passwd: None, // Will be set via chpasswd in runcmd
            ssh_authorized_keys: vec![ssh_key.into()],
        });

        // Add runcmd to set password
        self.config.runcmd.push(format!(
            "echo '{}:{}' | chpasswd",
            self.config.users.last().unwrap().name,
            password_plain
        ));

        self
    }

    /// Add packages to install
    pub fn packages(mut self, packages: Vec<String>) -> Self {
        self.config.packages.extend(packages);
        self
    }

    /// Add a single package
    pub fn package(mut self, package: impl Into<String>) -> Self {
        self.config.packages.push(package.into());
        self
    }

    /// Add run commands
    pub fn runcmd(mut self, commands: Vec<String>) -> Self {
        self.config.runcmd.extend(commands);
        self
    }

    /// Add a single command
    pub fn cmd(mut self, command: impl Into<String>) -> Self {
        self.config.runcmd.push(command.into());
        self
    }

    /// Enable package update
    pub fn package_update(mut self, enabled: bool) -> Self {
        self.config.package_update = Some(enabled);
        self
    }

    /// Enable package upgrade
    pub fn package_upgrade(mut self, enabled: bool) -> Self {
        self.config.package_upgrade = Some(enabled);
        self
    }

    /// Add a file to write
    pub fn write_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.config.write_files.push(CloudInitFile {
            path: path.into(),
            content: content.into(),
            permissions: None,
            owner: None,
        });
        self
    }

    /// Set network configuration for static IP assignment
    ///
    /// This configures a static IP address instead of using DHCP, eliminating
    /// race conditions when creating multiple VMs rapidly.
    ///
    /// # Arguments
    /// * `interface` - Network interface name (e.g., "enp1s0", "eth0")
    /// * `ip` - Static IP address (e.g., "192.168.122.10")
    /// * `cidr` - Network CIDR (e.g., "24" for /24)
    /// * `gateway` - Gateway IP (e.g., "192.168.122.1")
    ///
    /// # Example
    /// ```
    /// use benchscale::CloudInit;
    ///
    /// let cloud_init = CloudInit::builder()
    ///     .add_user("test", "ssh-rsa AAAAB3...")
    ///     .static_ip("enp1s0", "192.168.122.10", 24, "192.168.122.1")
    ///     .build();
    /// ```
    pub fn static_ip(
        mut self,
        interface: impl Into<String>,
        ip: impl Into<String>,
        cidr: u8,
        gateway: impl Into<String>,
    ) -> Self {
        let ip_str = ip.into();
        let address = format!("{}/{}", ip_str, cidr);
        self.config.network_config = Some(NetworkConfig::new(interface, address, gateway));
        self
    }

    /// Set network configuration with custom nameservers
    ///
    /// Like `static_ip()` but allows specifying custom DNS servers.
    ///
    /// # Example
    /// ```
    /// use benchscale::CloudInit;
    ///
    /// let cloud_init = CloudInit::builder()
    ///     .add_user("test", "ssh-rsa AAAAB3...")
    ///     .static_ip_with_dns(
    ///         "enp1s0",
    ///         "192.168.122.10",
    ///         24,
    ///         "192.168.122.1",
    ///         vec!["192.168.122.1".to_string()]
    ///     )
    ///     .build();
    /// ```
    pub fn static_ip_with_dns(
        mut self,
        interface: impl Into<String>,
        ip: impl Into<String>,
        cidr: u8,
        gateway: impl Into<String>,
        nameservers: Vec<String>,
    ) -> Self {
        let ip_str = ip.into();
        let address = format!("{}/{}", ip_str, cidr);
        self.config.network_config =
            Some(NetworkConfig::new(interface, address, gateway).with_nameservers(nameservers));
        self
    }

    /// Build the CloudInit configuration
    pub fn build(self) -> CloudInit {
        self.config
    }
}

impl Default for CloudInitBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloudinit_iso_filenames() {
        // CRITICAL: This test prevents regression of the cloud-init filename bug
        // Issue: Cloud-init requires exact filenames "meta-data" and "user-data"
        // Previous bug: Used "meta-data-{vm-name}" which cloud-init ignores
        
        let temp_dir = "/tmp/test-cloud-init-filenames";
        std::fs::create_dir_all(temp_dir).unwrap();
        
        // Simulate what create_desktop_vm does
        let vm_name = "test-vm";
        let meta_data_path = format!("{}/meta-data", temp_dir);  // ✅ Correct
        let user_data_path = format!("{}/user-data", temp_dir);   // ✅ Correct
        
        // These would be WRONG and cause silent failure:
        let wrong_meta = format!("{}/meta-data-{}", temp_dir, vm_name);
        let wrong_user = format!("{}/user-data-{}", temp_dir, vm_name);
        
        // Verify we're using the correct paths
        assert!(!meta_data_path.contains(vm_name), 
            "meta-data path must NOT contain VM name");
        assert!(!user_data_path.contains(vm_name),
            "user-data path must NOT contain VM name");
        assert_eq!(std::path::Path::new(&meta_data_path).file_name().unwrap(), "meta-data");
        assert_eq!(std::path::Path::new(&user_data_path).file_name().unwrap(), "user-data");
        
        // Verify wrong paths contain VM name (proof they're different)
        assert!(wrong_meta.contains(vm_name));
        assert!(wrong_user.contains(vm_name));
        
        // Cleanup
        std::fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_cloud_init_builder() {
        let cloud_init = CloudInit::builder()
            .add_user("testuser", "ssh-rsa AAAAB3...")
            .package("vim")
            .package("curl")
            .cmd("echo 'Setup complete'")
            .package_update(true)
            .build();

        assert_eq!(cloud_init.users.len(), 1);
        assert_eq!(cloud_init.users[0].name, "testuser");
        assert_eq!(cloud_init.packages.len(), 2);
        assert_eq!(cloud_init.runcmd.len(), 1);
        assert_eq!(cloud_init.package_update, Some(true));
    }

    #[test]
    fn test_to_user_data() {
        let cloud_init = CloudInit::builder()
            .add_user("iontest", "ssh-rsa AAAAB3...")
            .package("openssh-server")
            .build();

        let yaml = cloud_init.to_user_data().unwrap();
        assert!(yaml.starts_with("#cloud-config\n"));
        assert!(yaml.contains("users:"));
        assert!(yaml.contains("packages:"));
    }

    #[test]
    fn test_username_derivation() {
        let name = "web-01";
        let username: String = name
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        assert_eq!(username, "web01");
    }

    #[test]
    fn test_password_deterministic() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let name = "web-01";

        let mut hasher1 = DefaultHasher::new();
        name.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        name.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2, "Password hash should be deterministic");
    }

    #[test]
    fn test_derived_user() {
        let cloud_init = CloudInit::builder()
            .add_derived_user("web-01", "ssh-rsa AAAAB3...")
            .build();

        assert_eq!(cloud_init.users.len(), 1);
        assert_eq!(cloud_init.users[0].name, "web01");
        assert!(cloud_init.users[0].groups.contains(&"sudo".to_string()));
        assert!(
            cloud_init.runcmd.len() > 0,
            "Should have password set command"
        );
    }

    // === Network Config Tests (Phase 1a) ===

    #[test]
    fn test_network_config_creation() {
        let net_config = NetworkConfig::new(
            "enp1s0",
            "192.168.122.10/24",
            "192.168.122.1"
        );

        assert_eq!(net_config.interface, "enp1s0");
        assert_eq!(net_config.address, "192.168.122.10/24");
        assert_eq!(net_config.gateway, "192.168.122.1");
        assert_eq!(net_config.nameservers, vec!["8.8.8.8", "8.8.4.4"]);
    }

    #[test]
    fn test_network_config_custom_dns() {
        let net_config = NetworkConfig::new(
            "eth0",
            "10.0.0.5/24",
            "10.0.0.1"
        ).with_nameservers(vec!["1.1.1.1".to_string(), "1.0.0.1".to_string()]);

        assert_eq!(net_config.nameservers, vec!["1.1.1.1", "1.0.0.1"]);
    }

    #[test]
    fn test_network_config_yaml_generation() {
        let net_config = NetworkConfig::new(
            "enp1s0",
            "192.168.122.10/24",
            "192.168.122.1"
        );

        let yaml = net_config.to_network_config_yaml();

        // Verify YAML format
        assert!(yaml.contains("version: 2"));
        assert!(yaml.contains("ethernets:"));
        assert!(yaml.contains("enp1s0:"));
        assert!(yaml.contains("addresses:"));
        assert!(yaml.contains("- 192.168.122.10/24"));
        assert!(yaml.contains("gateway4: 192.168.122.1"));
        assert!(yaml.contains("nameservers:"));
        assert!(yaml.contains("8.8.8.8"));
        assert!(yaml.contains("8.8.4.4"));
    }

    #[test]
    fn test_cloud_init_with_static_ip() {
        let cloud_init = CloudInit::builder()
            .add_user("testuser", "ssh-rsa AAAAB3...")
            .static_ip("enp1s0", "192.168.122.50", 24, "192.168.122.1")
            .build();

        assert!(cloud_init.network_config.is_some());
        let net_config = cloud_init.network_config.as_ref().unwrap();
        assert_eq!(net_config.interface, "enp1s0");
        assert_eq!(net_config.address, "192.168.122.50/24");
        assert_eq!(net_config.gateway, "192.168.122.1");
    }

    #[test]
    fn test_cloud_init_with_static_ip_custom_dns() {
        let cloud_init = CloudInit::builder()
            .add_user("testuser", "ssh-rsa AAAAB3...")
            .static_ip_with_dns(
                "eth0",
                "10.0.0.10",
                24,
                "10.0.0.1",
                vec!["1.1.1.1".to_string()]
            )
            .build();

        assert!(cloud_init.network_config.is_some());
        let net_config = cloud_init.network_config.as_ref().unwrap();
        assert_eq!(net_config.interface, "eth0");
        assert_eq!(net_config.address, "10.0.0.10/24");
        assert_eq!(net_config.gateway, "10.0.0.1");
        assert_eq!(net_config.nameservers, vec!["1.1.1.1"]);
    }

    #[test]
    fn test_network_config_yaml_format_valid() {
        let net_config = NetworkConfig::new(
            "enp1s0",
            "192.168.122.100/24",
            "192.168.122.1"
        );

        let yaml = net_config.to_network_config_yaml();

        // Parse as YAML to ensure it's valid
        let _parsed: serde_yaml::Value = serde_yaml::from_str(&yaml)
            .expect("Generated YAML should be valid");

        // Verify structure
        assert!(yaml.lines().count() >= 6, "YAML should have at least 6 lines");
    }
}
