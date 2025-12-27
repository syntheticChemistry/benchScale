//! Lab registry for persistent lab state management
//!
//! Enables listing, loading, and managing labs across CLI invocations.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, warn};

use crate::{topology::Topology, Error, LabStatus, Result};

/// Lab metadata stored in registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabMetadata {
    /// Lab unique ID
    pub id: String,
    /// Lab name
    pub name: String,
    /// Lab status
    pub status: LabStatus,
    /// Lab topology
    pub topology: Topology,
    /// Backend type used
    pub backend_type: String,
    /// Node IDs in this lab
    pub node_ids: Vec<String>,
    /// Network ID
    pub network_id: Option<String>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last updated timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Lab registry for managing persistent state
pub struct LabRegistry {
    state_dir: PathBuf,
}

impl LabRegistry {
    /// Create a new lab registry with specified state directory
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    /// Create a lab registry from configuration
    pub fn from_config(config: &crate::Config) -> Self {
        Self::new(config.lab.state_dir.clone())
    }

    /// Ensure state directory exists
    async fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.state_dir).await?;
        Ok(())
    }

    /// Get path to lab metadata file
    fn lab_path(&self, lab_id: &str) -> PathBuf {
        let mut path = self.state_dir.clone();
        path.push(format!("{}.json", lab_id));
        path
    }

    /// Register a new lab
    pub async fn register_lab(
        &self,
        id: String,
        name: String,
        topology: Topology,
        backend_type: String,
    ) -> Result<LabMetadata> {
        self.ensure_dir().await?;

        let now = chrono::Utc::now();
        let metadata = LabMetadata {
            id: id.clone(),
            name,
            status: LabStatus::Creating,
            topology,
            backend_type,
            node_ids: Vec::new(),
            network_id: None,
            created_at: now,
            updated_at: now,
        };

        self.save_lab(&metadata).await?;
        info!("Registered lab: {} ({})", metadata.name, metadata.id);

        Ok(metadata)
    }

    /// Update lab metadata
    pub async fn update_lab(&self, metadata: &LabMetadata) -> Result<()> {
        let mut updated = metadata.clone();
        updated.updated_at = chrono::Utc::now();

        self.save_lab(&updated).await?;
        debug!("Updated lab: {} ({})", updated.name, updated.id);

        Ok(())
    }

    /// Save lab metadata to disk
    async fn save_lab(&self, metadata: &LabMetadata) -> Result<()> {
        let path = self.lab_path(&metadata.id);
        let json = serde_json::to_string_pretty(metadata)?;
        fs::write(&path, json).await?;
        Ok(())
    }

    /// Load lab metadata by ID
    pub async fn load_lab(&self, lab_id: &str) -> Result<LabMetadata> {
        let path = self.lab_path(lab_id);

        if !path.exists() {
            return Err(Error::Lab(format!("Lab not found: {}", lab_id)));
        }

        let json = fs::read_to_string(&path).await?;
        let metadata: LabMetadata = serde_json::from_str(&json)?;

        Ok(metadata)
    }

    /// Load lab metadata by name
    pub async fn load_lab_by_name(&self, name: &str) -> Result<LabMetadata> {
        let labs = self.list_labs().await?;

        for lab in labs {
            if lab.name == name {
                return Ok(lab);
            }
        }

        Err(Error::Lab(format!("Lab not found: {}", name)))
    }

    /// List all registered labs
    pub async fn list_labs(&self) -> Result<Vec<LabMetadata>> {
        self.ensure_dir().await?;

        let mut labs = Vec::new();
        let mut entries = fs::read_dir(&self.state_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match fs::read_to_string(&path).await {
                    Ok(json) => match serde_json::from_str::<LabMetadata>(&json) {
                        Ok(metadata) => labs.push(metadata),
                        Err(e) => warn!("Failed to parse lab metadata {:?}: {}", path, e),
                    },
                    Err(e) => warn!("Failed to read lab metadata {:?}: {}", path, e),
                }
            }
        }

        // Sort by creation time (newest first)
        labs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(labs)
    }

    /// Delete lab from registry
    pub async fn delete_lab(&self, lab_id: &str) -> Result<()> {
        let path = self.lab_path(lab_id);

        if path.exists() {
            fs::remove_file(&path).await?;
            info!("Deleted lab from registry: {}", lab_id);
        }

        Ok(())
    }

    /// Get lab count
    pub async fn count_labs(&self) -> Result<usize> {
        Ok(self.list_labs().await?.len())
    }

    /// Clean up labs in failed or destroyed state
    pub async fn cleanup_stale_labs(&self, max_age_days: u32) -> Result<usize> {
        let labs = self.list_labs().await?;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(max_age_days as i64);
        let mut cleaned = 0;

        for lab in labs {
            let should_clean = match lab.status {
                LabStatus::Destroyed | LabStatus::Failed => lab.updated_at < cutoff,
                _ => false,
            };

            if should_clean {
                if let Err(e) = self.delete_lab(&lab.id).await {
                    warn!("Failed to clean up lab {}: {}", lab.id, e);
                } else {
                    cleaned += 1;
                }
            }
        }

        if cleaned > 0 {
            info!("Cleaned up {} stale labs", cleaned);
        }

        Ok(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::{NetworkConfig, TopologyMetadata};

    async fn create_test_registry() -> LabRegistry {
        let dir = std::env::temp_dir().join(format!("benchscale-test-{}", uuid::Uuid::new_v4()));
        LabRegistry::new(dir)
    }

    async fn create_test_topology() -> Topology {
        Topology {
            metadata: TopologyMetadata {
                name: "test".to_string(),
                description: None,
                version: None,
                tags: vec![],
            },
            network: NetworkConfig {
                name: "test-net".to_string(),
                subnet: "10.0.0.0/24".to_string(),
                conditions: None,
            },
            nodes: vec![],
        }
    }

    #[tokio::test]
    async fn test_register_and_load_lab() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        let metadata = registry
            .register_lab(
                "test-id".to_string(),
                "test-lab".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(metadata.name, "test-lab");
        assert_eq!(metadata.status, LabStatus::Creating);

        let loaded = registry.load_lab("test-id").await.unwrap();
        assert_eq!(loaded.name, "test-lab");
    }

    #[tokio::test]
    async fn test_list_labs() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        registry
            .register_lab(
                "lab1".to_string(),
                "Lab 1".to_string(),
                topology.clone(),
                "docker".to_string(),
            )
            .await
            .unwrap();

        registry
            .register_lab(
                "lab2".to_string(),
                "Lab 2".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        let labs = registry.list_labs().await.unwrap();
        assert_eq!(labs.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_lab() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        registry
            .register_lab(
                "test-id".to_string(),
                "test-lab".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        registry.delete_lab("test-id").await.unwrap();

        assert!(registry.load_lab("test-id").await.is_err());
    }
}
