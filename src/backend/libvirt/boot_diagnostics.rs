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

/// Extracts systemd journal from a VM via SSH (no root / guestmount needed).
///
/// Falls back to reading `/var/log/cloud-init-output.log` when journalctl
/// is unavailable. If SSH is unreachable (VM crashed hard), returns a
/// descriptive message instead of failing.
pub fn extract_journal_via_ssh(vm_name: &str) -> Result<String> {
    info!("Extracting journal from VM '{}' via SSH", vm_name);

    let ip = get_vm_ip_from_virsh(vm_name);
    let Some(ip) = ip else {
        return Ok("Journal unavailable: could not determine VM IP".to_string());
    };

    let users = ["ubuntu", "reagent", "builder", "cosmic"];
    for user in users {
        let addr = format!("{}@{}", user, ip);
        let output = Command::new("ssh")
            .args([
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "ConnectTimeout=5",
                "-o", "BatchMode=yes",
                &addr,
                "journalctl --no-pager --boot -0 --priority warning --lines 200 2>/dev/null || cat /var/log/cloud-init-output.log 2>/dev/null || echo 'no journal available'",
            ])
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                if !text.trim().is_empty() {
                    info!("Extracted {} bytes of journal via SSH (user={})", text.len(), user);
                    return Ok(text);
                }
            }
        }
    }

    Ok("Journal unavailable: SSH not reachable on any known user".to_string())
}

/// Helper: resolve a VM's IP from virsh DHCP leases (no sudo needed).
fn get_vm_ip_from_virsh(vm_name: &str) -> Option<String> {
    let output = Command::new("virsh")
        .args(["domifaddr", vm_name])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if line.contains("ipv4") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(ip_mask) = parts.last() {
                return ip_mask.split('/').next().map(String::from);
            }
        }
    }
    None
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
    /// Generate a comprehensive diagnostic report for a failed VM boot.
    ///
    /// Uses SSH-based journal extraction (no root / guestmount needed).
    /// The `_disk_path` parameter is kept for API compatibility but is no
    /// longer used — journal logs are pulled via SSH before the VM is torn down.
    pub async fn generate(vm_name: &str, _disk_path: &Path) -> Result<Self> {
        info!("Generating comprehensive boot diagnostics for '{}'", vm_name);
        
        let serial_console = capture_serial_console(vm_name)
            .unwrap_or_else(|e| format!("Failed to capture console: {}", e));
        
        let journal_logs = extract_journal_via_ssh(vm_name)
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

