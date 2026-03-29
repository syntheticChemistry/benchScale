// SPDX-License-Identifier: AGPL-3.0-only
//! VM Registry with SQLite Backend
//!
//! Provides persistent storage for VM state, configuration, and lifecycle events.
//! Uses SQLite for reliability, with async operations via sqlx.

use crate::persistence::state::{EventType, LifecycleEvent, VmState};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "persistence")]
use sqlx::{Row, SqlitePool};

/// VM record in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmRecord {
    /// Unique VM identifier
    pub id: String,
    /// Human-readable VM name
    pub name: String,
    /// Unix timestamp when created
    pub created_at: i64,
    /// Unix timestamp when last updated
    pub updated_at: i64,
    /// Current lifecycle state
    pub state: VmState,
    /// IP address (if assigned)
    pub ip_address: Option<String>,
    /// VM configuration (JSON)
    pub config: serde_json::Value,
    /// User metadata (JSON)
    pub metadata: HashMap<String, String>,
    /// Owner (for handoff)
    pub owner: Option<String>,
    /// Project association
    pub project: Option<String>,
    /// Searchable tags
    pub tags: Vec<String>,
}

/// Filter for querying VMs
#[derive(Debug, Clone, Default)]
pub struct VmFilter {
    /// Filter by state
    pub state: Option<Vec<VmState>>,
    /// Filter by owner
    pub owner: Option<String>,
    /// Filter by project
    pub project: Option<String>,
    /// Filter by tag
    pub tags: Vec<String>,
}

/// VM Registry for persistent state management
#[cfg(feature = "persistence")]
pub struct VmRegistry {
    pool: SqlitePool,
}

#[cfg(feature = "persistence")]
impl VmRegistry {
    /// Create new registry with SQLite backend
    ///
    /// # Arguments
    /// * `db_path` - Path to SQLite database file (use `:memory:` for in-memory)
    ///
    /// # Example
    /// ```no_run
    /// use benchscale::persistence::VmRegistry;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let registry = VmRegistry::new("vms.db").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_path)
            .await
            .context("Failed to connect to SQLite database")?;

        let registry = Self { pool };
        registry.initialize_schema().await?;
        Ok(registry)
    }

    /// Initialize database schema
    async fn initialize_schema(&self) -> Result<()> {
        // VMs table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS vms (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                state TEXT NOT NULL,
                ip_address TEXT,
                config TEXT NOT NULL,
                metadata TEXT NOT NULL,
                owner TEXT,
                project TEXT,
                tags TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create vms table")?;

        // Lifecycle events table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS lifecycle_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                vm_id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                event_data TEXT NOT NULL,
                details TEXT,
                FOREIGN KEY (vm_id) REFERENCES vms(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create lifecycle_events table")?;

        // Handoffs table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS handoffs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                vm_id TEXT NOT NULL,
                from_owner TEXT NOT NULL,
                to_owner TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                reason TEXT,
                FOREIGN KEY (vm_id) REFERENCES vms(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create handoffs table")?;

        Ok(())
    }

    /// Register a new VM
    pub async fn register(&self, record: VmRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO vms (id, name, created_at, updated_at, state, ip_address, config, metadata, owner, project, tags)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.id)
        .bind(&record.name)
        .bind(record.created_at)
        .bind(record.updated_at)
        .bind(serde_json::to_string(&record.state)?)
        .bind(record.ip_address.as_ref())
        .bind(serde_json::to_string(&record.config)?)
        .bind(serde_json::to_string(&record.metadata)?)
        .bind(record.owner.as_ref())
        .bind(record.project.as_ref())
        .bind(serde_json::to_string(&record.tags)?)
        .execute(&self.pool)
        .await
        .context("Failed to register VM")?;

        Ok(())
    }

    /// Update VM state with validation
    pub async fn update_state(&self, vm_id: &str, new_state: VmState) -> Result<()> {
        // Get current state
        let current = self.get(vm_id).await?;

        // Validate transition
        if !current.state.can_transition_to(new_state) {
            anyhow::bail!(
                "Invalid state transition for VM {}: {:?} -> {:?}",
                vm_id,
                current.state,
                new_state
            );
        }

        // Update state
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        sqlx::query("UPDATE vms SET state = ?, updated_at = ? WHERE id = ?")
            .bind(serde_json::to_string(&new_state)?)
            .bind(now)
            .bind(vm_id)
            .execute(&self.pool)
            .await
            .context("Failed to update VM state")?;

        // Log event
        self.log_event(
            vm_id,
            EventType::StateChange {
                from: current.state,
                to: new_state,
            },
            None,
        )
        .await?;

        Ok(())
    }

    /// Update VM IP address
    pub async fn update_ip(&self, vm_id: &str, ip: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        sqlx::query("UPDATE vms SET ip_address = ?, updated_at = ? WHERE id = ?")
            .bind(ip)
            .bind(now)
            .bind(vm_id)
            .execute(&self.pool)
            .await
            .context("Failed to update VM IP")?;

        Ok(())
    }

    /// Get VM by ID
    pub async fn get(&self, vm_id: &str) -> Result<VmRecord> {
        let row = sqlx::query("SELECT * FROM vms WHERE id = ?")
            .bind(vm_id)
            .fetch_one(&self.pool)
            .await
            .context("VM not found")?;

        Ok(VmRecord {
            id: row.get("id"),
            name: row.get("name"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            state: serde_json::from_str(row.get("state"))?,
            ip_address: row.get("ip_address"),
            config: serde_json::from_str(row.get("config"))?,
            metadata: serde_json::from_str(row.get("metadata"))?,
            owner: row.get("owner"),
            project: row.get("project"),
            tags: serde_json::from_str(row.get("tags"))?,
        })
    }

    /// List VMs by filter
    pub async fn list(&self, filter: VmFilter) -> Result<Vec<VmRecord>> {
        // Fetch all VMs and filter in-memory
        // This is acceptable for typical VM counts (<1000)
        // For larger deployments, consider SQL WHERE clauses
        let rows = sqlx::query("SELECT * FROM vms")
            .fetch_all(&self.pool)
            .await?;

        let mut records = Vec::new();
        for row in rows {
            let record = VmRecord {
                id: row.get("id"),
                name: row.get("name"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                state: serde_json::from_str(row.get("state"))?,
                ip_address: row.get("ip_address"),
                config: serde_json::from_str(row.get("config"))?,
                metadata: serde_json::from_str(row.get("metadata"))?,
                owner: row.get("owner"),
                project: row.get("project"),
                tags: serde_json::from_str(row.get("tags"))?,
            };

            // Apply filters
            if let Some(states) = &filter.state {
                if !states.contains(&record.state) {
                    continue;
                }
            }

            if let Some(owner) = &filter.owner {
                if record.owner.as_ref() != Some(owner) {
                    continue;
                }
            }

            if let Some(project) = &filter.project {
                if record.project.as_ref() != Some(project) {
                    continue;
                }
            }

            records.push(record);
        }

        Ok(records)
    }

    /// Delete VM record
    pub async fn delete(&self, vm_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM vms WHERE id = ?")
            .bind(vm_id)
            .execute(&self.pool)
            .await
            .context("Failed to delete VM")?;

        Ok(())
    }

    /// Handoff VM to new owner
    pub async fn handoff(
        &self,
        vm_id: &str,
        from: &str,
        to: &str,
        reason: Option<String>,
    ) -> Result<()> {
        // Update owner
        sqlx::query("UPDATE vms SET owner = ? WHERE id = ? AND owner = ?")
            .bind(to)
            .bind(vm_id)
            .bind(from)
            .execute(&self.pool)
            .await
            .context("Failed to handoff VM (owner mismatch?)")?;

        // Record handoff
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO handoffs (vm_id, from_owner, to_owner, timestamp, reason) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(vm_id)
        .bind(from)
        .bind(to)
        .bind(now)
        .bind(reason.as_ref())
        .execute(&self.pool)
        .await?;

        // Log event
        self.log_event(
            vm_id,
            EventType::Handoff {
                from: from.to_string(),
                to: to.to_string(),
            },
            reason,
        )
        .await?;

        Ok(())
    }

    /// Log lifecycle event
    pub async fn log_event(
        &self,
        vm_id: &str,
        event: EventType,
        details: Option<String>,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        let event_type = match &event {
            EventType::StateChange { .. } => "state_change",
            EventType::Error { .. } => "error",
            EventType::Action { .. } => "action",
            EventType::Handoff { .. } => "handoff",
        };

        sqlx::query(
            "INSERT INTO lifecycle_events (vm_id, timestamp, event_type, event_data, details) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(vm_id)
        .bind(now)
        .bind(event_type)
        .bind(serde_json::to_string(&event)?)
        .bind(details)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get VM lifecycle history
    pub async fn get_history(&self, vm_id: &str) -> Result<Vec<LifecycleEvent>> {
        let rows =
            sqlx::query("SELECT * FROM lifecycle_events WHERE vm_id = ? ORDER BY timestamp ASC")
                .bind(vm_id)
                .fetch_all(&self.pool)
                .await?;

        let mut events = Vec::new();
        for row in rows {
            events.push(LifecycleEvent {
                timestamp: row.get("timestamp"),
                event_type: serde_json::from_str(row.get("event_data"))?,
                details: row.get("details"),
            });
        }

        Ok(events)
    }
}

#[cfg(all(test, feature = "persistence"))]
mod tests {
    use super::*;

    async fn create_test_registry() -> VmRegistry {
        VmRegistry::new(":memory:").await.unwrap()
    }

    fn create_test_record(id: &str, name: &str) -> VmRecord {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        VmRecord {
            id: id.to_string(),
            name: name.to_string(),
            created_at: now,
            updated_at: now,
            state: VmState::Created,
            ip_address: None,
            config: serde_json::json!({}),
            metadata: HashMap::new(),
            owner: Some("alice".to_string()),
            project: Some("test-project".to_string()),
            tags: vec!["test".to_string()],
        }
    }

    #[tokio::test]
    async fn test_register_and_get_vm() {
        let registry = create_test_registry().await;
        let record = create_test_record("vm-1", "test-vm");

        registry.register(record.clone()).await.unwrap();

        let retrieved = registry.get("vm-1").await.unwrap();
        assert_eq!(retrieved.id, "vm-1");
        assert_eq!(retrieved.name, "test-vm");
        assert_eq!(retrieved.state, VmState::Created);
    }

    #[tokio::test]
    async fn test_state_transition_validation() {
        let registry = create_test_registry().await;
        let record = create_test_record("vm-1", "test-vm");

        registry.register(record).await.unwrap();

        // Valid transition
        registry
            .update_state("vm-1", VmState::Starting)
            .await
            .unwrap();
        registry
            .update_state("vm-1", VmState::Running)
            .await
            .unwrap();

        // Invalid transition
        let result = registry.update_state("vm-1", VmState::Created).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_vm_handoff() {
        let registry = create_test_registry().await;
        let record = create_test_record("vm-1", "test-vm");

        registry.register(record).await.unwrap();

        // Handoff
        registry
            .handoff("vm-1", "alice", "bob", Some("scaling up".to_string()))
            .await
            .unwrap();

        // Verify
        let retrieved = registry.get("vm-1").await.unwrap();
        assert_eq!(retrieved.owner, Some("bob".to_string()));

        // Check history
        let history = registry.get_history("vm-1").await.unwrap();
        assert!(history
            .iter()
            .any(|e| matches!(e.event_type, EventType::Handoff { .. })));
    }

    #[tokio::test]
    async fn test_list_with_filter() {
        let registry = create_test_registry().await;

        // Register multiple VMs
        let record1 = create_test_record("vm-1", "vm1");
        registry.register(record1).await.unwrap();

        let record2 = create_test_record("vm-2", "vm2");
        registry.register(record2).await.unwrap();

        // Update vm-1 to Running (proper state transitions)
        registry
            .update_state("vm-1", VmState::Starting)
            .await
            .unwrap();
        registry
            .update_state("vm-1", VmState::Running)
            .await
            .unwrap();

        // List running VMs
        let running = registry
            .list(VmFilter {
                state: Some(vec![VmState::Running]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, "vm-1");
    }

    #[tokio::test]
    async fn test_lifecycle_events() {
        let registry = create_test_registry().await;
        let record = create_test_record("vm-1", "test-vm");

        registry.register(record).await.unwrap();

        // State transitions
        registry
            .update_state("vm-1", VmState::Starting)
            .await
            .unwrap();
        registry
            .update_state("vm-1", VmState::Running)
            .await
            .unwrap();

        // Get history
        let history = registry.get_history("vm-1").await.unwrap();

        assert_eq!(history.len(), 2);
        assert!(matches!(
            history[0].event_type,
            EventType::StateChange { .. }
        ));
    }

    #[tokio::test]
    async fn test_update_ip() {
        let registry = create_test_registry().await;
        let record = create_test_record("vm-1", "test-vm");

        registry.register(record).await.unwrap();

        // Update IP
        registry.update_ip("vm-1", "192.168.1.100").await.unwrap();

        // Verify
        let retrieved = registry.get("vm-1").await.unwrap();
        assert_eq!(retrieved.ip_address, Some("192.168.1.100".to_string()));
    }

    #[tokio::test]
    async fn test_delete_vm() {
        let registry = create_test_registry().await;
        let record = create_test_record("vm-1", "test-vm");

        registry.register(record).await.unwrap();

        // Delete
        registry.delete("vm-1").await.unwrap();

        // Verify
        let result = registry.get("vm-1").await;
        assert!(result.is_err());
    }
}
