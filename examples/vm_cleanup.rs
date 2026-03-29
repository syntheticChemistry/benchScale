// SPDX-License-Identifier: AGPL-3.0-only
// Copyright © 2024-2025 DataScienceBioLab
//
//! VM Cleanup CLI Tool
//!
//! Usage:
//!   cargo run --example vm_cleanup -- --vm ubuntu24-desktop-rustdesk
//!   cargo run --example vm_cleanup -- --prefix ubuntu24
//!   cargo run --example vm_cleanup -- --orphaned
//!   cargo run --example vm_cleanup -- --emergency

use anyhow::Result;
use benchscale::backend::cleanup::VmCleanup;
use clap::{Parser, Subcommand};
use tracing::Level;

#[derive(Parser)]
#[command(name = "vm-cleanup")]
#[command(about = "Clean up VMs and their resources")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Clean up a specific VM
    Vm {
        /// Name of the VM to clean up
        #[arg(short, long)]
        name: String,
    },
    /// Clean up all VMs matching a prefix
    Prefix {
        /// Prefix to match (e.g., "ubuntu24")
        #[arg(short, long)]
        prefix: String,
    },
    /// Clean up orphaned disk images
    Orphaned,
    /// Emergency cleanup (stops ALL VMs)
    Emergency,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let cli = Cli::parse();
    let cleanup = VmCleanup::default();

    match cli.command {
        Commands::Vm { name } => {
            cleanup.cleanup_vm(&name)?;
        }
        Commands::Prefix { prefix } => {
            let cleaned = cleanup.cleanup_matching(&prefix)?;
            println!("✅ Cleaned up {} VMs", cleaned.len());
            for vm in cleaned {
                println!("   - {}", vm);
            }
        }
        Commands::Orphaned => {
            let cleaned = cleanup.cleanup_orphaned_disks()?;
            println!("✅ Cleaned up {} orphaned disks", cleaned.len());
            for disk in cleaned {
                println!("   - {:?}", disk);
            }
        }
        Commands::Emergency => {
            cleanup.emergency_cleanup()?;
        }
    }

    Ok(())
}

