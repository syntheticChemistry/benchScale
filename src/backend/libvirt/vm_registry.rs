use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Registry entry for a VM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmRegistryEntry {
    /// VM name
    pub name: String,
    /// VM creation timestamp (Unix epoch seconds)
    pub created_at: u64,
    /// Static IP address (if assigned)
    pub static_ip: Option<String>,
    /// PID of the process that created the VM
    pub creator_pid: u32,
    /// Build status
    pub status: VmStatus,
    /// Last update timestamp
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VmStatus {
    Creating,
    Building,
    Running,
    Failed,
    Completed,
}

/// VM Registry for tracking active VMs and detecting orphans
pub struct VmRegistry {
    registry_path: PathBuf,
    entries: HashMap<String, VmRegistryEntry>,
}

impl VmRegistry {
    /// Default registry path
    pub fn default_path() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("benchscale")
            .join("vm_registry.json")
    }

    /// Create or load registry from default location
    pub fn new() -> Result<Self> {
        Self::from_path(&Self::default_path())
    }

    /// Create or load registry from specified path
    pub fn from_path(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create registry directory")?;
        }

        let entries = if path.exists() {
            let data = fs::read_to_string(path)
                .context("Failed to read registry file")?;
            serde_json::from_str(&data)
                .context("Failed to parse registry JSON")?
        } else {
            HashMap::new()
        };

        Ok(Self {
            registry_path: path.to_path_buf(),
            entries,
        })
    }

    /// Register a new VM
    pub fn register(&mut self, name: String, static_ip: Option<String>) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = VmRegistryEntry {
            name: name.clone(),
            created_at: now,
            static_ip,
            creator_pid: std::process::id(),
            status: VmStatus::Creating,
            updated_at: now,
        };

        info!("Registering VM '{}' in registry", name);
        self.entries.insert(name, entry);
        self.save()?;

        Ok(())
    }

    /// Update VM status
    pub fn update_status(&mut self, name: &str, status: VmStatus) -> Result<()> {
        if let Some(entry) = self.entries.get_mut(name) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            debug!("Updating VM '{}' status: {:?} -> {:?}", name, entry.status, status);
            entry.status = status;
            entry.updated_at = now;
            self.save()?;
        } else {
            warn!("Attempted to update status for unregistered VM: {}", name);
        }

        Ok(())
    }

    /// Unregister a VM (successful completion or cleanup)
    pub fn unregister(&mut self, name: &str) -> Result<()> {
        if self.entries.remove(name).is_some() {
            info!("Unregistered VM '{}' from registry", name);
            self.save()?;
        } else {
            debug!("VM '{}' not found in registry during unregister", name);
        }

        Ok(())
    }

    /// Get all registered VMs
    pub fn list_all(&self) -> Vec<&VmRegistryEntry> {
        self.entries.values().collect()
    }

    /// Find orphaned VMs (creator process no longer exists)
    pub fn find_orphans(&self) -> Vec<&VmRegistryEntry> {
        self.entries
            .values()
            .filter(|entry| !self.process_exists(entry.creator_pid))
            .collect()
    }

    /// Find stale VMs (in Creating/Building state for too long)
    pub fn find_stale(&self, max_age_secs: u64) -> Vec<&VmRegistryEntry> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.entries
            .values()
            .filter(|entry| {
                matches!(entry.status, VmStatus::Creating | VmStatus::Building)
                    && (now - entry.updated_at) > max_age_secs
            })
            .collect()
    }

    /// Check if a process exists
    fn process_exists(&self, pid: u32) -> bool {
        // On Linux, check if /proc/<pid> exists
        #[cfg(target_os = "linux")]
        {
            Path::new(&format!("/proc/{}", pid)).exists()
        }

        // Fallback for other platforms
        #[cfg(not(target_os = "linux"))]
        {
            use std::process::Command;
            Command::new("ps")
                .args(["-p", &pid.to_string()])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }
    }

    /// Save registry to disk
    fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.entries)
            .context("Failed to serialize registry")?;
        
        fs::write(&self.registry_path, json)
            .context("Failed to write registry file")?;

        debug!("Registry saved to {}", self.registry_path.display());
        Ok(())
    }

    /// Get registry file path
    pub fn path(&self) -> &Path {
        &self.registry_path
    }

    /// Clear all entries (for testing)
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.save()
    }
}

impl Default for VmRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to create default VM registry")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_registry_create_and_load() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");

        // Create new registry
        let mut registry = VmRegistry::from_path(&path).unwrap();
        registry.register("test-vm".to_string(), None).unwrap();

        // Load existing registry
        let loaded = VmRegistry::from_path(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entries.contains_key("test-vm"));
    }

    #[test]
    fn test_orphan_detection() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");
        let mut registry = VmRegistry::from_path(&path).unwrap();

        // Create entry with current PID (not orphaned)
        registry.register("current-vm".to_string(), None).unwrap();

        // Create entry with impossible PID (orphaned)
        let orphan = VmRegistryEntry {
            name: "orphan-vm".to_string(),
            created_at: 1000,
            static_ip: None,
            creator_pid: 999999, // Very unlikely to exist
            status: VmStatus::Building,
            updated_at: 1000,
        };
        registry.entries.insert("orphan-vm".to_string(), orphan);

        let orphans = registry.find_orphans();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].name, "orphan-vm");
    }

    #[test]
    fn test_stale_detection() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("registry.json");
        let mut registry = VmRegistry::from_path(&path).unwrap();

        let old_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 7200; // 2 hours ago

        // Create stale entry
        let stale = VmRegistryEntry {
            name: "stale-vm".to_string(),
            created_at: old_time,
            static_ip: None,
            creator_pid: std::process::id(),
            status: VmStatus::Building,
            updated_at: old_time,
        };
        registry.entries.insert("stale-vm".to_string(), stale);

        let stale_vms = registry.find_stale(3600); // Max age 1 hour
        assert_eq!(stale_vms.len(), 1);
        assert_eq!(stale_vms[0].name, "stale-vm");
    }
}

