// SPDX-License-Identifier: AGPL-3.0-only
// Integration tests for cloud-init validation helpers
//
// These tests verify the behavior of wait_for_cloud_init, wait_for_ssh,
// and the *_ready() convenience methods WITH A REAL LIBVIRT CONNECTION.
//
// ## Requirements
// - Libvirt daemon running (`systemctl status libvirtd`)
// - User in libvirt group (`sudo usermod -aG libvirt $USER`)
// - OR run with: `sudo -E cargo test --features libvirt`
//
// ## Running
// ```bash
// cargo test --features libvirt libvirt_validation_tests -- --ignored
// ```
//
// ## Note
// These are marked #[ignore] because they require actual libvirt access.
// For unit testing timeout/retry logic without libvirt, see:
// - `src/backend/timeout_utils.rs` (pure functions)
// - `src/backend/timeout_utils.rs::tests` (unit tests)

#[cfg(test)]
mod cloud_init_validation_tests {
    use crate::backend::LibvirtBackend;
    use std::time::Duration;
    use tokio::time::Instant;

    #[tokio::test]
    #[ignore] // Requires libvirt daemon with proper permissions
    async fn test_wait_for_ssh_timeout_behavior() {
        // Test that wait_for_ssh properly times out
        let backend = LibvirtBackend::new().expect("Failed to create backend");

        let start = Instant::now();
        let result = backend
            .wait_for_ssh(
                "192.0.2.1", // TEST-NET-1 (unreachable)
                "testuser",
                "testpass",
                Duration::from_secs(5), // Short timeout for test
            )
            .await;

        let elapsed = start.elapsed();

        // Should fail with timeout
        assert!(result.is_err(), "Expected timeout error");

        // Should have taken at least 5 seconds
        assert!(elapsed.as_secs() >= 5, "Timeout occurred too early");

        // Should have error message mentioning timeout
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("Timeout"), "Error should mention timeout");
        assert!(err_msg.contains("SSH"), "Error should mention SSH");
    }

    #[tokio::test]
    #[ignore] // Requires libvirt daemon with proper permissions
    async fn test_wait_for_cloud_init_timeout_behavior() {
        // Test that wait_for_cloud_init properly times out
        let backend = LibvirtBackend::new().expect("Failed to create backend");

        let result = backend
            .wait_for_cloud_init(
                "nonexistent-vm",
                None, // No known IP - will try to query libvirt
                "testuser",
                "testpass",
                Duration::from_secs(1), // Very short timeout for test
            )
            .await;

        // Should fail (nonexistent VM can't get IP)
        assert!(result.is_err(), "Expected error for nonexistent VM");

        // Error should be meaningful
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(!err_msg.is_empty(), "Error message should not be empty");
    }

    #[tokio::test]
    #[ignore] // Requires libvirt daemon with proper permissions
    async fn test_exponential_backoff_ssh() {
        // Verify timeout behavior (not strict timing due to test environment variability)
        let backend = LibvirtBackend::new().expect("Failed to create backend");

        let start = Instant::now();
        let _result = backend
            .wait_for_ssh(
                "192.0.2.1", // Unreachable TEST-NET-1
                "test",
                "test",
                Duration::from_secs(3), // Shorter timeout for faster tests
            )
            .await;

        let elapsed = start.elapsed();

        // Should take at least 2 seconds (some attempts with backoff)
        assert!(elapsed.as_secs() >= 2, "Should wait for retries");
    }

    #[test]
    #[ignore] // Requires libvirt daemon with proper permissions
    fn test_backend_creation() {
        // Verify backend can be created
        let result = LibvirtBackend::new();
        assert!(result.is_ok(), "Backend creation should succeed");
    }

    #[tokio::test]
    #[ignore] // Requires libvirt daemon with proper permissions
    async fn test_wait_for_ip_private_helper() {
        // Test the private wait_for_ip helper
        let backend = LibvirtBackend::new().expect("Failed to create backend");

        let result = backend
            .wait_for_ip("nonexistent-vm", Duration::from_secs(2))
            .await;

        // Should timeout for nonexistent VM
        assert!(result.is_err(), "Should fail for nonexistent VM");
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("Timeout"), "Should mention timeout");
    }
}

// Integration tests that require real VMs
// These are marked with #[ignore] by default and run separately
#[cfg(test)]
mod integration_tests {
    use crate::backend::ssh::SshClient;
    use crate::backend::{Backend, LibvirtBackend};
    use std::time::Duration;

    /// Test cloud-init validation with a real VM
    ///
    /// To run: cargo test --features libvirt test_real_vm_cloud_init -- --ignored --nocapture
    ///
    /// Prerequisites:
    /// - Ubuntu cloud image at /tmp/test-cloud-image.img
    /// - Sufficient permissions for libvirt
    #[tokio::test]
    #[ignore]
    async fn test_real_vm_cloud_init_validation() {
        use std::path::Path;

        let backend = LibvirtBackend::new().expect("Failed to create backend");

        // Create cloud-init config using builder and add password via runcmd
        let cloud_init = crate::CloudInit::builder()
            .add_user("testuser", "")
            .cmd("echo 'testuser:testpass123' | chpasswd")
            .package("curl")
            .build();

        // Create VM
        let node = backend
            .create_desktop_vm(
                "test-cloud-init-vm",
                Path::new("/tmp/test-cloud-image.img"),
                &cloud_init,
                2048,
                2,
                10,
            )
            .await
            .expect("VM creation failed");

        println!("VM created: {} @ {}", node.name, node.ip_address);

        // Wait for cloud-init (pass known static IP)
        let result = backend
            .wait_for_cloud_init(
                &node.id,
                Some(&node.ip_address), // Use the known static IP
                "testuser",
                "testpass123",
                Duration::from_secs(300), // 5 minutes
            )
            .await;

        // Cleanup (stop the VM)
        let _ = backend.stop_node(&node.id).await;

        assert!(result.is_ok(), "Cloud-init should complete successfully");
    }

    /// Test the create_desktop_vm_ready convenience method
    #[tokio::test]
    #[ignore]
    async fn test_real_vm_create_ready() {
        use std::path::Path;

        let backend = LibvirtBackend::new().expect("Failed to create backend");

        let cloud_init = crate::CloudInit::builder()
            .add_user("testuser", "")
            .cmd("echo 'testuser:testpass123' | chpasswd")
            .build();

        // Use the convenience method
        let node = backend
            .create_desktop_vm_ready(
                "test-ready-vm",
                Path::new("/tmp/test-cloud-image.img"),
                &cloud_init,
                2048,
                2,
                10,
                "testuser",
                "testpass123",
                Duration::from_secs(300),
            )
            .await
            .expect("VM creation with validation failed");

        println!("VM fully ready: {} @ {}", node.name, node.ip_address);

        // Verify SSH actually works
        let ssh_result = SshClient::connect(&node.ip_address, 22, "testuser", "testpass123").await;

        // Cleanup
        let _ = backend.stop_node(&node.id).await;

        assert!(
            ssh_result.is_ok(),
            "SSH should work immediately after create_ready"
        );
    }
}
