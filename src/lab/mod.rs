// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lab management and high-level API

pub mod registry;
pub use registry::{LabMetadata, LabRegistry};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::backend::{Backend, NodeInfo};
use crate::network::NetworkSimulator;
use crate::scenarios::{TestResult, TestRunner, TestScenario};
use crate::topology::Topology;
use crate::{Error, Result};

/// Lab status
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LabStatus {
    /// Lab is being created
    Creating,
    /// Lab is running
    Running,
    /// Lab is being destroyed
    Destroying,
    /// Lab is destroyed
    Destroyed,
    /// Lab creation/operation failed
    Failed,
}

/// Lab handle for managing a distributed test environment
pub struct Lab {
    id: String,
    name: String,
    topology: Topology,
    backend: Arc<dyn Backend>,
    state: Arc<RwLock<LabState>>,
}

/// Internal lab state
struct LabState {
    status: LabStatus,
    network_id: Option<String>,
    nodes: HashMap<String, NodeInfo>,
    error: Option<String>,
}

impl Lab {
    /// Create a new lab from a topology
    pub async fn create<B: Backend + 'static>(
        name: impl Into<String>,
        topology: Topology,
        backend: B,
    ) -> Result<Self> {
        let name = name.into();
        let id = Uuid::new_v4().to_string();

        info!("Creating lab: {} (id: {})", name, id);

        // Validate topology
        topology.validate()?;

        let lab = Self {
            id,
            name: name.clone(), // Need to clone since we use it later in logging
            topology,
            backend: Arc::new(backend),
            state: Arc::new(RwLock::new(LabState {
                status: LabStatus::Creating,
                network_id: None,
                nodes: HashMap::new(),
                error: None,
            })),
        };

        // Create network
        match lab.create_network().await {
            Ok(()) => {
                // Create nodes
                match lab.create_nodes().await {
                    Ok(()) => {
                        // Apply network conditions
                        if let Err(e) = lab.apply_network_conditions().await {
                            warn!("Failed to apply network conditions: {}", e);
                        }

                        // Mark as running
                        let mut state = lab.state.write().await;
                        state.status = LabStatus::Running;
                        info!("Lab {} created successfully", name);
                    }
                    Err(e) => {
                        let mut state = lab.state.write().await;
                        state.status = LabStatus::Failed;
                        state.error = Some(format!("Failed to create nodes: {}", e));
                        return Err(e);
                    }
                }
            }
            Err(e) => {
                let mut state = lab.state.write().await;
                state.status = LabStatus::Failed;
                state.error = Some(format!("Failed to create network: {}", e));
                return Err(e);
            }
        }

        Ok(lab)
    }

    /// Create a new lab from a topology, accepting a pre-built `Arc<dyn Backend>`.
    ///
    /// Useful when the backend is shared across multiple labs (e.g. JSON-RPC server).
    pub async fn create_with_arc(
        name: impl Into<String>,
        topology: Topology,
        backend: Arc<dyn Backend>,
    ) -> Result<Self> {
        let name = name.into();
        let id = Uuid::new_v4().to_string();

        info!("Creating lab: {} (id: {})", name, id);
        topology.validate()?;

        let lab = Self {
            id,
            name: name.clone(),
            topology,
            backend,
            state: Arc::new(RwLock::new(LabState {
                status: LabStatus::Creating,
                network_id: None,
                nodes: HashMap::new(),
                error: None,
            })),
        };

        match lab.create_network().await {
            Ok(()) => match lab.create_nodes().await {
                Ok(()) => {
                    if let Err(e) = lab.apply_network_conditions().await {
                        warn!("Failed to apply network conditions: {}", e);
                    }
                    let mut state = lab.state.write().await;
                    state.status = LabStatus::Running;
                    info!("Lab {} created successfully", name);
                }
                Err(e) => {
                    let mut state = lab.state.write().await;
                    state.status = LabStatus::Failed;
                    state.error = Some(format!("Failed to create nodes: {}", e));
                    return Err(e);
                }
            },
            Err(e) => {
                let mut state = lab.state.write().await;
                state.status = LabStatus::Failed;
                state.error = Some(format!("Failed to create network: {}", e));
                return Err(e);
            }
        }

        Ok(lab)
    }

    /// Create the lab network
    async fn create_network(&self) -> Result<()> {
        info!("Creating network: {}", self.topology.network.name);

        let network_info = self
            .backend
            .create_network(&self.topology.network.name, &self.topology.network.subnet)
            .await?;

        let mut state = self.state.write().await;
        state.network_id = Some(network_info.id);

        Ok(())
    }

    /// Create all nodes in the topology
    async fn create_nodes(&self) -> Result<()> {
        let network_name = &self.topology.network.name;

        for node_config in &self.topology.nodes {
            info!("Creating node: {}", node_config.name);

            let node_info = self
                .backend
                .create_node(
                    &node_config.name,
                    &node_config.image,
                    network_name,
                    node_config.env.clone(),
                )
                .await?;

            let mut state = self.state.write().await;
            state.nodes.insert(node_config.name.clone(), node_info);
        }

        Ok(())
    }

    /// Apply network conditions to nodes
    async fn apply_network_conditions(&self) -> Result<()> {
        let simulator = NetworkSimulator::new();
        let state = self.state.read().await;

        for (node_name, node_info) in &state.nodes {
            if let Some(conditions) = self.topology.get_node_conditions(node_name) {
                info!("Applying network conditions to node: {}", node_name);
                simulator
                    .apply_conditions(self.backend.clone(), &node_info.container_id, &conditions)
                    .await?;
            }
        }

        Ok(())
    }

    /// Deploy a binary to a specific node
    pub async fn deploy_to_node(&self, node_name: &str, binary_path: &str) -> Result<()> {
        info!("Deploying {} to node {}", binary_path, node_name);

        let state = self.state.read().await;
        let node_info = state
            .nodes
            .get(node_name)
            .ok_or_else(|| Error::Lab(format!("Node not found: {}", node_name)))?;

        self.backend
            .copy_to_node(&node_info.container_id, binary_path, "/usr/local/bin/")
            .await?;

        Ok(())
    }

    /// Execute a command on a specific node
    pub async fn exec_on_node(
        &self,
        node_name: &str,
        command: Vec<String>,
    ) -> Result<crate::backend::ExecResult> {
        let state = self.state.read().await;
        let node_info = state
            .nodes
            .get(node_name)
            .ok_or_else(|| Error::Lab(format!("Node not found: {}", node_name)))?;

        self.backend
            .exec_command(&node_info.container_id, command)
            .await
    }

    /// Run test scenarios in the lab
    pub async fn run_tests(&self, scenarios: Vec<TestScenario>) -> Result<Vec<TestResult>> {
        info!("Running {} test scenarios", scenarios.len());

        let mut runner = TestRunner::new();
        for scenario in scenarios {
            runner.add_scenario(scenario);
        }

        // Build node name -> container ID map
        let state = self.state.read().await;
        let lab_nodes: HashMap<String, String> = state
            .nodes
            .iter()
            .map(|(name, info)| (name.clone(), info.container_id.clone()))
            .collect();

        Ok(runner.run_all(self.backend.clone(), &lab_nodes).await)
    }

    /// Get lab status
    pub async fn status(&self) -> LabStatus {
        self.state.read().await.status.clone()
    }

    /// Get lab nodes
    pub async fn nodes(&self) -> Vec<NodeInfo> {
        self.state.read().await.nodes.values().cloned().collect()
    }

    /// Get a specific node
    pub async fn get_node(&self, name: &str) -> Option<NodeInfo> {
        self.state.read().await.nodes.get(name).cloned()
    }

    /// Get lab ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get lab name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get topology
    pub fn topology(&self) -> &Topology {
        &self.topology
    }

    /// Destroy the lab
    pub async fn destroy(&self) -> Result<()> {
        info!("Destroying lab: {}", self.name);

        {
            let mut state = self.state.write().await;
            state.status = LabStatus::Destroying;
        }

        // Stop and delete all nodes
        let nodes = {
            let state = self.state.read().await;
            state.nodes.values().cloned().collect::<Vec<_>>()
        };

        for node in nodes {
            info!("Deleting node: {}", node.name);
            if let Err(e) = self.backend.delete_node(&node.container_id).await {
                warn!("Failed to delete node {}: {}", node.name, e);
            }
        }

        // Delete network
        if let Err(e) = self
            .backend
            .delete_network(&self.topology.network.name)
            .await
        {
            warn!("Failed to delete network: {}", e);
        }

        {
            let mut state = self.state.write().await;
            state.status = LabStatus::Destroyed;
            state.nodes.clear();
            state.network_id = None;
        }

        info!("Lab {} destroyed", self.name);
        Ok(())
    }

    /// Get logs from a node
    pub async fn get_logs(&self, node_name: &str) -> Result<String> {
        let state = self.state.read().await;
        let node_info = state
            .nodes
            .get(node_name)
            .ok_or_else(|| Error::Lab(format!("Node not found: {}", node_name)))?;

        self.backend.get_logs(&node_info.container_id).await
    }
}

/// Handle for managing lab lifecycle
pub struct LabHandle {
    lab: Arc<Lab>,
}

impl LabHandle {
    /// Create a new lab handle
    pub fn new(lab: Lab) -> Self {
        Self { lab: Arc::new(lab) }
    }

    /// Get the lab
    pub fn lab(&self) -> &Lab {
        &self.lab
    }

    /// Destroy the lab when handle is dropped
    pub async fn destroy(self) -> Result<()> {
        self.lab.destroy().await
    }
}

impl Clone for LabHandle {
    fn clone(&self) -> Self {
        Self {
            lab: Arc::clone(&self.lab),
        }
    }
}

#[cfg(test)]
mod tests;
