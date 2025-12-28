//! Unit tests for benchScale cloud-init agnostic user derivation

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_username_derivation() {
        // Test basic name
        let name = "web-01";
        let username: String = name.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        assert_eq!(username, "web01");
        
        // Test complex name
        let name = "rustdesk-20251228-115834";
        let username: String = name.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        assert_eq!(username, "rustdesk20251228115834");
        
        // Test with special characters
        let name = "db_primary-01";
        let username: String = name.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        assert_eq!(username, "dbprimary01");
    }

    #[test]
    fn test_password_deterministic() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let name = "web-01";
        
        // Hash twice to verify determinism
        let mut hasher1 = DefaultHasher::new();
        name.hash(&mut hasher1);
        let hash1 = hasher1.finish();
        
        let mut hasher2 = DefaultHasher::new();
        name.hash(&mut hasher2);
        let hash2 = hasher2.finish();
        
        assert_eq!(hash1, hash2, "Password hash should be deterministic");
    }

    #[test]
    fn test_cloud_init_user_creation() {
        let cloud_init = CloudInit::builder()
            .add_derived_user("web-01", "ssh-rsa AAAAB3test...")
            .build();
        
        assert_eq!(cloud_init.users.len(), 1);
        assert_eq!(cloud_init.users[0].name, "web01");
        assert!(cloud_init.users[0].groups.contains(&"sudo".to_string()));
        assert!(cloud_init.runcmd.len() > 0, "Should have password set command");
    }

    #[test]
    fn test_cloud_init_multiple_users() {
        let cloud_init = CloudInit::builder()
            .add_derived_user("web-01", "ssh-rsa key1")
            .add_derived_user("db-01", "ssh-rsa key2")
            .build();
        
        assert_eq!(cloud_init.users.len(), 2);
        assert_eq!(cloud_init.users[0].name, "web01");
        assert_eq!(cloud_init.users[1].name, "db01");
        assert_eq!(cloud_init.runcmd.len(), 2, "Should have two password set commands");
    }

    #[test]
    fn test_cloud_init_to_yaml() {
        let cloud_init = CloudInit::builder()
            .add_derived_user("test-vm", "ssh-rsa AAAAB3test...")
            .package("curl")
            .build();
        
        let yaml = cloud_init.to_user_data().expect("Should generate YAML");
        assert!(yaml.starts_with("#cloud-config"));
        assert!(yaml.contains("users:"));
        assert!(yaml.contains("testvm"));
        assert!(yaml.contains("packages:"));
        assert!(yaml.contains("runcmd:"));
    }
}

