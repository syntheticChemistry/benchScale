#!/usr/bin/env rust-script
//! Example: Build Ubuntu Desktop + RustDesk template using benchScale ImageBuilder
//! 
//! This demonstrates ImageBuilder with a working desktop environment
//! (Using Ubuntu Desktop since COSMIC repo isn't publicly available yet)

use benchscale::{BuildStep, CloudInit, ImageBuilder};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  Building Ubuntu Desktop + RustDesk Template (benchScale)               ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();

    let base_image = PathBuf::from("../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img");
    
    if !base_image.exists() {
        eprintln!("❌ Base image not found: {}", base_image.display());
        eprintln!("   Run: cd ../agentReagents/scripts && ./download-cloud-images.sh");
        std::process::exit(1);
    }

    // Setup cloud-init
    let ssh_key = std::env::var("SSH_PUBLIC_KEY")
        .ok()
        .or_else(|| {
            std::fs::read_to_string(format!("{}/.ssh/id_rsa.pub", std::env::var("HOME").unwrap()))
                .ok()
        });

    let cloud_init = CloudInit::builder()
        .add_user("desktop", ssh_key.as_deref().unwrap_or(""))
        .build();

    // Build template with ImageBuilder
    let builder = ImageBuilder::new("ubuntu-desktop-rustdesk")?
        .from_cloud_image(base_image)
        .with_memory(4096)
        .with_vcpus(2)
        .with_disk_size(35)
        .with_cloud_init(cloud_init)
        
        // Step 1: Wait for cloud-init (critical - handles apt locks!)
        .add_step(BuildStep::WaitForCloudInit)
        
        // Step 2: Install Ubuntu Desktop (minimal)
        .add_step(BuildStep::InstallPackages(vec![
            "ubuntu-desktop-minimal".to_string(),
            "pipewire".to_string(),
            "wireplumber".to_string(),
        ]))
        
        // Step 3: Install RustDesk
        .add_step(BuildStep::RunCommands(vec![
            "cd /tmp && wget -q https://github.com/rustdesk/rustdesk/releases/download/1.2.3/rustdesk-1.2.3-x86_64.deb".to_string(),
            "sudo DEBIAN_FRONTEND=noninteractive apt install -y -f ./rustdesk-1.2.3-x86_64.deb || true".to_string(),
            "sudo DEBIAN_FRONTEND=noninteractive apt install -y -f".to_string(),
            "rm -f /tmp/rustdesk-1.2.3-x86_64.deb".to_string(),
        ]))
        
        // Step 4: Configure auto-start
        .add_step(BuildStep::RunCommands(vec![
            "sudo systemctl set-default graphical.target".to_string(),
            "mkdir -p ~/.config/autostart".to_string(),
            r#"cat > ~/.config/autostart/rustdesk.desktop << 'EOF'
[Desktop Entry]
Type=Application
Name=RustDesk
Exec=/usr/bin/rustdesk
X-GNOME-Autostart-enabled=true
EOF"#.to_string(),
        ]))
        
        // Step 5: Reboot to start GUI
        .add_step(BuildStep::Reboot)
        
        // Step 6: USER VERIFICATION
        .add_step(BuildStep::UserVerification {
            message: r#"
╔══════════════════════════════════════════════════════════════════════════╗
║  Please verify the desktop environment:                                  ║
╚══════════════════════════════════════════════════════════════════════════╝

1. Connect via VNC (shown above)
2. Verify Ubuntu desktop loads
3. Verify RustDesk auto-starts
4. Note the RustDesk ID (if shown)
5. Press ENTER when ready to save template
"#.to_string(),
            vnc_port: None,
        });

    println!("Starting build process...");
    println!("This will take 15-20 minutes for desktop installation.");
    println!();

    // Build!
    let result = builder.build().await?;

    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  ✅ Template Build Complete!                                             ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("📍 Template: {}", result.template_path.display());
    println!("📊 Size: {} MB", result.final_size_bytes / 1024 / 1024);
    println!();
    println!("🚀 Create VM from template:");
    println!("   cargo run --example use_template --features libvirt");
    println!();

    Ok(())
}

