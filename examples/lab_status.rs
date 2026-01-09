// SPDX-License-Identifier: MIT OR Apache-2.0
//! Laboratory Status and Hygiene Management CLI
//!
//! This tool provides comprehensive lab hygiene management:
//! - View status of all VM experiments
//! - Clean up old/orphaned experiments
//! - Maintain laboratory hygiene for reliable research

use anyhow::Result;
use benchscale::backend::lab::{LabHygiene, VmLabStatus};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Parser)]
#[command(name = "lab-status")]
#[command(about = "Laboratory hygiene management for benchScale")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Image directory (default: /var/lib/libvirt/images)
    #[arg(short, long, default_value = "/var/lib/libvirt/images")]
    image_dir: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Show laboratory status
    Status {
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Clean laboratory (remove old/orphaned experiments)
    Clean {
        /// Preserve active healthy VMs
        #[arg(long)]
        preserve_active: bool,

        /// Preserve VMs created in last N hours
        #[arg(long)]
        preserve_recent_hours: Option<f64>,

        /// Dry run (show what would be cleaned)
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let cli = Cli::parse();
    let lab = LabHygiene::new(cli.image_dir)?;

    match &cli.command {
        Commands::Status { format } => {
            let status = lab.status().await?;

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                print_status_text(&status);
            }
        }
        Commands::Clean {
            preserve_active,
            preserve_recent_hours,
            dry_run,
        } => {
            let report = lab
                .clean_lab(*preserve_active, *preserve_recent_hours, *dry_run)
                .await?;
            print_cleanup_report(&report, *dry_run);
        }
    }

    Ok(())
}

fn print_status_text(status: &benchscale::backend::lab::LabStatus) {
    println!("\n🔬 Laboratory Status Report");
    println!("═══════════════════════════════════════════════════");
    println!();
    println!("📊 Summary:");
    println!("   Total Experiments: {}", status.total_vms);
    println!("   Active VMs:        {} ✅", status.active_vms);
    println!("   Orphaned VMs:      {} ⚠️", status.orphaned_vms);
    println!("   Zombie VMs:        {} ❌", status.zombie_vms);
    println!("   Inactive VMs:      {}", status.total_vms - status.active_vms - status.orphaned_vms - status.zombie_vms);
    println!();
    println!("💾 Resources:");
    println!("   Total Memory:      {} MB ({:.1} GB)", status.total_memory_mb, status.total_memory_mb as f64 / 1024.0);
    println!("   Total Disk:        {} MB ({:.1} GB)", status.total_disk_mb, status.total_disk_mb as f64 / 1024.0);
    println!("   Total CPU:         {:.1}%", status.total_cpu_percent);
    println!();

    if !status.experiments.is_empty() {
        println!("🧪 Active Experiments:");
        println!("═══════════════════════════════════════════════════");
        for exp in &status.experiments {
            print_experiment(exp);
        }
    }
}

fn print_experiment(exp: &benchscale::backend::lab::LabExperiment) {
    print!("\n   📝 {}", exp.name);

    match &exp.status {
        VmLabStatus::Active {
            id,
            cpu_percent,
            memory_mb,
        } => {
            println!(" ✅ ACTIVE");
            println!("      ID: {}", id);
            println!("      CPU: {:.1}%", cpu_percent);
            println!("      Memory: {} MB", memory_mb);
        }
        VmLabStatus::Inactive => {
            println!(" 💤 INACTIVE");
        }
        VmLabStatus::Orphaned {
            pid,
            cpu_percent,
            memory_mb,
            runtime_hours,
        } => {
            println!(" ⚠️  ORPHANED");
            println!("      PID: {}", pid);
            println!("      Runtime: {:.1}h", runtime_hours);
            println!("      CPU: {:.1}%", cpu_percent);
            println!("      Memory: {} MB", memory_mb);
        }
        VmLabStatus::Zombie => {
            println!(" ❌ ZOMBIE");
        }
    }

    if let Some(vnc) = exp.vnc_port {
        println!("      VNC: :{}", vnc);
    }

    println!("      Disk: {} MB ({:.1} GB)", exp.disk_size_mb, exp.disk_size_mb as f64 / 1024.0);
    if !exp.disk_images.is_empty() {
        println!("      Images:");
        for img in &exp.disk_images {
            println!("         - {}", img.display());
        }
    }
}

fn print_cleanup_report(report: &benchscale::backend::lab::CleanupReport, dry_run: bool) {
    println!("\n🧹 Laboratory Cleanup Report");
    println!("═══════════════════════════════════════════════════");

    if dry_run {
        println!("\n🔍 DRY RUN - No changes made\n");
    }

    println!("VMs to clean: {}", report.vms_to_clean.len());
    if dry_run {
        println!("VMs cleaned:  N/A (dry run)");
    } else {
        println!("VMs cleaned:  {}", report.vms_cleaned);
    }
    println!("Disk to free: {} MB ({:.1} GB)", report.disk_to_free_mb, report.disk_to_free_mb as f64 / 1024.0);
    println!("Memory to free: {} MB ({:.1} GB)", report.memory_to_free_mb, report.memory_to_free_mb as f64 / 1024.0);

    if !report.vms_to_clean.is_empty() {
        println!("\n{} to be cleaned:", if dry_run { "Experiments" } else { "Cleaned experiments" });
        for vm in &report.vms_to_clean {
            println!("   - {}", vm);
        }
    }

    if !report.errors.is_empty() {
        println!("\n❌ Errors:");
        for err in &report.errors {
            println!("   - {}", err);
        }
    }

    println!("\n{}", if dry_run { "✅ Dry run complete" } else { "✅ Cleanup complete" });
}

