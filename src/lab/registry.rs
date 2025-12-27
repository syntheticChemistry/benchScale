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
    use crate::topology::{NetworkConfig, NodeConfig, TopologyMetadata};
    use std::collections::HashMap;

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

    #[tokio::test]
    async fn test_update_lab() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        let mut metadata = registry
            .register_lab(
                "test-id".to_string(),
                "test-lab".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        let original_updated_at = metadata.updated_at;

        // Small delay to ensure timestamp changes
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        metadata.status = LabStatus::Running;
        registry.update_lab(&metadata).await.unwrap();

        let loaded = registry.load_lab("test-id").await.unwrap();
        assert_eq!(loaded.status, LabStatus::Running);
        assert!(loaded.updated_at > original_updated_at);
    }

    #[tokio::test]
    async fn test_load_nonexistent_lab() {
        let registry = create_test_registry().await;
        let result = registry.load_lab("nonexistent-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_lab_by_name() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        registry
            .register_lab(
                "test-id".to_string(),
                "unique-lab-name".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        let loaded = registry.load_lab_by_name("unique-lab-name").await.unwrap();
        assert_eq!(loaded.id, "test-id");
    }

    #[tokio::test]
    async fn test_load_lab_by_name_not_found() {
        let registry = create_test_registry().await;
        let result = registry.load_lab_by_name("nonexistent-name").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_count_labs() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        assert_eq!(registry.count_labs().await.unwrap(), 0);

        registry
            .register_lab(
                "lab1".to_string(),
                "Lab 1".to_string(),
                topology.clone(),
                "docker".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(registry.count_labs().await.unwrap(), 1);

        registry
            .register_lab(
                "lab2".to_string(),
                "Lab 2".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(registry.count_labs().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_cleanup_stale_labs() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        // Create a failed lab
        let mut failed_lab = registry
            .register_lab(
                "failed-lab".to_string(),
                "Failed Lab".to_string(),
                topology.clone(),
                "docker".to_string(),
            )
            .await
            .unwrap();

        failed_lab.status = LabStatus::Failed;
        // Set updated_at to 10 days ago - must do this AFTER update
        failed_lab.updated_at = chrono::Utc::now() - chrono::Duration::days(10);
        // Save directly to preserve the old timestamp
        registry.save_lab(&failed_lab).await.unwrap();

        // Create a running lab (should not be cleaned)
        registry
            .register_lab(
                "running-lab".to_string(),
                "Running Lab".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        // Clean up labs older than 5 days
        let cleaned = registry.cleanup_stale_labs(5).await.unwrap();
        assert_eq!(cleaned, 1);

        // Verify failed lab is gone, running lab remains
        assert!(registry.load_lab("failed-lab").await.is_err());
        assert!(registry.load_lab("running-lab").await.is_ok());
    }

    #[tokio::test]
    async fn test_cleanup_destroyed_labs() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        // Create a destroyed lab
        let mut destroyed_lab = registry
            .register_lab(
                "destroyed-lab".to_string(),
                "Destroyed Lab".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        destroyed_lab.status = LabStatus::Destroyed;
        destroyed_lab.updated_at = chrono::Utc::now() - chrono::Duration::days(10);
        // Save directly to preserve the old timestamp
        registry.save_lab(&destroyed_lab).await.unwrap();

        let cleaned = registry.cleanup_stale_labs(5).await.unwrap();
        assert_eq!(cleaned, 1);
    }

    #[tokio::test]
    async fn test_cleanup_no_stale_labs() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        // Create recent labs
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

        let cleaned = registry.cleanup_stale_labs(30).await.unwrap();
        assert_eq!(cleaned, 0);
    }

    #[tokio::test]
    async fn test_list_labs_sorted_by_creation() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        // Create labs in sequence with small delays
        registry
            .register_lab(
                "lab1".to_string(),
                "First Lab".to_string(),
                topology.clone(),
                "docker".to_string(),
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        registry
            .register_lab(
                "lab2".to_string(),
                "Second Lab".to_string(),
                topology.clone(),
                "docker".to_string(),
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        registry
            .register_lab(
                "lab3".to_string(),
                "Third Lab".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        let labs = registry.list_labs().await.unwrap();
        assert_eq!(labs.len(), 3);

        // Should be sorted newest first
        assert_eq!(labs[0].name, "Third Lab");
        assert_eq!(labs[1].name, "Second Lab");
        assert_eq!(labs[2].name, "First Lab");
    }

    #[tokio::test]
    async fn test_delete_nonexistent_lab() {
        let registry = create_test_registry().await;
        // Should not error when deleting nonexistent lab
        let result = registry.delete_lab("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_empty_registry() {
        let registry = create_test_registry().await;
        let labs = registry.list_labs().await.unwrap();
        assert_eq!(labs.len(), 0);
    }

    #[tokio::test]
    async fn test_lab_metadata_fields() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        let _metadata = registry
            .register_lab(
                "test-id".to_string(),
                "test-lab".to_string(),
                topology,
                "libvirt".to_string(),
            )
            .await
            .unwrap();

        let loaded = registry.load_lab("test-id").await.unwrap();
        assert_eq!(loaded.id, "test-id");
        assert_eq!(loaded.name, "test-lab");
        assert_eq!(loaded.backend_type, "libvirt");
        assert_eq!(loaded.status, LabStatus::Creating);
        assert!(loaded.node_ids.is_empty());
        assert!(loaded.network_id.is_none());
        assert!(loaded.created_at <= chrono::Utc::now());
        assert!(loaded.updated_at <= chrono::Utc::now());
    }

    #[tokio::test]
    async fn test_update_lab_with_nodes() {
        let registry = create_test_registry().await;
        let topology = create_test_topology().await;

        let mut metadata = registry
            .register_lab(
                "test-id".to_string(),
                "test-lab".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        metadata.node_ids = vec!["node1".to_string(), "node2".to_string()];
        metadata.network_id = Some("net123".to_string());
        metadata.status = LabStatus::Running;

        registry.update_lab(&metadata).await.unwrap();

        let loaded = registry.load_lab("test-id").await.unwrap();
        assert_eq!(loaded.node_ids.len(), 2);
        assert_eq!(loaded.network_id, Some("net123".to_string()));
        assert_eq!(loaded.status, LabStatus::Running);
    }

    #[tokio::test]
    async fn test_registry_from_config() {
        let config = crate::Config::default();
        let registry = LabRegistry::from_config(&config);
        // Should successfully create registry from config
        assert!(registry.list_labs().await.is_ok());
    }

    #[tokio::test]
    async fn test_lab_with_complex_topology() {
        let registry = create_test_registry().await;

        let topology = Topology {
            metadata: TopologyMetadata {
                name: "complex".to_string(),
                description: Some("Complex topology".to_string()),
                version: Some("2.0".to_string()),
                tags: vec!["test".to_string(), "complex".to_string()],
            },
            network: NetworkConfig {
                name: "complex-net".to_string(),
                subnet: "172.16.0.0/16".to_string(),
                conditions: Some(crate::topology::NetworkConditions {
                    latency_ms: Some(10),
                    packet_loss_percent: Some(0.5),
                    bandwidth_kbps: Some(10000),
                }),
            },
            nodes: vec![NodeConfig {
                name: "node1".to_string(),
                image: "nginx".to_string(),
                env: HashMap::from([("KEY".to_string(), "value".to_string())]),
                ports: vec!["80:8080".to_string()],
                volumes: vec!["/data:/mnt".to_string()],
                network_conditions: None,
                metadata: HashMap::new(),
            }],
        };

        let _metadata = registry
            .register_lab(
                "complex-id".to_string(),
                "complex-lab".to_string(),
                topology,
                "docker".to_string(),
            )
            .await
            .unwrap();

        let loaded = registry.load_lab("complex-id").await.unwrap();
        assert_eq!(loaded.topology.nodes.len(), 1);
        assert_eq!(loaded.topology.nodes[0].name, "node1");
        assert!(loaded.topology.network.conditions.is_some());
    }
}
