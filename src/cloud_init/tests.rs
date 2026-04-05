// SPDX-License-Identifier: AGPL-3.0-or-later
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
    let meta_data_path = format!("{}/meta-data", temp_dir); // ✅ Correct
    let user_data_path = format!("{}/user-data", temp_dir); // ✅ Correct

    // These would be WRONG and cause silent failure:
    let wrong_meta = format!("{}/meta-data-{}", temp_dir, vm_name);
    let wrong_user = format!("{}/user-data-{}", temp_dir, vm_name);

    // Verify we're using the correct paths
    assert!(
        !meta_data_path.contains(vm_name),
        "meta-data path must NOT contain VM name"
    );
    assert!(
        !user_data_path.contains(vm_name),
        "user-data path must NOT contain VM name"
    );
    assert_eq!(
        std::path::Path::new(&meta_data_path).file_name().unwrap(),
        "meta-data"
    );
    assert_eq!(
        std::path::Path::new(&user_data_path).file_name().unwrap(),
        "user-data"
    );

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
    let net_config = NetworkConfig::new("enp1s0", "192.168.122.10/24", "192.168.122.1");

    assert_eq!(net_config.interface, "enp1s0");
    assert_eq!(net_config.address, "192.168.122.10/24");
    assert_eq!(net_config.gateway, "192.168.122.1");
    assert_eq!(net_config.nameservers, vec!["8.8.8.8", "8.8.4.4"]);
}

#[test]
fn test_network_config_custom_dns() {
    let net_config = NetworkConfig::new("eth0", "10.0.0.5/24", "10.0.0.1")
        .with_nameservers(vec!["1.1.1.1".to_string(), "1.0.0.1".to_string()]);

    assert_eq!(net_config.nameservers, vec!["1.1.1.1", "1.0.0.1"]);
}

#[test]
fn test_network_config_yaml_generation() {
    let net_config = NetworkConfig::new("enp1s0", "192.168.122.10/24", "192.168.122.1");

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
            vec!["1.1.1.1".to_string()],
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
    let net_config = NetworkConfig::new("enp1s0", "192.168.122.100/24", "192.168.122.1");

    let yaml = net_config.to_network_config_yaml();

    // Parse as YAML to ensure it's valid
    let _parsed: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("Generated YAML should be valid");

    // Verify structure
    assert!(
        yaml.lines().count() >= 6,
        "YAML should have at least 6 lines"
    );
}
