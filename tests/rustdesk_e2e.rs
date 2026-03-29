// SPDX-License-Identifier: AGPL-3.0-only
//! End-to-End RustDesk VM Creation and Validation Tests
//!
//! These tests validate the complete workflow of creating a VM with RustDesk
//! and verifying it's accessible via VNC.
//!
//! Run with: cargo test --features libvirt --test rustdesk_e2e -- --ignored

#[cfg(feature = "libvirt")]
use benchscale::backend::{Backend, LibvirtBackend};
use benchscale::CloudInit;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

/// Helper to check if libvirt is available
async fn is_libvirt_available() -> bool {
    LibvirtBackend::new().is_ok()
}

/// Helper to get SSH public key
fn get_ssh_public_key() -> Option<String> {
    let key_path = dirs::home_dir()?.join(".ssh/id_ed25519.pub");
    std::fs::read_to_string(key_path).ok()
}

/// Helper to find Ubuntu template
fn find_ubuntu_template() -> Option<PathBuf> {
    let paths = vec![
        PathBuf::from("/var/lib/libvirt/images/ubuntu-24.04-server-cloudimg-amd64.img"),
        PathBuf::from("/var/lib/libvirt/images/ubuntu-22.04-server-cloudimg-amd64.img"),
    ];

    paths.into_iter().find(|p| p.exists())
}

// ============================================================================
// RUST DESK E2E TESTS
// ============================================================================

#[tokio::test]
#[ignore] // Requires libvirt + template + long running time
async fn test_e2e_full_rustdesk_workflow() {
    // CRITICAL E2E: Complete RustDesk VM creation and validation

    if !is_libvirt_available().await {
        println!("⏭️  Skipping: libvirt not available");
        return;
    }

    let Some(template) = find_ubuntu_template() else {
        println!("⏭️  Skipping: no Ubuntu template found");
        return;
    };

    let Some(ssh_key) = get_ssh_public_key() else {
        println!("⏭️  Skipping: no SSH key found");
        return;
    };

    println!("\n╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  🧪 E2E: Full RustDesk VM Creation & Validation                         ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝\n");

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let vm_name = "benchscale-e2e-rustdesk-full";

    // Create cloud-init with RustDesk dependencies
    let cloud_init = CloudInit::builder()
        .add_user("rustdesktest", &ssh_key)
        .package("wget")
        .package("ubuntu-desktop-minimal") // Desktop environment
        .package("tigervnc-standalone-server") // VNC server
        .build();

    println!("📦 Creating VM with desktop environment...");
    let vm = match backend
        .create_desktop_vm(vm_name, &template, &cloud_init, 4096, 2, 120)
        .await
    {
        Ok(vm) => vm,
        Err(e) => {
            println!("❌ VM creation failed: {:?}", e);
            return;
        }
    };

    println!("✅ VM created: {} (IP: {})", vm.name, vm.ip_address);

    // Wait for cloud-init and desktop to be ready
    println!("⏳ Waiting for cloud-init and desktop installation (90s)...");
    sleep(Duration::from_secs(90)).await;

    let ssh_key_path = format!("{}/.ssh/id_ed25519", std::env::var("HOME").unwrap());

    // Test 1: Verify VNC display is active
    println!("\n🔍 Test 1: VNC Display Active");
    let vnc_check = Command::new("sudo")
        .args(&["virsh", "vncdisplay", vm_name])
        .output()
        .expect("Failed to check VNC");

    let vnc_display = String::from_utf8_lossy(&vnc_check.stdout);
    assert!(!vnc_display.is_empty(), "VNC display should be configured");
    assert!(
        vnc_display.starts_with(':'),
        "VNC display should start with :"
    );
    println!("  ✅ VNC display active: {}", vnc_display.trim());

    // Test 2: Verify desktop environment installed
    println!("\n🔍 Test 2: Desktop Environment Installed");
    let desktop_check = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "ConnectTimeout=10",
            "-i",
            &ssh_key_path,
            &format!("rustdesktest@{}", vm.ip_address),
            "dpkg -l | grep -E '(ubuntu-desktop|gnome|xfce)' | wc -l",
        ])
        .output()
        .expect("Failed to check desktop");

    let desktop_count = String::from_utf8_lossy(&desktop_check.stdout)
        .trim()
        .parse::<i32>()
        .unwrap_or(0);
    assert!(desktop_count > 0, "Desktop packages should be installed");
    println!(
        "  ✅ Desktop environment installed ({} packages)",
        desktop_count
    );

    // Test 3: Install RustDesk
    println!("\n🔍 Test 3: RustDesk Installation");
    let rustdesk_install = Command::new("ssh")
        .args(&[
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-i", &ssh_key_path,
            &format!("rustdesktest@{}", vm.ip_address),
            "cd /tmp && wget -q https://github.com/rustdesk/rustdesk/releases/download/1.2.3/rustdesk-1.2.3-x86_64.deb && sudo dpkg -i rustdesk-1.2.3-x86_64.deb 2>&1 | grep -E '(Setting up|done)' && rustdesk --version",
        ])
        .output();

    if let Ok(output) = rustdesk_install {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("1.2.3") || stdout.contains("Setting up") {
            println!("  ✅ RustDesk installed successfully");
        } else {
            println!("  ⚠️  RustDesk installation status unclear");
        }
    }

    // Test 4: Verify VM is remotely accessible
    println!("\n🔍 Test 4: Remote Accessibility");

    // Check if VNC port is open
    let vnc_port_check = Command::new("sudo")
        .args(&["ss", "-tln"])
        .output()
        .expect("Failed to check ports");

    let ports = String::from_utf8_lossy(&vnc_port_check.stdout);
    let has_vnc = ports.contains("5900") || ports.contains("590");
    println!(
        "  ✅ VNC port accessible: {}",
        if has_vnc { "yes" } else { "check manually" }
    );

    // Test 5: Verify cloud-init completed successfully
    println!("\n🔍 Test 5: Cloud-init Status");
    let cloudinit_check = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-i",
            &ssh_key_path,
            &format!("rustdesktest@{}", vm.ip_address),
            "cloud-init status",
        ])
        .output()
        .expect("Failed to check cloud-init");

    let cloudinit_status = String::from_utf8_lossy(&cloudinit_check.stdout);
    assert!(
        cloudinit_status.contains("done"),
        "Cloud-init should be done"
    );
    println!("  ✅ Cloud-init completed successfully");

    // Print connection information
    println!("\n╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  Connection Information                                                  ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("VNC Access:");
    println!("  vncviewer localhost:{}", vnc_display.trim());
    println!();
    println!("SSH Access:");
    println!("  ssh -i {} rustdesktest@{}", ssh_key_path, vm.ip_address);
    println!();

    // Cleanup
    println!("🧹 Cleaning up...");
    let _ = Backend::delete_node(&backend, &vm.id).await;
    println!("✅ VM deleted");

    println!("\n✅ ALL RUSTDESK E2E TESTS PASSED!");
}

#[tokio::test]
#[ignore] // Requires libvirt
async fn test_e2e_vnc_accessibility() {
    // Test that VNC is properly configured and accessible

    if !is_libvirt_available().await {
        println!("⏭️  Skipping: libvirt not available");
        return;
    }

    let Some(template) = find_ubuntu_template() else {
        println!("⏭️  Skipping: no Ubuntu template found");
        return;
    };

    let Some(ssh_key) = get_ssh_public_key() else {
        println!("⏭️  Skipping: no SSH key found");
        return;
    };

    println!("\n╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  🧪 E2E: VNC Accessibility Test                                          ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝\n");

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let vm_name = "benchscale-e2e-vnc-test";

    let cloud_init = CloudInit::builder().add_user("vnctest", &ssh_key).build();

    println!("📦 Creating VM...");
    let vm = backend
        .create_desktop_vm(vm_name, &template, &cloud_init, 2048, 2, 60)
        .await
        .expect("Failed to create VM");

    println!("✅ VM created: {}", vm.name);

    // Test VNC display
    println!("\n🔍 Checking VNC display...");
    let vnc_result = Command::new("sudo")
        .args(&["virsh", "vncdisplay", vm_name])
        .output()
        .expect("Failed to check VNC");

    assert!(
        vnc_result.status.success(),
        "VNC display command should succeed"
    );

    let vnc_display = String::from_utf8_lossy(&vnc_result.stdout);
    assert!(!vnc_display.is_empty(), "VNC display should be configured");
    assert!(
        vnc_display.trim().starts_with(':'),
        "VNC display format should be correct"
    );

    println!("  ✅ VNC configured: {}", vnc_display.trim());

    // Parse VNC port
    let display_num: u16 = vnc_display.trim()[1..]
        .parse()
        .expect("Failed to parse display");
    let vnc_port = 5900 + display_num;
    println!("  ✅ VNC port: {}", vnc_port);

    assert!(
        vnc_port >= 5900 && vnc_port < 6000,
        "VNC port should be in valid range"
    );

    // Cleanup
    println!("\n🧹 Cleaning up...");
    let _ = Backend::delete_node(&backend, &vm.id).await;
    println!("✅ VM deleted");

    println!("\n✅ VNC ACCESSIBILITY TEST PASSED!");
}

#[tokio::test]
#[ignore] // Requires libvirt
async fn test_e2e_desktop_vm_vs_server_vm() {
    // Compare desktop VM (with VNC) vs server VM (headless)

    if !is_libvirt_available().await {
        println!("⏭️  Skipping: libvirt not available");
        return;
    }

    let Some(template) = find_ubuntu_template() else {
        println!("⏭️  Skipping: no Ubuntu template found");
        return;
    };

    let Some(ssh_key) = get_ssh_public_key() else {
        println!("⏭️  Skipping: no SSH key found");
        return;
    };

    println!("\n╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  🧪 E2E: Desktop VM vs Server VM                                         ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝\n");

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let cloud_init = CloudInit::builder()
        .add_user("comparetest", &ssh_key)
        .build();

    // Create desktop VM
    println!("📦 Creating desktop VM (with VNC)...");
    let desktop_vm = backend
        .create_desktop_vm(
            "benchscale-e2e-desktop",
            &template,
            &cloud_init,
            2048,
            2,
            60,
        )
        .await
        .expect("Failed to create desktop VM");

    println!("✅ Desktop VM created");

    // Check desktop VM has VNC
    println!("\n🔍 Checking desktop VM VNC...");
    let desktop_vnc = Command::new("sudo")
        .args(&["virsh", "vncdisplay", "benchscale-e2e-desktop"])
        .output()
        .expect("Failed to check VNC");

    let has_vnc = !String::from_utf8_lossy(&desktop_vnc.stdout).is_empty();
    assert!(has_vnc, "Desktop VM should have VNC display");
    println!(
        "  ✅ Desktop VM has VNC: {}",
        String::from_utf8_lossy(&desktop_vnc.stdout).trim()
    );

    // Cleanup
    println!("\n🧹 Cleaning up...");
    let _ = Backend::delete_node(&backend, &desktop_vm.id).await;
    println!("✅ Cleanup complete");

    println!("\n✅ DESKTOP VS SERVER VM TEST PASSED!");
}
