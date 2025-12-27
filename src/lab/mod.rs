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
use crate::tests::{TestResult, TestRunner, TestScenario};
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
            Ok(_) => {
                // Create nodes
                match lab.create_nodes().await {
                    Ok(_) => {
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
mod tests {
    use super::*;
    use crate::topology::{NetworkConfig, NodeConfig, TopologyMetadata};
    use async_trait::async_trait;

    /// Mock backend for testing
    struct MockBackend {
        fail_network: bool,
        fail_node: bool,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                fail_network: false,
                fail_node: false,
            }
        }

        fn with_network_failure() -> Self {
            Self {
                fail_network: true,
                fail_node: false,
            }
        }

        fn with_node_failure() -> Self {
            Self {
                fail_network: false,
                fail_node: true,
            }
        }
    }

    #[async_trait]
    impl Backend for MockBackend {
        async fn create_network(
            &self,
            name: &str,
            subnet: &str,
        ) -> Result<crate::backend::NetworkInfo> {
            if self.fail_network {
                return Err(Error::Network("Mock network failure".to_string()));
            }
            Ok(crate::backend::NetworkInfo {
                name: name.to_string(),
                id: "mock-net-id".to_string(),
                subnet: subnet.to_string(),
                gateway: "10.0.0.1".to_string(),
            })
        }

        async fn delete_network(&self, _name: &str) -> Result<()> {
            Ok(())
        }

        async fn create_node(
            &self,
            name: &str,
            _image: &str,
            network: &str,
            _env: HashMap<String, String>,
        ) -> Result<crate::backend::NodeInfo> {
            if self.fail_node {
                return Err(Error::Backend("Mock node failure".to_string()));
            }
            Ok(crate::backend::NodeInfo {
                id: format!("mock-{}", name),
                name: name.to_string(),
                container_id: format!("mock-{}", name),
                ip_address: "10.0.0.10".to_string(),
                network: network.to_string(),
                status: crate::backend::NodeStatus::Running,
                metadata: HashMap::new(),
            })
        }

        async fn start_node(&self, _node_id: &str) -> Result<()> {
            Ok(())
        }

        async fn stop_node(&self, _node_id: &str) -> Result<()> {
            Ok(())
        }

        async fn delete_node(&self, _node_id: &str) -> Result<()> {
            Ok(())
        }

        async fn get_node(&self, node_id: &str) -> Result<crate::backend::NodeInfo> {
            Ok(crate::backend::NodeInfo {
                id: node_id.to_string(),
                name: "mock-node".to_string(),
                container_id: node_id.to_string(),
                ip_address: "10.0.0.10".to_string(),
                network: "mock-net".to_string(),
                status: crate::backend::NodeStatus::Running,
                metadata: HashMap::new(),
            })
        }

        async fn list_nodes(&self, _network: &str) -> Result<Vec<crate::backend::NodeInfo>> {
            Ok(vec![])
        }

        async fn exec_command(
            &self,
            _node_id: &str,
            command: Vec<String>,
        ) -> Result<crate::backend::ExecResult> {
            Ok(crate::backend::ExecResult {
                exit_code: 0,
                stdout: format!("Executed: {}", command.join(" ")),
                stderr: String::new(),
            })
        }

        async fn copy_to_node(
            &self,
            _node_id: &str,
            _src_path: &str,
            _dest_path: &str,
        ) -> Result<()> {
            Ok(())
        }

        async fn get_logs(&self, _node_id: &str) -> Result<String> {
            Ok("Mock logs".to_string())
        }

        async fn apply_network_conditions(
            &self,
            _node_id: &str,
            _latency_ms: Option<u32>,
            _packet_loss_percent: Option<f32>,
            _bandwidth_kbps: Option<u32>,
        ) -> Result<()> {
            Ok(())
        }

        async fn is_available(&self) -> Result<bool> {
            Ok(true)
        }
    }

    fn create_test_topology() -> crate::Topology {
        crate::Topology {
            metadata: TopologyMetadata {
                name: "test-topology".to_string(),
                description: Some("Test topology".to_string()),
                version: Some("1.0".to_string()),
                tags: vec![],
            },
            network: NetworkConfig {
                name: "test-net".to_string(),
                subnet: "10.0.0.0/24".to_string(),
                conditions: None,
            },
            nodes: vec![
                NodeConfig {
                    name: "node-1".to_string(),
                    image: "alpine:latest".to_string(),
                    env: HashMap::new(),
                    ports: vec![],
                    volumes: vec![],
                    network_conditions: None,
                    metadata: HashMap::new(),
                },
                NodeConfig {
                    name: "node-2".to_string(),
                    image: "alpine:latest".to_string(),
                    env: HashMap::new(),
                    ports: vec![],
                    volumes: vec![],
                    network_conditions: None,
                    metadata: HashMap::new(),
                },
            ],
        }
    }

    #[test]
    fn test_lab_status() {
        assert_eq!(LabStatus::Creating, LabStatus::Creating);
        assert_ne!(LabStatus::Creating, LabStatus::Running);
    }

    #[tokio::test]
    async fn test_lab_creation_success() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        assert_eq!(lab.status().await, LabStatus::Running);
    }

    #[tokio::test]
    async fn test_lab_creation_network_failure() {
        let topology = create_test_topology();
        let backend = MockBackend::with_network_failure();

        let result = Lab::create("test-lab", topology, backend).await;
        assert!(
            result.is_err(),
            "Lab creation should fail with network error"
        );
    }

    #[tokio::test]
    async fn test_lab_creation_node_failure() {
        let topology = create_test_topology();
        let backend = MockBackend::with_node_failure();

        let result = Lab::create("test-lab", topology, backend).await;
        assert!(result.is_err(), "Lab creation should fail with node error");
    }

    #[tokio::test]
    async fn test_lab_nodes_list() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let nodes = lab.nodes().await;
        assert_eq!(nodes.len(), 2, "Lab should have 2 nodes");
    }

    #[tokio::test]
    async fn test_lab_exec_on_node() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let result = lab
            .exec_on_node("node-1", vec!["echo".to_string(), "test".to_string()])
            .await
            .expect("Command execution should succeed");

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("echo"));
    }

    #[tokio::test]
    async fn test_lab_destroy() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        lab.destroy().await.expect("Lab destruction should succeed");

        assert_eq!(lab.status().await, LabStatus::Destroyed);
    }

    #[tokio::test]
    async fn test_lab_id_and_name() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("my-test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        // ID should be a valid UUID format
        let id = lab.id();
        assert!(!id.is_empty(), "Lab ID should not be empty");
        assert!(id.len() > 10, "Lab ID should be reasonable length");

        // Name should match
        assert_eq!(lab.name(), "my-test-lab");
    }

    #[tokio::test]
    async fn test_lab_get_node() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let node = lab.get_node("node-1").await;
        assert!(node.is_some(), "Should find node-1");
        assert_eq!(node.unwrap().name, "node-1");
    }

    #[tokio::test]
    async fn test_lab_get_nonexistent_node() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let result = lab.get_node("nonexistent").await;
        assert!(result.is_none(), "Should return None for nonexistent node");
    }

    #[tokio::test]
    async fn test_topology_validation() {
        // Create topology with invalid subnet
        let mut topology = create_test_topology();
        topology.network.subnet = "invalid-subnet".to_string();

        let backend = MockBackend::new();

        let result = Lab::create("test-lab", topology, backend).await;
        assert!(
            result.is_err(),
            "Should fail validation with invalid subnet"
        );
    }

    #[tokio::test]
    async fn test_deploy_to_node() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let result = lab.deploy_to_node("node-1", "/path/to/binary").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_deploy_to_nonexistent_node() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let result = lab.deploy_to_node("nonexistent", "/path/to/binary").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_logs() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let result = lab.get_logs("node-1").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Mock logs");
    }

    #[tokio::test]
    async fn test_get_logs_nonexistent_node() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let result = lab.get_logs("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lab_topology_accessor() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology.clone(), backend)
            .await
            .expect("Lab creation should succeed");

        assert_eq!(lab.topology().metadata.name, topology.metadata.name);
        assert_eq!(lab.topology().nodes.len(), topology.nodes.len());
    }

    #[tokio::test]
    async fn test_exec_on_nonexistent_node() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let result = lab
            .exec_on_node("nonexistent", vec!["echo".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_destroy_idempotent() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        // Destroy once
        lab.destroy().await.expect("First destroy should succeed");
        assert_eq!(lab.status().await, LabStatus::Destroyed);

        // Destroy again - should still work (idempotent)
        lab.destroy().await.expect("Second destroy should succeed");
        assert_eq!(lab.status().await, LabStatus::Destroyed);
    }

    #[tokio::test]
    async fn test_multiple_nodes() {
        let mut topology = create_test_topology();
        topology.nodes = vec![
            NodeConfig {
                name: "node-1".to_string(),
                image: "alpine".to_string(),
                env: HashMap::new(),
                ports: vec![],
                volumes: vec![],
                network_conditions: None,
                metadata: HashMap::new(),
            },
            NodeConfig {
                name: "node-2".to_string(),
                image: "alpine".to_string(),
                env: HashMap::new(),
                ports: vec![],
                volumes: vec![],
                network_conditions: None,
                metadata: HashMap::new(),
            },
            NodeConfig {
                name: "node-3".to_string(),
                image: "alpine".to_string(),
                env: HashMap::new(),
                ports: vec![],
                volumes: vec![],
                network_conditions: None,
                metadata: HashMap::new(),
            },
        ];

        let backend = MockBackend::new();
        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        let nodes = lab.nodes().await;
        assert_eq!(nodes.len(), 3);
    }

    #[tokio::test]
    async fn test_run_tests_empty() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        // Run with no test scenarios
        let results = lab
            .run_tests(vec![])
            .await
            .expect("Empty test run should succeed");
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_run_tests_with_scenarios() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");

        // Create a simple test scenario
        let scenario = crate::tests::TestScenario {
            name: "test-scenario".to_string(),
            description: Some("Test scenario".to_string()),
            steps: vec![],
            timeout: None,
        };

        let results = lab
            .run_tests(vec![scenario])
            .await
            .expect("Test run should succeed");
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_lab_handle_creation() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");
        let handle = LabHandle::new(lab);

        assert_eq!(handle.lab().name(), "test-lab");
    }

    #[tokio::test]
    async fn test_lab_handle_clone() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");
        let handle1 = LabHandle::new(lab);
        let handle2 = handle1.clone();

        assert_eq!(handle1.lab().name(), handle2.lab().name());
        assert_eq!(handle1.lab().id(), handle2.lab().id());
    }

    #[tokio::test]
    async fn test_lab_handle_destroy() {
        let topology = create_test_topology();
        let backend = MockBackend::new();

        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Lab creation should succeed");
        let _lab_id = lab.id().to_string();
        let handle = LabHandle::new(lab);

        handle
            .destroy()
            .await
            .expect("Handle destroy should succeed");
        // After destroy, handle is consumed so we can't check status
        // But we verified it doesn't panic
    }

    #[tokio::test]
    async fn test_lab_status_enum_equality() {
        assert_eq!(LabStatus::Creating, LabStatus::Creating);
        assert_eq!(LabStatus::Running, LabStatus::Running);
        assert_eq!(LabStatus::Destroying, LabStatus::Destroying);
        assert_eq!(LabStatus::Destroyed, LabStatus::Destroyed);
        assert_eq!(LabStatus::Failed, LabStatus::Failed);

        assert_ne!(LabStatus::Creating, LabStatus::Running);
        assert_ne!(LabStatus::Running, LabStatus::Destroyed);
    }

    #[tokio::test]
    async fn test_lab_status_debug() {
        let status = LabStatus::Running;
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("Running"));
    }

    #[tokio::test]
    async fn test_lab_empty_topology() {
        let mut topology = create_test_topology();
        topology.nodes = vec![];

        let backend = MockBackend::new();
        let lab = Lab::create("test-lab", topology, backend)
            .await
            .expect("Empty topology should be valid");

        let nodes = lab.nodes().await;
        assert_eq!(nodes.len(), 0);
    }
}
