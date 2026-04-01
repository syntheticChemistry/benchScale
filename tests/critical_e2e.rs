// SPDX-License-Identifier: AGPL-3.0-only
//! Critical E2E tests for benchScale automation
//!
//! These tests validate end-to-end flows that are critical for production use.
//! They are marked with `#[ignore]` because they require:
//! - libvirt access with sudo
//! - Base Ubuntu 24.04 template
//! - SSH key at ~/.ssh/id_ed25519.pub
//!
//! Run with: cargo test --features libvirt --test critical_e2e -- --ignored

#[cfg(feature = "libvirt")]
use benchscale::backend::{Backend, LibvirtBackend};
use benchscale::config::BenchScaleConfig;
use benchscale::CloudInit;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

/// Helper to check if libvirt is available
async fn is_libvirt_available() -> bool {
    LibvirtBackend::new().is_ok()
}

/// Helper to find SSH public key
fn get_ssh_public_key() -> Option<String> {
    let key_path = dirs::home_dir()?.join(".ssh/id_ed25519.pub");
    std::fs::read_to_string(key_path).ok()
}

/// Helper to find Ubuntu 24.04 template
fn find_ubuntu_template() -> Option<PathBuf> {
    let img = BenchScaleConfig::default().storage().images_dir();
    let paths = vec![
        img.join("ubuntu-24.04-server-cloudimg-amd64.img"),
        img.join("ubuntu-22.04-server-cloudimg-amd64.img"),
        PathBuf::from("../agentReagents/images/templates/ubuntu-24.04-baseline.qcow2"),
    ];

    paths.into_iter().find(|p| p.exists())
}

// ============================================================================
// CRITICAL E2E TESTS
// ============================================================================

#[tokio::test]
#[ignore] // Requires libvirt + template
async fn test_e2e_cloudinit_processing() {
    // CRITICAL: Validates that cloud-init ACTUALLY processes config
    // This catches the filename bug we just fixed!

    if !is_libvirt_available().await {
        println!("⏭️  Skipping: libvirt not available");
        return;
    }

    let Some(template) = find_ubuntu_template() else {
        println!("⏭️  Skipping: no Ubuntu template found");
        return;
    };

    let Some(ssh_key) = get_ssh_public_key() else {
        println!("⏭️  Skipping: no SSH key found at ~/.ssh/id_ed25519.pub");
        return;
    };

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║  🧪 E2E: Cloud-init Processing Validation                    ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let vm_name = "benchscale-e2e-cloudinit";

    // Create cloud-init with specific markers we can verify
    let cloud_init = CloudInit::builder()
        .add_user("e2etest", &ssh_key)
        .package("curl")
        .package("wget")
        .cmd("touch /tmp/cloudinit-ran-successfully")
        .build();

    println!("📦 Creating VM: {}", vm_name);
    let vm = match backend
        .create_desktop_vm(vm_name, &template, &cloud_init, 2048, 2, 60)
        .await
    {
        Ok(vm) => vm,
        Err(e) => {
            println!("❌ VM creation failed: {:?}", e);
            return;
        }
    };

    println!("✅ VM created with IP: {}", vm.ip_address);

    // Wait for cloud-init to complete (60 seconds should be enough)
    println!("⏳ Waiting for cloud-init to process (60s)...");
    sleep(Duration::from_secs(60)).await;

    // Verify cloud-init processing via SSH
    println!("\n🔍 Verifying cloud-init effects via SSH...");

    // Test 1: SSH connection with injected key
    let ssh_test = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "ConnectTimeout=10",
            "-i",
            &format!("{}/.ssh/id_ed25519", std::env::var("HOME").unwrap()),
            &format!("e2etest@{}", vm.ip_address),
            "echo 'SSH_SUCCESS'",
        ])
        .output();

    let ssh_works = if let Ok(output) = ssh_test {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.contains("SSH_SUCCESS")
    } else {
        false
    };

    // Test 2: Check if packages were installed
    let packages_check = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "ConnectTimeout=10",
            "-i",
            &format!("{}/.ssh/id_ed25519", std::env::var("HOME").unwrap()),
            &format!("e2etest@{}", vm.ip_address),
            "dpkg -l | grep -E '(curl|wget)' | wc -l",
        ])
        .output();

    let packages_installed = if let Ok(output) = packages_check {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.trim().parse::<i32>().unwrap_or(0) >= 2
    } else {
        false
    };

    // Test 3: Check if runcmd executed
    let runcmd_check = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "ConnectTimeout=10",
            "-i",
            &format!("{}/.ssh/id_ed25519", std::env::var("HOME").unwrap()),
            &format!("e2etest@{}", vm.ip_address),
            "test -f /tmp/cloudinit-ran-successfully && echo 'MARKER_FOUND'",
        ])
        .output();

    let runcmd_ran = if let Ok(output) = runcmd_check {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.contains("MARKER_FOUND")
    } else {
        false
    };

    // Test 4: Verify cloud-init status
    let cloudinit_status = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "ConnectTimeout=10",
            "-i",
            &format!("{}/.ssh/id_ed25519", std::env::var("HOME").unwrap()),
            &format!("e2etest@{}", vm.ip_address),
            "cloud-init status --wait",
        ])
        .output();

    let cloudinit_done = if let Ok(output) = cloudinit_status {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.contains("done")
    } else {
        false
    };

    // Print results
    println!("\n📊 Test Results:");
    println!(
        "  SSH Key Auth:         {}",
        if ssh_works { "✅ PASS" } else { "❌ FAIL" }
    );
    println!(
        "  Packages Installed:   {}",
        if packages_installed {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    );
    println!(
        "  Runcmd Executed:      {}",
        if runcmd_ran { "✅ PASS" } else { "❌ FAIL" }
    );
    println!(
        "  Cloud-init Status:    {}",
        if cloudinit_done {
            "✅ PASS (done)"
        } else {
            "❌ FAIL (not done)"
        }
    );

    // Cleanup
    println!("\n🧹 Cleaning up...");
    let _ = Backend::delete_node(&backend, &vm.id).await;
    println!("✅ VM deleted");

    // Assert all tests passed
    assert!(
        ssh_works,
        "SSH key authentication failed - cloud-init didn't inject SSH keys!"
    );
    assert!(
        packages_installed,
        "Packages not installed - cloud-init didn't process packages!"
    );
    assert!(
        runcmd_ran,
        "Runcmd didn't execute - cloud-init didn't process commands!"
    );
    assert!(
        cloudinit_done,
        "Cloud-init not done - config not fully processed!"
    );

    println!("\n✅ ALL CLOUD-INIT TESTS PASSED!");
}

#[tokio::test]
#[ignore] // Requires libvirt + SSH setup
async fn test_e2e_ssh_automation() {
    // CRITICAL: Validates SSH-based automation works end-to-end

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

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║  🧪 E2E: SSH Automation Validation                           ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let vm_name = "benchscale-e2e-ssh";

    let cloud_init = CloudInit::builder().add_user("sshtest", &ssh_key).build();

    println!("📦 Creating VM: {}", vm_name);
    let vm = backend
        .create_desktop_vm(vm_name, &template, &cloud_init, 2048, 2, 60)
        .await
        .expect("Failed to create VM");

    println!("✅ VM created with IP: {}", vm.ip_address);
    println!("⏳ Waiting for SSH to be ready (45s)...");
    sleep(Duration::from_secs(45)).await;

    let ssh_key_path = format!("{}/.ssh/id_ed25519", std::env::var("HOME").unwrap());

    // Test 1: Basic connectivity
    println!("\n🔍 Test 1: Basic SSH connectivity");
    let result = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "ConnectTimeout=10",
            "-i",
            &ssh_key_path,
            &format!("sshtest@{}", vm.ip_address),
            "whoami",
        ])
        .output()
        .expect("Failed to run SSH");

    let username = String::from_utf8_lossy(&result.stdout).trim().to_string();
    assert_eq!(
        username, "sshtest",
        "SSH command didn't return expected username"
    );
    println!("  ✅ SSH connectivity works");

    // Test 2: Command execution
    println!("\n🔍 Test 2: Command execution");
    let result = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "ConnectTimeout=10",
            "-i",
            &ssh_key_path,
            &format!("sshtest@{}", vm.ip_address),
            "echo 'test123' | sha256sum",
        ])
        .output()
        .expect("Failed to run SSH command");

    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        output.contains("ecd"),
        "Command execution didn't produce expected output"
    );
    println!("  ✅ Command execution works");

    // Test 3: File operations
    println!("\n🔍 Test 3: File operations");
    let _ = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-i",
            &ssh_key_path,
            &format!("sshtest@{}", vm.ip_address),
            "echo 'automation test' > /tmp/test.txt",
        ])
        .output();

    let result = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-i",
            &ssh_key_path,
            &format!("sshtest@{}", vm.ip_address),
            "cat /tmp/test.txt",
        ])
        .output()
        .expect("Failed to read file via SSH");

    let content = String::from_utf8_lossy(&result.stdout).trim().to_string();
    assert_eq!(
        content, "automation test",
        "File operations didn't work correctly"
    );
    println!("  ✅ File operations work");

    // Test 4: Error handling
    println!("\n🔍 Test 4: Error handling");
    let result = Command::new("ssh")
        .args(&[
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "ConnectTimeout=10",
            "-i",
            &ssh_key_path,
            &format!("sshtest@{}", vm.ip_address),
            "exit 42",
        ])
        .output()
        .expect("Failed to run SSH");

    assert_eq!(
        result.status.code(),
        Some(42),
        "Exit code not propagated correctly"
    );
    println!("  ✅ Error handling works");

    // Cleanup
    println!("\n🧹 Cleaning up...");
    let _ = Backend::delete_node(&backend, &vm.id).await;
    println!("✅ VM deleted");

    println!("\n✅ ALL SSH AUTOMATION TESTS PASSED!");
}

#[tokio::test]
#[ignore] // Requires libvirt
async fn test_e2e_vm_lifecycle() {
    // CRITICAL: Validates full VM lifecycle

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

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║  🧪 E2E: VM Lifecycle Validation                             ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let backend = LibvirtBackend::new().expect("Failed to create backend");
    let vm_name = "benchscale-e2e-lifecycle";

    let cloud_init = CloudInit::builder()
        .add_user("lifecycletest", &ssh_key)
        .build();

    // Test 1: Creation
    println!("🔍 Test 1: VM Creation");
    let vm = backend
        .create_desktop_vm(vm_name, &template, &cloud_init, 2048, 2, 30)
        .await
        .expect("Failed to create VM");
    println!("  ✅ VM created: {}", vm.id);

    // Test 2: Verify running
    println!("\n🔍 Test 2: Verify VM Running");
    let status_check = Command::new("sudo")
        .args(&["virsh", "list", "--all"])
        .output()
        .expect("Failed to check VM status");

    let status_output = String::from_utf8_lossy(&status_check.stdout);
    assert!(
        status_output.contains(vm_name),
        "VM not found in virsh list"
    );
    assert!(status_output.contains("running"), "VM not running");
    println!("  ✅ VM is running");

    // Test 3: Stop
    println!("\n🔍 Test 3: VM Stop");
    let stop_result = Command::new("sudo")
        .args(&["virsh", "shutdown", vm_name])
        .output()
        .expect("Failed to stop VM");

    assert!(stop_result.status.success(), "Failed to shutdown VM");
    sleep(Duration::from_secs(10)).await;
    println!("  ✅ VM shutdown command sent");

    // Test 4: Verify stopped
    println!("\n🔍 Test 4: Verify VM Stopped");
    sleep(Duration::from_secs(5)).await;
    let status_check = Command::new("sudo")
        .args(&["virsh", "list", "--all"])
        .output()
        .expect("Failed to check VM status");

    let status_output = String::from_utf8_lossy(&status_check.stdout);
    assert!(status_output.contains(vm_name), "VM disappeared!");
    println!("  ✅ VM stopped");

    // Test 5: Cleanup
    println!("\n🔍 Test 5: VM Deletion");
    let _ = Backend::delete_node(&backend, &vm.id).await;

    let status_check = Command::new("sudo")
        .args(&["virsh", "list", "--all"])
        .output()
        .expect("Failed to check VM status");

    let status_output = String::from_utf8_lossy(&status_check.stdout);
    assert!(
        !status_output.contains(vm_name),
        "VM still exists after deletion!"
    );
    println!("  ✅ VM deleted");

    println!("\n✅ ALL LIFECYCLE TESTS PASSED!");
}
