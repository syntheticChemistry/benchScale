// SPDX-License-Identifier: AGPL-3.0-only
//! Integration tests for benchScale
//!
//! These tests verify basic functionality without requiring actual VMs

use benchscale::CloudInit;

#[test]
fn test_cloud_init_creation() {
    let cloud_init = CloudInit::builder()
        .add_user("testuser", "ssh-rsa AAAA...")
        .package("vim")
        .package("curl")
        .build();

    assert_eq!(cloud_init.users.len(), 1);
    assert_eq!(cloud_init.packages.len(), 2);
}

#[test]
fn test_cloud_init_yaml_generation() {
    let cloud_init = CloudInit::builder()
        .add_user("testuser", "ssh-rsa AAAA...")
        .build();

    let yaml = cloud_init.to_user_data().expect("Failed to generate YAML");

    // Should contain user data
    assert!(!yaml.is_empty());
    assert!(yaml.contains("testuser"));
}

#[test]
fn test_cloud_init_with_multiple_users() {
    let cloud_init = CloudInit::builder()
        .add_user("user1", "key1")
        .add_user("user2", "key2")
        .build();

    assert_eq!(cloud_init.users.len(), 2);
}

#[test]
fn test_cloud_init_with_packages() {
    let cloud_init = CloudInit::builder()
        .package("vim")
        .package("curl")
        .package("git")
        .package("htop")
        .build();

    assert_eq!(cloud_init.packages.len(), 4);
}
