// Production-Ready Example: Cloud-Init VM with Guaranteed SSH Access
//
// This example demonstrates the recommended way to create VMs with benchScale,
// using the new create_desktop_vm_ready() API that guarantees SSH accessibility.
//
// Run with:
//   cargo run --example production_vm_ready --features libvirt

use benchscale::{CloudInit, LibvirtBackend};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  benchScale Production Example: VM with Guaranteed SSH Access       ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();

    // Configuration
    let vm_name = format!(
        "production-ready-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    let base_image = PathBuf::from(
        std::env::var("BENCHSCALE_BASE_IMAGE")
            .unwrap_or_else(|_| "ubuntu-24.04-server-cloudimg-amd64.img".to_string()),
    );
    let ssh_public_key = std::env::var("SSH_PUBLIC_KEY").unwrap_or_else(|_| {
        // Fallback to reading from default SSH key
        std::fs::read_to_string(std::env::var("HOME").unwrap() + "/.ssh/id_rsa.pub")
            .unwrap_or_else(|_| "".to_string())
    });

    println!("🔧 Configuration");
    println!("   VM Name: {}", vm_name);
    println!("   Image: {}", base_image.display());
    println!("   Memory: 2048 MB (2 GB)");
    println!("   vCPUs: 2");
    println!("   Disk: 15 GB");
    println!("   SSH Auth: Public key");
    println!();

    // Initialize backend
    println!("📦 Initializing LibvirtBackend...");
    let backend = LibvirtBackend::new()?;
    println!("✅ Backend ready");
    println!();

    // Build cloud-init configuration (PRODUCTION PATTERN)
    println!("☁️  Building Cloud-Init configuration...");
    let cloud_init = if ssh_public_key.is_empty() {
        println!("⚠️  No SSH key found - using password authentication");
        println!("   For production, use SSH keys via SSH_PUBLIC_KEY env var");

        CloudInit::builder()
            .add_user("ubuntu", "")
            .cmd("echo 'ubuntu:ubuntu' | chpasswd")
            .cmd("sed -i 's/PasswordAuthentication no/PasswordAuthentication yes/' /etc/ssh/sshd_config")
            .cmd("systemctl restart ssh")
            .packages(vec![
                "curl".to_string(),
                "wget".to_string(),
                "net-tools".to_string(),
            ])
            .build()
    } else {
        println!("✅ Using SSH key authentication (RECOMMENDED)");

        CloudInit::builder()
            .add_user("ubuntu", &ssh_public_key)
            .packages(vec![
                "curl".to_string(),
                "wget".to_string(),
                "net-tools".to_string(),
            ])
            .build()
    };

    println!("✅ Cloud-Init ready");
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // PRODUCTION PATTERN: Use create_desktop_vm_ready()
    // ═══════════════════════════════════════════════════════════════════════

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🚀 Creating VM with GUARANTEED SSH access...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("   Using: backend.create_desktop_vm_ready()");
    println!("   This method will:");
    println!("   1. Create the VM");
    println!("   2. Wait for IP assignment");
    println!("   3. Wait for cloud-init to complete");
    println!("   4. Validate SSH is accessible");
    println!("   5. Return NodeInfo only when VM is fully ready");
    println!();

    let start = Instant::now();

    let username = "ubuntu";
    let password = if ssh_public_key.is_empty() {
        "ubuntu"
    } else {
        ""
    };

    let result = backend
        .create_desktop_vm_ready(
            &vm_name,
            &base_image,
            &cloud_init,
            2048, // 2GB RAM
            2,    // 2 vCPUs
            15,   // 15GB disk
            username,
            password,
            Duration::from_secs(600), // Wait up to 10 minutes
        )
        .await;

    match result {
        Ok(node) => {
            let total_time = start.elapsed();

            println!("✅ VM IS READY!");
            println!();
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("📊 VM Information");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();
            println!("   Name: {}", node.name);
            println!("   ID: {}", node.id);
            println!("   IP Address: {}", node.ip_address);
            println!("   Total Setup Time: {:.2}s", total_time.as_secs_f64());
            println!();
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("✅ SSH IS GUARANTEED TO WORK!");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();
            println!("   Connect now:");
            if ssh_public_key.is_empty() {
                println!("   $ ssh ubuntu@{}", node.ip_address);
                println!("   Password: ubuntu");
            } else {
                println!("   $ ssh ubuntu@{}", node.ip_address);
            }
            println!();
            println!("   Or use programmatically:");
            println!(
                "   let ssh = SshClient::connect(\"{}\", 22, \"ubuntu\", ...).await?;",
                node.ip_address
            );
            println!("   ssh.execute(\"hostname\").await?;  // Works immediately!");
            println!();

            // Demonstrate the value
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("💡 Key Benefits");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();
            println!("   ✅ No retry loops needed");
            println!("   ✅ No race conditions");
            println!("   ✅ No hardcoded sleeps");
            println!("   ✅ No timing bugs");
            println!("   ✅ SSH works immediately");
            println!("   ✅ Cloud-init guaranteed complete");
            println!("   ✅ Clear error messages on failure");
            println!();

            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("🧹 Cleanup");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();
            println!("   To destroy this VM:");
            println!("   $ virsh destroy {}", vm_name);
            println!("   $ virsh undefine {} --remove-all-storage", vm_name);
            println!();
            println!("   VM will remain running for inspection.");
            println!();
        }
        Err(e) => {
            println!("❌ VM creation failed: {}", e);
            println!();
            println!("   The error message above provides:");
            println!("   • What stage failed (VM creation, cloud-init, SSH)");
            println!("   • Why it failed (timeout, authentication, network)");
            println!("   • How long it waited");
            println!("   • Suggestions for debugging");
            println!();
            return Err(e);
        }
    }

    Ok(())
}
