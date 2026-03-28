#!/usr/bin/env rust-script
//! Example: Build Pop!_OS COSMIC + RustDesk template using benchScale ImageBuilder
//!
//! This demonstrates the proper way to build VM templates with:
//! - Monitored installation
//! - User verification points
//! - Intermediate state saving
//! - Full error handling
//!
//! Run with: cargo run --example build_cosmic_image --features libvirt

use benchscale::{BuildStep, CloudInit, ImageBuilder};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  Building Pop!_OS COSMIC + RustDesk Template (benchScale)               ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();

    // Configuration
    let base_image =
        PathBuf::from("../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img");

    if !base_image.exists() {
        eprintln!("❌ Base image not found: {}", base_image.display());
        eprintln!("   Run: cd ../agentReagents/scripts && ./download-cloud-images.sh");
        std::process::exit(1);
    }

    // Setup cloud-init with SSH key
    let ssh_key = std::env::var("SSH_PUBLIC_KEY").ok().or_else(|| {
        std::fs::read_to_string(format!(
            "{}/.ssh/id_rsa.pub",
            std::env::var("HOME").unwrap()
        ))
        .ok()
    });

    let cloud_init = CloudInit::builder()
        .hostname("cosmic-builder")
        .add_user("cosmic", ssh_key.as_deref())
        .password("cosmic", "cosmic")
        .build();

    // Build template with monitored steps
    let builder = ImageBuilder::new("popos-24-cosmic-rustdesk")?
        .from_cloud_image(base_image)
        .with_memory(4096)
        .with_vcpus(2)
        .with_disk_size(35)
        .with_cloud_init(cloud_init)

        // Step 1: Wait for cloud-init to finish (handles apt locks!)
        .add_step(BuildStep::WaitForCloudInit)

        // Step 2: Install prerequisites
        .add_step(BuildStep::InstallPackages(vec![
            "curl".to_string(),
            "wget".to_string(),
            "gnupg2".to_string(),
            "software-properties-common".to_string(),
        ]))

        // Step 3: Add COSMIC repository (when available)
        // Note: COSMIC is still in development, repo may not exist yet
        .add_step(BuildStep::RunCommands(vec![
            // Check if COSMIC repo exists first
            "curl -fsSL https://apt.system76.com/signing-key.asc 2>/dev/null || echo 'COSMIC repo not available yet'".to_string(),
        ]))

        // For now, install a desktop environment we know works
        .add_step(BuildStep::InstallPackages(vec![
            "ubuntu-desktop-minimal".to_string(),
            "pipewire".to_string(),
            "wireplumber".to_string(),
        ]))

        // Step 4: Install RustDesk
        .add_step(BuildStep::RunCommands(vec![
            "cd /tmp && wget -q https://github.com/rustdesk/rustdesk/releases/download/1.2.3/rustdesk-1.2.3-x86_64.deb".to_string(),
            "sudo DEBIAN_FRONTEND=noninteractive apt install -y -f ./rustdesk-1.2.3-x86_64.deb || true".to_string(),
            "sudo DEBIAN_FRONTEND=noninteractive apt install -y -f".to_string(),
            "rm -f /tmp/rustdesk-1.2.3-x86_64.deb".to_string(),
        ]))

        // Step 5: Configure auto-login and RustDesk auto-start
        .add_step(BuildStep::RunCommands(vec![
            "sudo systemctl set-default graphical.target".to_string(),
            "mkdir -p ~/.config/autostart".to_string(),
            "cat > ~/.config/autostart/rustdesk.desktop << 'EOF'\n[Desktop Entry]\nType=Application\nName=RustDesk\nExec=/usr/bin/rustdesk\nX-GNOME-Autostart-enabled=true\nEOF".to_string(),
        ]))

        // Step 6: Save intermediate state (before reboot)
        .add_step(BuildStep::SaveIntermediate {
            name: "before-gui-reboot".to_string(),
            path: PathBuf::from("/var/lib/libvirt/images/cosmic-intermediate.qcow2"),
        })

        // Step 7: Reboot to start GUI
        .add_step(BuildStep::Reboot)

        // Step 8: USER VERIFICATION - Check GUI via VNC
        .add_step(BuildStep::UserVerification {
            message: r#"
Please verify the desktop environment is working:
  1. Connect via VNC (shown above)
  2. Verify desktop loads
  3. Verify RustDesk auto-starts
  4. Note the RustDesk ID (if shown)
  5. Press ENTER when ready to save template
"#.to_string(),
            vnc_port: None, // Auto-detect
        })

        // Step 9: Cleanup
        .add_step(BuildStep::RunCommands(vec![
            "sudo apt autoremove -y".to_string(),
            "sudo apt clean".to_string(),
        ]));

    println!("Starting build process...");
    println!();

    // Build the template!
    let result = builder.build().await?;

    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  ✅ Template Build Complete!                                             ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("📍 Template: {}", result.template_path.display());
    println!("📊 Size: {} MB", result.final_size_bytes / 1024 / 1024);
    println!();
    println!("🚀 Usage:");
    println!("   let backend = LibvirtBackend::new()?;");
    println!("   let vm = backend.create_from_template(");
    println!("       \"my-vm\",");
    println!(
        "       &PathBuf::from(\"{}\"),",
        result.template_path.display()
    );
    println!("       Some(&cloud_init),");
    println!("       4096, 2, false");
    println!("   ).await?;");
    println!();

    Ok(())
}
