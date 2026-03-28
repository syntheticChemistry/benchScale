#!/usr/bin/env rust-script
//! Example: Build from existing VM using improved ImageBuilder
//! Demonstrates lessons learned from the working pipeline

use benchscale::{BuildStep, ImageBuilder};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  Building from Existing VM (Lessons Learned from Pipeline)              ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();

    let vm_name = "rustdesk-automated-20251229-231311";

    println!("Using existing VM: {}", vm_name);
    println!();
    println!("Improvements from pipeline lessons:");
    println!("  ✅ Auto-detect SSH user (ubuntu/desktop/builder)");
    println!("  ✅ Get actual VM IP from virsh");
    println!("  ✅ Retry SSH connections");
    println!("  ✅ Better error messages");
    println!();

    // Build from existing VM (new feature!)
    let builder = ImageBuilder::from_existing_vm(vm_name)?
        .add_step(BuildStep::WaitForCloudInit)
        .add_step(BuildStep::RunCommands(vec![
            "echo 'Testing improved ImageBuilder'".to_string(),
        ]));

    println!("Starting build from existing VM...");
    println!();

    let result = builder.build_from_existing(vm_name).await?;

    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  ✅ Build Complete with Improvements!                                    ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Template: {}", result.template_path.display());
    println!("Size: {} MB", result.final_size_bytes / 1024 / 1024);
    println!();

    Ok(())
}
