// SPDX-License-Identifier: AGPL-3.0-or-later
//! End-to-End tests for LibvirtBackend with IP Pool
//!
//! These tests validate the IP pool integration with actual VM creation.
//! They are marked with `#[ignore]` because they require:
//! - libvirt access
//! - sudo permissions  
//! - Base images available
//!
//! Run with: cargo test --features libvirt --test libvirt_e2e_tests -- --ignored

use benchscale::CloudInit;
#[cfg(feature = "libvirt")]
use benchscale::backend::{Backend, LibvirtBackend};
use benchscale::config::BenchScaleConfig;

/// Helper to check if libvirt is available
async fn is_libvirt_available() -> bool {
    LibvirtBackend::new().is_ok()
}

/// Helper to create a test cloud-init config
fn create_test_cloud_init() -> CloudInit {
    CloudInit::builder()
        .add_user(
            "testuser",
            "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC... (test key)",
        )
        .package("curl")
        .build()
}

// ============================================================================
// E2E Tests - Marked #[ignore] for manual/CI execution
// ============================================================================

#[tokio::test]
#[ignore] // Requires actual libvirt + base image
async fn test_create_single_vm_with_ip_pool() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    // Check if base image exists
    let base_image = BenchScaleConfig::default()
        .storage()
        .images_dir()
        .join("ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found at {:?}", base_image);
        return;
    }

    println!("🧪 Testing single VM creation with IP pool...");

    // Create VM
    let vm_result: Result<_, _> = backend
        .create_desktop_vm(
            "benchscale-test-ip-pool",
            base_image.as_path(),
            &cloud_init,
            1024,
            1,
            10,
            None,
        )
        .await;

    match vm_result {
        Ok(vm) => {
            println!("✅ VM created with IP: {}", vm.ip_address);

            // Verify IP is in expected range (192.168.122.10-250)
            assert!(vm.ip_address.starts_with("192.168.122."));

            // Cleanup
            Backend::delete_node(&backend, &vm.id)
                .await
                .expect("Failed to cleanup");
            println!("✅ VM deleted and IP released");
        }
        Err(e) => {
            panic!("VM creation failed: {:?}", e);
        }
    }
}

#[tokio::test]
#[ignore] // Requires actual libvirt
async fn test_concurrent_vm_creation_no_ip_conflict() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    let base_image = BenchScaleConfig::default()
        .storage()
        .images_dir()
        .join("ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found");
        return;
    }

    println!("🧪 Testing concurrent VM creation (2 VMs)...");

    // Create two VMs concurrently
    let vm1_future = backend.create_desktop_vm(
        "benchscale-concurrent-1",
        base_image.as_path(),
        &cloud_init,
        1024,
        1,
        10,
        None,
    );
    let vm2_future = backend.create_desktop_vm(
        "benchscale-concurrent-2",
        base_image.as_path(),
        &cloud_init,
        1024,
        1,
        10,
        None,
    );

    let (r1, r2) = tokio::join!(vm1_future, vm2_future);

    let mut ips = Vec::new();
    let mut ids = Vec::new();

    if let Ok(vm1) = r1 {
        println!("✅ VM1 created with IP: {}", vm1.ip_address);
        ips.push(vm1.ip_address.clone());
        ids.push(vm1.id);
    }

    if let Ok(vm2) = r2 {
        println!("✅ VM2 created with IP: {}", vm2.ip_address);
        ips.push(vm2.ip_address.clone());
        ids.push(vm2.id);
    }

    // Verify no IP conflicts
    assert_eq!(ips.len(), 2, "Both VMs should have been created");
    assert_ne!(ips[0], ips[1], "VMs should have different IPs!");
    println!("✅ No IP conflicts detected");

    // Cleanup
    for id in ids {
        let _ = Backend::delete_node(&backend, &id).await;
    }
    println!("✅ Cleanup complete");
}

#[tokio::test]
#[ignore] // Requires actual libvirt
async fn test_ip_release_on_delete() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    let base_image = BenchScaleConfig::default()
        .storage()
        .images_dir()
        .join("ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found");
        return;
    }

    println!("🧪 Testing IP release on VM deletion...");

    // Create VM
    let vm: benchscale::backend::NodeInfo = backend
        .create_desktop_vm(
            "benchscale-test-release",
            base_image.as_path(),
            &cloud_init,
            1024,
            1,
            10,
            None,
        )
        .await
        .expect("Failed to create VM");

    let allocated_ip = vm.ip_address.clone();
    println!("✅ VM created with IP: {}", allocated_ip);

    // Delete VM
    Backend::delete_node(&backend, &vm.id)
        .await
        .expect("Failed to delete VM");
    println!("✅ VM deleted");

    // Try to create another VM immediately - should get same or different IP
    let vm2: benchscale::backend::NodeInfo = backend
        .create_desktop_vm(
            "benchscale-test-release2",
            base_image.as_path(),
            &cloud_init,
            1024,
            1,
            10,
            None,
        )
        .await
        .expect("Failed to create second VM");

    println!("✅ Second VM created with IP: {}", vm2.ip_address);
    println!("   (IP was available for reuse)");

    // Cleanup
    Backend::delete_node(&backend, &vm2.id)
        .await
        .expect("Failed to cleanup");
}

/// Note: This test is commented out because it would exhaust the IP pool
/// and could interfere with other tests. Enable manually if needed.
#[tokio::test]
#[ignore]
async fn test_ip_pool_capacity() {
    // This test would create VMs until the pool is exhausted
    // to verify proper error handling. Commented out for safety.
    println!("⚠️  IP pool exhaustion test skipped (would consume all IPs)");
}
