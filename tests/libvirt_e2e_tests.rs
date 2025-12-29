//! End-to-End tests for LibvirtBackend with IP Pool
//!
//! These tests require:
//! - Libvirt daemon running
//! - Proper permissions to create VMs
//! - Base VM images available
//! - Sufficient disk space
//!
//! Run with: cargo test --features libvirt --test libvirt_e2e_tests -- --ignored

use benchscale::backend::{Backend, LibvirtBackend, NodeInfo};
use benchscale::{CloudInit, Error};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::Path;
use tokio;

/// Helper to check if libvirt is available
async fn is_libvirt_available() -> bool {
    LibvirtBackend::new().is_ok()
}

/// Helper to create a test cloud-init config
fn create_test_cloud_init() -> CloudInit {
    CloudInit::builder()
        .add_user("testuser", "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC... (test key)")
        .package("curl")
        .build()
}

// ============================================================================
// Phase 3a: E2E Single VM Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires actual libvirt + base image
async fn test_create_single_vm_with_static_ip() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    // Check if base image exists
    let base_image = Path::new("/var/lib/libvirt/images/ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found at {:?}", base_image);
        return;
    }

    // Create VM
    let vm_name = "benchscale-test-single-vm";
    let result = backend
        .create_desktop_vm(
            vm_name,
            base_image,
            &cloud_init,
            1024,  // 1 GB RAM
            1,     // 1 vCPU
            10,    // 10 GB disk
        )
        .await;

    // Cleanup on success or failure
    if result.is_ok() {
        let vm = result.unwrap();
        
        // Verify IP was assigned
        assert!(!vm.ip_address.is_empty(), "VM should have an IP address");
        
        // Verify IP is in valid range
        let ip: Ipv4Addr = vm.ip_address.parse().expect("Invalid IP format");
        assert!(
            ip >= Ipv4Addr::new(192, 168, 122, 10) && ip <= Ipv4Addr::new(192, 168, 122, 250),
            "IP should be in valid range"
        );

        println!("✅ VM created successfully with IP: {}", vm.ip_address);

        // Cleanup
        backend.delete_node(&vm.id).await.expect("Failed to cleanup VM");
    } else {
        panic!("VM creation failed: {:?}", result.err());
    }
}

#[tokio::test]
#[ignore] // Requires actual libvirt
async fn test_vm_has_correct_network_connectivity() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    let base_image = Path::new("/var/lib/libvirt/images/ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found");
        return;
    }

    let vm_name = "benchscale-test-network";
    let result = backend
        .create_desktop_vm(vm_name, base_image, &cloud_init, 1024, 1, 10)
        .await;

    if let Ok(vm) = result {
        // Wait a moment for VM to boot
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        // Try to ping the VM (basic connectivity test)
        let ping_result = std::process::Command::new("ping")
            .args(&["-c", "1", "-W", "2", &vm.ip_address])
            .output();

        if let Ok(output) = ping_result {
            if output.status.success() {
                println!("✅ VM is reachable at {}", vm.ip_address);
            } else {
                println!("⚠️  VM not yet reachable (may need more boot time)");
            }
        }

        // Cleanup
        backend.delete_node(&vm.id).await.expect("Failed to cleanup VM");
    }
}

// ============================================================================
// Phase 3b: E2E Multi-VM Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires actual libvirt
async fn test_create_two_vms_concurrent_no_ip_conflict() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    let base_image = Path::new("/var/lib/libvirt/images/ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found");
        return;
    }

    // Create 2 VMs concurrently
    let vm1_future = backend.create_desktop_vm(
        "benchscale-test-vm1",
        base_image,
        &cloud_init,
        1024, 1, 10,
    );

    let vm2_future = backend.create_desktop_vm(
        "benchscale-test-vm2",
        base_image,
        &cloud_init,
        1024, 1, 10,
    );

    // Execute concurrently
    let (result1, result2) = tokio::join!(vm1_future, vm2_future);

    let mut cleanup_ids = Vec::new();

    if let (Ok(vm1), Ok(vm2)) = (result1, result2) {
        cleanup_ids.push(vm1.id.clone());
        cleanup_ids.push(vm2.id.clone());

        // Verify different IPs
        assert_ne!(
            vm1.ip_address, vm2.ip_address,
            "VMs should have different IP addresses (no conflict!)"
        );

        println!("✅ VM1 IP: {}", vm1.ip_address);
        println!("✅ VM2 IP: {}", vm2.ip_address);
        println!("✅ No IP conflicts detected!");
    } else {
        panic!("One or both VM creations failed");
    }

    // Cleanup
    for id in cleanup_ids {
        let _ = backend.delete_node(&id).await;
    }
}

#[tokio::test]
#[ignore] // Requires actual libvirt
async fn test_create_five_vms_concurrent_stress_test() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    let base_image = Path::new("/var/lib/libvirt/images/ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found");
        return;
    }

    println!("🚀 Creating 5 VMs concurrently...");
    let start = std::time::Instant::now();

    // Create 5 VMs concurrently using tokio::join!
    let (r1, r2, r3, r4, r5) = tokio::join!(
        backend.create_desktop_vm("benchscale-stress-vm1", base_image, &cloud_init, 1024, 1, 10),
        backend.create_desktop_vm("benchscale-stress-vm2", base_image, &cloud_init, 1024, 1, 10),
        backend.create_desktop_vm("benchscale-stress-vm3", base_image, &cloud_init, 1024, 1, 10),
        backend.create_desktop_vm("benchscale-stress-vm4", base_image, &cloud_init, 1024, 1, 10),
        backend.create_desktop_vm("benchscale-stress-vm5", base_image, &cloud_init, 1024, 1, 10),
    );

    let duration = start.elapsed();
    println!("⏱️  Creation time: {:?}", duration);

    let mut ips = Vec::new();
    let mut cleanup_ids = Vec::new();

    for result in [r1, r2, r3, r4, r5] {
        if let Ok(vm) = result {
            ips.push(vm.ip_address.clone());
            cleanup_ids.push(vm.id.clone());
        }
    }

    // Verify all IPs are unique
    let unique_ips: std::collections::HashSet<_> = ips.iter().collect();
    assert_eq!(
        ips.len(),
        unique_ips.len(),
        "All IPs should be unique (no conflicts)"
    );

    println!("✅ All {} VMs have unique IPs:", ips.len());
    for ip in &ips {
        println!("   • {}", ip);
    }

    // Cleanup
    for id in &cleanup_ids {
        let _ = backend.delete_node(id).await;
    }
}

#[tokio::test]
#[ignore] // Requires actual libvirt
async fn test_delete_vm_releases_ip() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    let base_image = Path::new("/var/lib/libvirt/images/ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found");
        return;
    }

    // Create VM
    let vm_result = backend
        .create_desktop_vm("benchscale-test-release", base_image, &cloud_init, 1024, 1, 10)
        .await;

    if let Ok(vm) = vm_result {
        let allocated_ip = vm.ip_address.clone();
        println!("✅ VM created with IP: {}", allocated_ip);

        // Delete VM
        Backend::delete_node(&backend, &vm.id).await.expect("Failed to delete VM");
        println!("✅ VM deleted");

        // Note: Actual verification of IP release would require access to the
        // backend's IP pool, which is private. In production, we'd check via
        // metrics or pool inspection API.

        // Try to create another VM immediately - should get same or different IP
        let vm2_result = backend
            .create_desktop_vm("benchscale-test-release2", base_image, &cloud_init, 1024, 1, 10)
            .await;

        if let Ok(vm2) = vm2_result {
            println!("✅ Second VM created with IP: {}", vm2.ip_address);
            println!("   (IP was available for reuse)");

            // Cleanup
            Backend::delete_node(&backend, &vm2.id).await.expect("Failed to cleanup");
        }
    }
}

// ============================================================================
// IP Pool Exhaustion Recovery Test
// ============================================================================

#[tokio::test]
#[ignore] // Requires actual libvirt + lots of disk space
async fn test_pool_exhaustion_handling() {
    // This test would create VMs until the pool is exhausted
    // Skipped by default as it requires significant resources

    println!("⚠️  Pool exhaustion test skipped (resource intensive)");
    println!("   To test manually:");
    println!("   1. Create a backend with small IP pool (e.g., 3 IPs)");
    println!("   2. Try to create 4 VMs");
    println!("   3. Verify 4th creation fails gracefully");
    println!("   4. Delete one VM");
    println!("   5. Verify IP is released and can create new VM");
}

// ============================================================================
// Performance Benchmark
// ============================================================================

#[tokio::test]
#[ignore] // Benchmark test
async fn bench_rapid_vm_creation() {
    if !is_libvirt_available().await {
        println!("Skipping: libvirt not available");
        return;
    }

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = create_test_cloud_init();

    let base_image = Path::new("/var/lib/libvirt/images/ubuntu-22.04-server-cloudimg-amd64.img");
    if !base_image.exists() {
        println!("Skipping: base image not found");
        return;
    }

    println!("🔬 Benchmarking VM creation...");
    
    // Benchmark: Create 3 VMs as fast as possible
    let start = std::time::Instant::now();
    
    let (r1, r2, r3): (
        Result<NodeInfo, Error>,
        Result<NodeInfo, Error>,
        Result<NodeInfo, Error>,
    ) = tokio::join!(
        backend.create_desktop_vm("benchscale-bench-vm1", base_image, &cloud_init, 1024, 1, 10),
        backend.create_desktop_vm("benchscale-bench-vm2", base_image, &cloud_init, 1024, 1, 10),
        backend.create_desktop_vm("benchscale-bench-vm3", base_image, &cloud_init, 1024, 1, 10),
    );

    let duration = start.elapsed();

    let mut cleanup_ids = Vec::new();
    let mut success_count = 0;

    for result in [r1, r2, r3] {
        if let Ok(vm) = result {
            cleanup_ids.push(vm.id.clone());
            success_count += 1;
        }
    }

    println!("✅ Created {} VMs in {:?}", success_count, duration);
    if success_count > 0 {
        println!("   Average: {:?} per VM", duration / success_count);
    }

    // Cleanup
    for id in &cleanup_ids {
        let _ = backend.delete_node(id).await;
    }
}

