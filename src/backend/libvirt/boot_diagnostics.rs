// SPDX-License-Identifier: AGPL-3.0-only
//! Deep boot diagnostics for failed VM boots
//!
//! This module provides comprehensive diagnostics when a VM fails to boot:
//! - Serial console log capture
//! - Disk mounting and journal inspection
//! - systemd boot analysis
//! - Kernel boot parameter analysis

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

/// Captures serial console output from a VM
pub fn capture_serial_console(vm_name: &str) -> Result<String> {
    info!("📼 Capturing serial console output for VM '{}'", vm_name);
    
    // Try to get console output via virsh console dump
    let output = Command::new("virsh")
        .args(["console", vm_name, "--force"])
        .output()
        .context("Failed to execute virsh console")?;

    if output.status.success() {
        let console_log = String::from_utf8_lossy(&output.stdout).to_string();
        info!("   ✅ Captured {} bytes of console output", console_log.len());
        Ok(console_log)
    } else {
        warn!("   ⚠️  Could not capture console output directly");
        Ok(String::from("Console output not available"))
    }
}

/// Extracts systemd journal from a VM disk image
pub fn extract_journal_from_disk(disk_path: &Path) -> Result<String> {
    info!("📖 Extracting systemd journal from disk: {:?}", disk_path);
    
    // Create temporary mount point
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_secs();
    let mount_point = std::env::temp_dir().join(format!("vm-diag-{secs}"));

    std::fs::create_dir_all(&mount_point)
        .context("Failed to create mount point")?;

    info!("   📁 Mount point: {:?}", mount_point);

    let disk_path_str = disk_path
        .to_str()
        .context("disk path is not valid UTF-8")?;
    let mount_point_str = mount_point
        .to_str()
        .context("mount point path is not valid UTF-8")?;

    // Find the partition with the root filesystem
    // Use guestmount (libguestfs) to mount the qcow2 image
    info!("   🔍 Mounting disk image (this may take a moment)...");
    let mount_output = Command::new("guestmount")
        .args([
            "-a", disk_path_str,
            "-m", "/dev/sda1",  // Usually the first partition
            "--ro",              // Read-only
            mount_point_str,
        ])
        .output()
        .context("Failed to mount disk image")?;

    if !mount_output.status.success() {
        // Try alternative partition naming
        let mount_output = Command::new("guestmount")
            .args([
                "-a", disk_path_str,
                "-m", "/dev/vda1",
                "--ro",
                mount_point_str,
            ])
            .output()
            .context("Failed to mount disk image (second attempt)")?;
        
        if !mount_output.status.success() {
            warn!("   ⚠️  Could not mount disk: {}", 
                String::from_utf8_lossy(&mount_output.stderr));
            return Ok(String::from("Could not mount disk image"));
        }
    }
    
    info!("   ✅ Disk mounted successfully");
    
    // Extract journal logs
    let journal_dir = mount_point.join("var/log/journal");
    let mut journal_content = String::new();
    
    if journal_dir.exists() {
        info!("   📊 Reading systemd journal...");
        let journal_dir_str = journal_dir
            .to_str()
            .context("journal directory path is not valid UTF-8")?;
        // Use journalctl to read the journal from the mounted filesystem
        let journal_output = Command::new("journalctl")
            .args([
                "--directory", journal_dir_str,
                "--no-pager",
                "--boot", "-0",  // Last boot
                "--priority", "warning",  // Warning and above
                "--lines", "200",  // Last 200 lines
            ])
            .output();
        
        match journal_output {
            Ok(output) if output.status.success() => {
                journal_content = String::from_utf8_lossy(&output.stdout).to_string();
                info!("   ✅ Extracted {} bytes of journal logs", journal_content.len());
            }
            _ => {
                warn!("   ⚠️  Could not read journal with journalctl");
            }
        }
    } else {
        warn!("   ⚠️  Journal directory not found at {:?}", journal_dir);
    }
    
    // Unmount
    info!("   🧹 Unmounting disk...");
    let unmount_output = Command::new("guestunmount")
        .arg(mount_point_str)
        .output();
    
    match unmount_output {
        Ok(output) if output.status.success() => {
            info!("   ✅ Disk unmounted");
        }
        _ => {
            warn!("   ⚠️  Could not unmount disk cleanly");
        }
    }
    
    // Clean up mount point
    let _ = std::fs::remove_dir(&mount_point);
    
    Ok(journal_content)
}

/// Gets boot parameters and kernel command line
pub fn get_boot_parameters(vm_name: &str) -> Result<String> {
    info!("⚙️  Getting boot parameters for VM '{}'", vm_name);
    
    let output = Command::new("virsh")
        .args(["dumpxml", vm_name])
        .output()
        .context("Failed to get VM XML")?;
    
    if output.status.success() {
        let xml = String::from_utf8_lossy(&output.stdout);
        // Extract kernel command line if present
        if let Some(start) = xml.find("<cmdline>") {
            if let Some(end) = xml[start..].find("</cmdline>") {
                let cmdline = &xml[start + 9..start + end];
                info!("   ✅ Kernel cmdline: {}", cmdline);
                return Ok(cmdline.to_string());
            }
        }
    }
    
    Ok(String::from("No kernel command line found"))
}

/// Analyzes VM state using virsh dominfo
pub fn analyze_vm_state(vm_name: &str) -> Result<String> {
    info!("🔬 Analyzing VM state for '{}'", vm_name);
    
    let output = Command::new("virsh")
        .args(["dominfo", vm_name])
        .output()
        .context("Failed to get VM info")?;
    
    if output.status.success() {
        let info_text = String::from_utf8_lossy(&output.stdout).to_string();
        debug!("VM info:\n{}", info_text);
        Ok(info_text)
    } else {
        Ok(String::from("Could not get VM state"))
    }
}

/// Comprehensive boot failure diagnostics
pub struct BootDiagnosticsReport {
    pub vm_name: String,
    pub serial_console: String,
    pub journal_logs: String,
    pub boot_parameters: String,
    pub vm_state: String,
}

impl BootDiagnosticsReport {
    /// Generate a comprehensive diagnostic report for a failed VM boot
    pub async fn generate(vm_name: &str, disk_path: &Path) -> Result<Self> {
        info!("🔬 Generating comprehensive boot diagnostics for '{}'", vm_name);
        
        let serial_console = capture_serial_console(vm_name)
            .unwrap_or_else(|e| format!("Failed to capture console: {}", e));
        
        let journal_logs = extract_journal_from_disk(disk_path)
            .unwrap_or_else(|e| format!("Failed to extract journal: {}", e));
        
        let boot_parameters = get_boot_parameters(vm_name)
            .unwrap_or_else(|e| format!("Failed to get boot params: {}", e));
        
        let vm_state = analyze_vm_state(vm_name)
            .unwrap_or_else(|e| format!("Failed to analyze state: {}", e));
        
        Ok(Self {
            vm_name: vm_name.to_string(),
            serial_console,
            journal_logs,
            boot_parameters,
            vm_state,
        })
    }
    
    /// Format the report as a readable string
    pub fn format(&self) -> String {
        format!(
            r"
╔══════════════════════════════════════════════════════════════════════╗
║                  BOOT DIAGNOSTICS REPORT                             ║
╚══════════════════════════════════════════════════════════════════════╝

VM: {}

═══════════════════════════════════════════════════════════════════════
VM STATE
═══════════════════════════════════════════════════════════════════════
{}

═══════════════════════════════════════════════════════════════════════
BOOT PARAMETERS
═══════════════════════════════════════════════════════════════════════
{}

═══════════════════════════════════════════════════════════════════════
SERIAL CONSOLE OUTPUT (Last 50 lines)
═══════════════════════════════════════════════════════════════════════
{}

═══════════════════════════════════════════════════════════════════════
SYSTEMD JOURNAL (Priority: Warning+, Last 200 lines)
═══════════════════════════════════════════════════════════════════════
{}

",
            self.vm_name,
            self.vm_state,
            self.boot_parameters,
            self.serial_console
                .lines()
                .rev()
                .take(50)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n"),
            self.journal_logs,
        )
    }
    
    /// Save the report to a file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.format())
            .context("Failed to write diagnostics report")?;
        info!("📝 Diagnostics report saved to: {:?}", path);
        Ok(())
    }
}

