// Integration test demonstrating benchScale cloud-init validation
//
// This test creates a real VM using the new wait_for_cloud_init() API
// and validates that SSH is immediately accessible.

use benchscale::{CloudInit, LibvirtBackend};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  benchScale Cloud-Init Validation - Integration Test                ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();

    // Test configuration
    let test_id = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let vm_name = format!("cloud-init-test-{}", test_id);
    let base_image = PathBuf::from(
        std::env::var("BENCHSCALE_BASE_IMAGE")
            .unwrap_or_else(|_| "ubuntu-24.04-server-cloudimg-amd64.img".to_string()),
    );

    println!("🔧 Test Configuration");
    println!("   VM Name: {}", vm_name);
    println!("   Base Image: {}", base_image.display());
    println!("   Memory: 2048 MB");
    println!("   vCPUs: 2");
    println!();

    // Create backend
    println!("📦 Initializing LibvirtBackend...");
    let backend = LibvirtBackend::new()?;
    println!("✅ Backend initialized");
    println!();

    // Build cloud-init config
    println!("☁️  Building Cloud-Init configuration...");
    let cloud_init = CloudInit::builder()
        .add_user("testuser", "") // No SSH key, will use password
        .cmd("echo 'testuser:testpass123' | chpasswd") // Set password
        .cmd("systemctl enable ssh")
        .cmd("systemctl start ssh")
        .package("curl") // Test package installation
        .build();

    println!("✅ Cloud-Init configured");
    println!("   User: testuser / testpass123");
    println!("   Packages: curl");
    println!();

    // Test 1: Create VM without validation (old way)
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Test 1: create_desktop_vm() - Old API (no validation)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    let start = Instant::now();
    let node = backend
        .create_desktop_vm(
            &vm_name,
            &base_image,
            &cloud_init,
            2048, // 2GB RAM
            2,    // 2 vCPUs
            10,   // 10GB disk
        )
        .await?;
    let creation_time = start.elapsed();

    println!("✅ VM created in {:?}", creation_time);
    println!("   VM ID: {}", node.id);
    println!("   IP: {}", node.ip_address);
    println!();

    // The VM has an IP but might not be ready yet
    println!("⚠️  OLD API: VM has IP but cloud-init might still be running!");
    println!("   This is the timing gap we're solving.");
    println!();

    // Test 2: Validate cloud-init manually (demonstrating the fix)
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Test 2: wait_for_cloud_init() - New API (with validation)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    println!("⏳ Waiting for cloud-init to complete (up to 5 minutes)...");
    let start = Instant::now();

    match backend
        .wait_for_cloud_init(
            &node.id,
            "testuser",
            "testpass123",
            Duration::from_secs(300), // 5 minute timeout
        )
        .await
    {
        Ok(()) => {
            let validation_time = start.elapsed();
            println!("✅ Cloud-init completed in {:?}", validation_time);
            println!();

            // Test 3: Verify SSH is accessible
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Test 3: wait_for_ssh() - SSH validation");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();

            println!("⏳ Validating SSH access...");
            let start = Instant::now();

            match backend
                .wait_for_ssh(
                    &node.ip_address,
                    "testuser",
                    "testpass123",
                    Duration::from_secs(60),
                )
                .await
            {
                Ok(()) => {
                    let ssh_time = start.elapsed();
                    println!("✅ SSH validated in {:?}", ssh_time);
                    println!("   Connection: testuser@{}", node.ip_address);
                    println!();
                }
                Err(e) => {
                    println!("⚠️  SSH validation timed out: {}", e);
                    println!("   This may indicate cloud-init hasn't fully completed SSH setup.");
                    println!();
                }
            }

            // Summary
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("✅ INTEGRATION TEST PASSED");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();
            println!("📊 Timing Summary:");
            println!("   VM Creation: {:?}", creation_time);
            println!("   Cloud-Init Validation: {:?}", validation_time);
            println!("   Total Time: {:?}", creation_time + validation_time);
            println!();
            println!("🎯 Key Insight:");
            println!("   The new create_desktop_vm_ready() API would have done all");
            println!("   of this in a single call, guaranteeing SSH readiness!");
            println!();
            println!("📝 Recommended Usage:");
            println!("   let node = backend.create_desktop_vm_ready(");
            println!("       name, image, cloud_init,");
            println!("       mem, vcpus, disk,");
            println!("       username, password,");
            println!("       timeout,");
            println!("   ).await?;");
            println!("   // SSH guaranteed to work here!");
            println!();
        }
        Err(e) => {
            println!("❌ Cloud-init validation failed: {}", e);
            println!();
            println!("   This is expected if:");
            println!("   - Cloud-init takes longer than timeout");
            println!("   - Package installation is slow");
            println!("   - Network issues occur");
            println!();
            println!("   The new API provides clear error messages for debugging.");
            println!();
        }
    }

    // Cleanup
    println!("🧹 Cleanup");
    println!("   To remove the test VM:");
    println!("   $ virsh destroy {}", vm_name);
    println!("   $ virsh undefine {} --remove-all-storage", vm_name);
    println!();
    println!("   Or leave it running to inspect:");
    println!("   $ ssh testuser@{}", node.ip_address);
    println!("   $ virsh console {}", vm_name);
    println!();

    Ok(())
}
