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
use virt::connect::Connect;
use virt::domain::Domain;
use virt::stream::Stream;
use virt::sys::{
    VIR_DOMAIN_BLOCKED, VIR_DOMAIN_CRASHED, VIR_DOMAIN_NOSTATE, VIR_DOMAIN_PAUSED,
    VIR_DOMAIN_PMSUSPENDED, VIR_DOMAIN_RUNNING, VIR_DOMAIN_SHUTDOWN, VIR_DOMAIN_SHUTOFF,
    VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE, VIR_IP_ADDR_TYPE_IPV4, VIR_STREAM_NONBLOCK,
};

fn domain_state_str(state: virt::sys::virDomainState) -> &'static str {
    if state == VIR_DOMAIN_NOSTATE {
        "no state"
    } else if state == VIR_DOMAIN_RUNNING {
        "running"
    } else if state == VIR_DOMAIN_BLOCKED {
        "idle"
    } else if state == VIR_DOMAIN_PAUSED {
        "paused"
    } else if state == VIR_DOMAIN_SHUTDOWN {
        "in shutdown"
    } else if state == VIR_DOMAIN_SHUTOFF {
        "shut off"
    } else if state == VIR_DOMAIN_CRASHED {
        "crashed"
    } else if state == VIR_DOMAIN_PMSUSPENDED {
        "pmsuspended"
    } else {
        "unknown"
    }
}

fn format_dominfo_like(vm_name: &str, domain: &Domain) -> Result<String, virt::error::Error> {
    let info = domain.get_info()?;
    Ok(format!(
        "Name:           {}\nState:          {}\nCPU(s):         {}\nCPU time:       {}\nMax memory:     {} KiB\nUsed memory:    {} KiB\n",
        vm_name,
        domain_state_str(info.state),
        info.nr_virt_cpu,
        info.cpu_time,
        info.max_mem,
        info.memory,
    ))
}

/// Captures serial console output from a VM
pub fn capture_serial_console(vm_name: &str) -> Result<String> {
    info!("📼 Capturing serial console output for VM '{}'", vm_name);

    if let Ok(conn) = Connect::open(Some("qemu:///system")) {
        if let Ok(domain) = Domain::lookup_by_name(&conn, vm_name) {
            if let Ok(mut stream) = Stream::new(&conn, VIR_STREAM_NONBLOCK) {
                if domain.open_console(None, &stream, 0).is_ok() {
                    let mut buf = vec![0u8; 8192];
                    let mut out = Vec::new();
                    for _ in 0..512 {
                        match stream.recv(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                out.extend_from_slice(&buf[..n]);
                                if out.len() >= 256 * 1024 {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    if !out.is_empty() {
                        let console_log = String::from_utf8_lossy(&out).to_string();
                        info!("   ✅ Captured {} bytes of console output", console_log.len());
                        return Ok(console_log);
                    }
                }
            }
        }
    }

    // TODO: replace virsh fallback once libvirt console capture is reliable for this path
    let output = Command::new("virsh")
        .args(["console", vm_name, "--force"])
        .output()
        .context("Failed to execute virsh console")?;

    if output.status.success() {
        let console_log = String::from_utf8_lossy(&output.stdout).to_string();
        info!("   ✅ Captured {} bytes of console output", console_log.len());
        return Ok(console_log);
    }

    warn!("   ⚠️  Could not capture console output directly");
    Ok(String::from("Console output not available"))
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
    let conn = Connect::open(Some("qemu:///system")).ok()?;
    let domain = Domain::lookup_by_name(&conn, vm_name).ok()?;
    let interfaces = domain
        .interface_addresses(VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE, 0)
        .ok()?;
    for iface in interfaces {
        for addr in &iface.addrs {
            if addr.typed == VIR_IP_ADDR_TYPE_IPV4 as i64 {
                return Some(addr.addr.clone());
            }
        }
    }
    None
}

/// Gets boot parameters and kernel command line
pub fn get_boot_parameters(vm_name: &str) -> Result<String> {
    info!("⚙️  Getting boot parameters for VM '{}'", vm_name);

    let conn = Connect::open(Some("qemu:///system")).map_err(|e| {
        anyhow::Error::new(e).context("Failed to get VM XML")
    })?;
    let domain = match Domain::lookup_by_name(&conn, vm_name) {
        Ok(d) => d,
        Err(_) => return Ok(String::from("No kernel command line found")),
    };

    let xml = match domain.get_xml_desc(0) {
        Ok(x) => x,
        Err(_) => return Ok(String::from("No kernel command line found")),
    };

    if let Some(start) = xml.find("<cmdline>") {
        if let Some(end) = xml[start..].find("</cmdline>") {
            let cmdline = &xml[start + 9..start + end];
            info!("   ✅ Kernel cmdline: {}", cmdline);
            return Ok(cmdline.to_string());
        }
    }

    Ok(String::from("No kernel command line found"))
}

/// Analyzes VM state using virsh dominfo
pub fn analyze_vm_state(vm_name: &str) -> Result<String> {
    info!("🔬 Analyzing VM state for '{}'", vm_name);

    let conn = Connect::open(Some("qemu:///system")).map_err(|e| {
        anyhow::Error::new(e).context("Failed to get VM info")
    })?;
    let domain = match Domain::lookup_by_name(&conn, vm_name) {
        Ok(d) => d,
        Err(_) => return Ok(String::from("Could not get VM state")),
    };

    match format_dominfo_like(vm_name, &domain) {
        Ok(info_text) => {
            debug!("VM info:\n{}", info_text);
            Ok(info_text)
        }
        Err(_) => Ok(String::from("Could not get VM state")),
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
