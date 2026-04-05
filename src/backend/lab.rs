// SPDX-License-Identifier: AGPL-3.0-or-later
//! Laboratory Hygiene Management for benchScale
//!
//! This module provides comprehensive "lab hygiene" capabilities:
//! - Audit all running QEMU VMs (libvirt-managed + orphaned)
//! - Track resource consumption (CPU, RAM, disk)
//! - Identify old/zombie experiments
//! - Provide clean state for new experiments
//! - Generate lab status reports
//!
//! Like a well-run microbiology lab, we maintain:
//! - Clean starting conditions
//! - Proper disposal of old samples
//! - Accurate inventory of active experiments
//! - Resource accounting

use anyhow::{Context, Result};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;
use tracing::{debug, error, info, warn};

/// VM status in the lab
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmLabStatus {
    /// VM is managed by libvirt and running
    Active {
        id: u32,
        cpu_percent: f64,
        memory_mb: u64,
    },
    /// VM is shut off but defined in libvirt
    Inactive,
    /// VM is running but not managed by libvirt (orphaned)
    Orphaned {
        pid: u32,
        cpu_percent: f64,
        memory_mb: u64,
        runtime_hours: f64,
    },
    /// VM is in an unknown/zombie state
    Zombie,
}

/// A VM experiment in the laboratory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabExperiment {
    pub name: String,
    pub status: VmLabStatus,
    pub disk_images: Vec<PathBuf>,
    pub disk_size_mb: u64,
    pub created: Option<SystemTime>,
    pub vnc_port: Option<u16>,
}

/// Laboratory status report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabStatus {
    pub experiments: Vec<LabExperiment>,
    pub total_vms: usize,
    pub active_vms: usize,
    pub orphaned_vms: usize,
    pub zombie_vms: usize,
    pub total_memory_mb: u64,
    pub total_disk_mb: u64,
    pub total_cpu_percent: f64,
}

/// Laboratory hygiene manager
pub struct LabHygiene {
    libvirt_backend: crate::backend::libvirt::LibvirtBackend,
    image_dir: PathBuf,
}

impl LabHygiene {
    /// Create a new lab hygiene manager
    pub fn new(image_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            libvirt_backend: crate::backend::libvirt::LibvirtBackend::new()?,
            image_dir,
        })
    }

    /// Generate comprehensive laboratory status report
    pub async fn status(&self) -> Result<LabStatus> {
        info!("🔬 Generating laboratory status report...");

        // 1. Get all libvirt-managed VMs
        let libvirt_domains = self.libvirt_backend.list_all_domains().await?;
        debug!("Found {} libvirt-managed domains", libvirt_domains.len());

        // 2. Get all QEMU processes
        let all_qemu = self.list_all_qemu_processes().await?;
        debug!("Found {} total QEMU processes", all_qemu.len());

        // 3. Build experiments list
        let mut experiments = Vec::new();
        let mut libvirt_vms: HashMap<String, _> = libvirt_domains
            .iter()
            .map(|d| (d.name.clone(), d))
            .collect();

        // Process libvirt-managed VMs
        for (vm_name, qemu) in all_qemu {
            if let Some(domain) = libvirt_vms.remove(&vm_name) {
                // Libvirt-managed VM
                let disk_images = self.find_associated_disk_images(&vm_name).await?;
                let disk_size_mb = self.calculate_disk_size(&disk_images).await?;

                experiments.push(LabExperiment {
                    name: vm_name.clone(),
                    status: if domain.is_active {
                        VmLabStatus::Active {
                            id: domain.id.parse().unwrap_or(0),
                            cpu_percent: qemu.cpu_percent,
                            memory_mb: qemu.memory_mb,
                        }
                    } else {
                        VmLabStatus::Inactive
                    },
                    disk_images,
                    disk_size_mb,
                    created: None, // NOTE: Creation time could be parsed from domain XML if needed.
                    vnc_port: qemu.vnc_port,
                });
            } else {
                // Orphaned VM
                let disk_images = self.find_associated_disk_images(&vm_name).await?;
                let disk_size_mb = self.calculate_disk_size(&disk_images).await?;

                experiments.push(LabExperiment {
                    name: vm_name.clone(),
                    status: VmLabStatus::Orphaned {
                        pid: qemu.pid,
                        cpu_percent: qemu.cpu_percent,
                        memory_mb: qemu.memory_mb,
                        runtime_hours: qemu.runtime_hours,
                    },
                    disk_images,
                    disk_size_mb,
                    created: None,
                    vnc_port: qemu.vnc_port,
                });
            }
        }

        // Add remaining libvirt VMs that aren't running
        for (vm_name, domain) in libvirt_vms {
            let disk_images = self.find_associated_disk_images(&vm_name).await?;
            let disk_size_mb = self.calculate_disk_size(&disk_images).await?;

            experiments.push(LabExperiment {
                name: vm_name.clone(),
                status: VmLabStatus::Inactive,
                disk_images,
                disk_size_mb,
                created: None,
                vnc_port: None,
            });
        }

        // Calculate totals
        let total_vms = experiments.len();
        let active_vms = experiments
            .iter()
            .filter(|e| matches!(e.status, VmLabStatus::Active { .. }))
            .count();
        let orphaned_vms = experiments
            .iter()
            .filter(|e| matches!(e.status, VmLabStatus::Orphaned { .. }))
            .count();
        let zombie_vms = experiments
            .iter()
            .filter(|e| matches!(e.status, VmLabStatus::Zombie))
            .count();

        let total_memory_mb = experiments
            .iter()
            .map(|e| match &e.status {
                VmLabStatus::Active { memory_mb, .. } => *memory_mb,
                VmLabStatus::Orphaned { memory_mb, .. } => *memory_mb,
                _ => 0,
            })
            .sum();

        let total_disk_mb = experiments.iter().map(|e| e.disk_size_mb).sum();

        let total_cpu_percent = experiments
            .iter()
            .map(|e| match &e.status {
                VmLabStatus::Active { cpu_percent, .. } => *cpu_percent,
                VmLabStatus::Orphaned { cpu_percent, .. } => *cpu_percent,
                _ => 0.0,
            })
            .sum();

        Ok(LabStatus {
            experiments,
            total_vms,
            active_vms,
            orphaned_vms,
            zombie_vms,
            total_memory_mb,
            total_disk_mb,
            total_cpu_percent,
        })
    }

    /// Clean the laboratory to a pristine state
    ///
    /// Options:
    /// - `preserve_active`: Keep running VMs that are active and healthy
    /// - `preserve_recent`: Keep VMs created in the last N hours
    /// - `dry_run`: Only report what would be cleaned
    pub async fn clean_lab(
        &self,
        preserve_active: bool,
        preserve_recent_hours: Option<f64>,
        dry_run: bool,
    ) -> Result<CleanupReport> {
        info!("🧹 Starting laboratory cleanup...");
        info!("   Options: preserve_active={}, preserve_recent={:?}, dry_run={}", 
            preserve_active, preserve_recent_hours, dry_run);

        let status = self.status().await?;
        let mut report = CleanupReport::default();

        for exp in status.experiments {
            let should_clean = match &exp.status {
                VmLabStatus::Active { .. } if preserve_active => {
                    debug!("Preserving active VM: {}", exp.name);
                    false
                }
                VmLabStatus::Orphaned { runtime_hours, .. } => {
                    if let Some(preserve_hours) = preserve_recent_hours {
                        if *runtime_hours < preserve_hours {
                            debug!("Preserving recent orphaned VM: {} ({}h old)", exp.name, runtime_hours);
                            false
                        } else {
                            warn!("Will clean old orphaned VM: {} ({}h old)", exp.name, runtime_hours);
                            true
                        }
                    } else {
                        warn!("Will clean orphaned VM: {}", exp.name);
                        true
                    }
                }
                VmLabStatus::Zombie => {
                    error!("Will clean zombie VM: {}", exp.name);
                    true
                }
                VmLabStatus::Inactive => {
                    info!("Will clean inactive VM: {}", exp.name);
                    true
                }
                _ => {
                    debug!("Will clean VM: {}", exp.name);
                    true
                }
            };

            if should_clean {
                report.vms_to_clean.push(exp.name.clone());
                report.disk_to_free_mb += exp.disk_size_mb;
                
                if let VmLabStatus::Active { memory_mb, .. } | VmLabStatus::Orphaned { memory_mb, .. } = exp.status {
                    report.memory_to_free_mb += memory_mb;
                }

                if !dry_run {
                    match self.clean_experiment(&exp.name).await {
                        Ok(_) => {
                            report.vms_cleaned += 1;
                            info!("   ✅ Cleaned: {}", exp.name);
                        }
                        Err(e) => {
                            report.errors.push(format!("{}: {}", exp.name, e));
                            error!("   ❌ Failed to clean {}: {}", exp.name, e);
                        }
                    }
                }
            }
        }

        if dry_run {
            info!("🔍 Dry run complete. {} VMs would be cleaned", report.vms_to_clean.len());
        } else {
            info!("✅ Lab cleanup complete. {} VMs cleaned", report.vms_cleaned);
        }

        Ok(report)
    }

    /// Clean a single experiment (VM + associated resources)
    async fn clean_experiment(&self, name: &str) -> Result<()> {
        debug!("Cleaning experiment: {}", name);

        // 1. Stop and undefine libvirt domain
        if let Err(e) = self.libvirt_backend.destroy_node(name).await {
            debug!("Failed to destroy domain {}: {}", name, e);
        }
        if let Err(e) = self.libvirt_backend.undefine_node(name).await {
            debug!("Failed to undefine domain {}: {}", name, e);
        }

        // 2. Kill orphaned QEMU processes
        let orphaned = self.find_orphaned_qemu_processes(name).await?;
        for pid in orphaned {
            if let Err(e) = self.kill_process(pid).await {
                warn!("Failed to kill QEMU process {}: {}", pid, e);
            }
        }

        // 3. Remove disk images
        let disk_images = self.find_associated_disk_images(name).await?;
        for image in disk_images {
            if image.exists() {
                if let Err(e) = tokio::fs::remove_file(&image).await {
                    warn!("Failed to remove disk image {}: {}", image.display(), e);
                }
            }
        }

        Ok(())
    }

    /// List all QEMU processes (libvirt-managed + orphaned)
    async fn list_all_qemu_processes(&self) -> Result<HashMap<String, QemuProcess>> {
        let output = Command::new("ps")
            .arg("auxww")
            .output()
            .context("Failed to execute ps")?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut processes = HashMap::new();

        for line in output_str.lines() {
            if line.contains("qemu-system-x86_64") && line.contains("-name guest=") {
                if let Some(proc) = self.parse_qemu_process(line) {
                    processes.insert(proc.vm_name.clone(), proc);
                }
            }
        }

        Ok(processes)
    }

    /// Parse QEMU process info from ps output
    fn parse_qemu_process(&self, line: &str) -> Option<QemuProcess> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 11 {
            return None;
        }

        let pid = parts[1].parse::<u32>().ok()?;
        let cpu_percent = parts[2].parse::<f64>().unwrap_or(0.0);
        let memory_percent = parts[3].parse::<f64>().unwrap_or(0.0);

        // Extract VM name from -name guest=<name>
        let vm_name = line
            .split("-name guest=")
            .nth(1)?
            .split(',')
            .next()?
            .to_string();

        // Extract VNC port from -vnc 0.0.0.0:<port>
        let vnc_port = line
            .split("-vnc ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.split(':').nth(1))
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.parse::<u16>().ok())
            .map(|p| p + 5900); // VNC display to port

        // Get total system memory to calculate absolute memory usage
        let total_memory_mb = get_system_memory_mb();
        let memory_mb = ((memory_percent / 100.0) * total_memory_mb as f64) as u64;

        // NOTE: Runtime could be derived from `/proc/{pid}/stat` start time when needed.
        let runtime_hours = 0.0;

        Some(QemuProcess {
            pid,
            vm_name,
            cpu_percent,
            memory_mb,
            runtime_hours,
            vnc_port,
        })
    }

    /// Find QEMU processes that are not managed by libvirt
    async fn find_orphaned_qemu_processes(&self, prefix: &str) -> Result<Vec<u32>> {
        let all_qemu = self.list_all_qemu_processes().await?;
        let libvirt_domains = self.libvirt_backend.list_all_domains().await?;
        let libvirt_names: std::collections::HashSet<_> =
            libvirt_domains.iter().map(|d| d.name.as_str()).collect();

        Ok(all_qemu
            .iter()
            .filter(|(name, _)| name.starts_with(prefix) && !libvirt_names.contains(name.as_str()))
            .map(|(_, proc)| proc.pid)
            .collect())
    }

    /// Find disk images associated with a VM
    async fn find_associated_disk_images(&self, vm_name: &str) -> Result<Vec<PathBuf>> {
        let mut images = Vec::new();
        if self.image_dir.is_dir() {
            let mut entries = tokio::fs::read_dir(&self.image_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                        if file_name.starts_with(vm_name)
                            && (file_name.ends_with(".qcow2") || file_name.ends_with(".iso"))
                        {
                            images.push(path);
                        }
                    }
                }
            }
        }
        Ok(images)
    }

    /// Calculate total disk size
    async fn calculate_disk_size(&self, paths: &[PathBuf]) -> Result<u64> {
        let mut total = 0u64;
        for path in paths {
            if let Ok(metadata) = tokio::fs::metadata(path).await {
                total += metadata.len() / 1_000_000; // Convert to MB
            }
        }
        Ok(total)
    }

    /// Kill a process by PID (no sudo — uses `nix::sys::signal::kill`).
    ///
    /// Works for same-user processes. For libvirt-managed QEMU processes,
    /// callers should prefer `virsh destroy` (via `destroy_node`) which
    /// handles the privilege boundary through the libvirt daemon.
    async fn kill_process(&self, pid: u32) -> Result<()> {
        let p = Pid::from_raw(
            i32::try_from(pid).map_err(|_| anyhow::anyhow!("PID {} out of range for kill", pid))?,
        );
        kill(p, Signal::SIGKILL).map_err(|e| {
            anyhow::anyhow!("kill({}, SIGKILL) failed: {}", pid, e)
        })?;
        Ok(())
    }
}

/// QEMU process information
#[derive(Debug, Clone)]
struct QemuProcess {
    pid: u32,
    vm_name: String,
    cpu_percent: f64,
    memory_mb: u64,
    runtime_hours: f64,
    vnc_port: Option<u16>,
}

/// Cleanup report
#[derive(Debug, Clone, Default)]
pub struct CleanupReport {
    pub vms_to_clean: Vec<String>,
    pub vms_cleaned: usize,
    pub disk_to_free_mb: u64,
    pub memory_to_free_mb: u64,
    pub errors: Vec<String>,
}

/// Get total system memory in MB
fn get_system_memory_mb() -> u64 {
    // Read from /proc/meminfo
    if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
        for line in contents.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(kb) = line.split_whitespace().nth(1) {
                    if let Ok(kb_val) = kb.parse::<u64>() {
                        return kb_val / 1024; // Convert KB to MB
                    }
                }
            }
        }
    }
    8192 // Default fallback
}

