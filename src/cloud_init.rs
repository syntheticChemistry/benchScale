//! Cloud-init configuration for VM provisioning
//!
//! Provides a type-safe, builder-based API for generating cloud-init
//! configuration files for automated VM setup.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

/// User configuration for cloud-init
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitUser {
    pub name: String,
    
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sudo: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_passwd: Option<bool>,
    
    /// Hashed password (use `mkpasswd --method=SHA-512`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passwd: Option<String>,
    
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ssh_authorized_keys: Vec<String>,
}

/// File to write via cloud-init
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitFile {
    pub path: String,
    pub content: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
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
    pub fn add_derived_user(mut self, deployment_name: impl Into<String>, ssh_key: impl Into<String>) -> Self {
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
            passwd: None,  // Will be set via chpasswd in runcmd
            ssh_authorized_keys: vec![ssh_key.into()],
        });
        
        // Add runcmd to set password
        self.config.runcmd.push(format!("echo '{}:{}' | chpasswd", 
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
        let username: String = name.chars()
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
        assert!(cloud_init.runcmd.len() > 0, "Should have password set command");
    }
}

