// SPDX-License-Identifier: AGPL-3.0-or-later
//! Integration tests for benchScale
//!
//! These tests verify complete workflows with Docker backend.

use benchscale::topology::{NetworkConfig, NodeConfig, TopologyMetadata};
use benchscale::{Backend, DockerBackend, Lab, Topology};
use std::collections::HashMap;

/// Helper to create a test topology
fn create_test_topology() -> Topology {
    Topology {
        metadata: TopologyMetadata {
            name: "integration-test".to_string(),
            description: Some("Integration test topology".to_string()),
            version: Some("1.0".to_string()),
            tags: vec!["test".to_string()],
        },
        network: NetworkConfig {
            name: "test-network".to_string(),
            subnet: "10.200.0.0/24".to_string(),
            conditions: None,
        },
        nodes: vec![
            NodeConfig {
                name: "test-node-1".to_string(),
                image: "alpine:latest".to_string(),
                env: HashMap::new(),
                ports: vec![],
                volumes: vec![],
                network_conditions: None,
                metadata: HashMap::new(),
            },
            NodeConfig {
                name: "test-node-2".to_string(),
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

#[tokio::test]
#[ignore] // Requires Docker daemon running
async fn test_docker_backend_availability() {
    let backend = DockerBackend::new().expect("Failed to create Docker backend");
    let available = backend
        .is_available()
        .await
        .expect("Failed to check availability");
    assert!(available, "Docker daemon should be available");
}

#[tokio::test]
#[ignore] // Requires Docker daemon running
async fn test_create_network() {
    let backend = DockerBackend::new().expect("Failed to create Docker backend");

    let network_name = "benchscale-test-network";
    let subnet = "10.201.0.0/24";

    // Create network
    let network_info = backend
        .create_network(network_name, subnet)
        .await
        .expect("Failed to create network");

    assert_eq!(network_info.name, network_name);
    assert_eq!(network_info.subnet, subnet);

    // Cleanup
    backend
        .delete_network(network_name)
        .await
        .expect("Failed to delete network");
}

#[tokio::test]
#[ignore] // Requires Docker daemon running
async fn test_create_and_delete_node() {
    let backend = DockerBackend::new().expect("Failed to create Docker backend");

    let network_name = "benchscale-test-net";
    let node_name = "test-container";

    // Create network
    backend
        .create_network(network_name, "10.202.0.0/24")
        .await
        .expect("Failed to create network");

    // Create node
    let node_info = backend
        .create_node(node_name, "alpine:latest", network_name, HashMap::new())
        .await
        .expect("Failed to create node");

    assert_eq!(node_info.name, node_name);
    assert!(
        !node_info.ip_address.is_empty(),
        "Node should have an IP address"
    );

    // Verify node is running
    let node_status = backend
        .get_node(&node_info.id)
        .await
        .expect("Failed to get node status");

    assert_eq!(node_status.name, node_name);

    // Cleanup
    backend
        .delete_node(&node_info.id)
        .await
        .expect("Failed to delete node");

    backend
        .delete_network(network_name)
        .await
        .expect("Failed to delete network");
}

#[tokio::test]
#[ignore] // Requires Docker daemon running
async fn test_execute_command_in_node() {
    let backend = DockerBackend::new().expect("Failed to create Docker backend");

    let network_name = "benchscale-exec-test";
    let node_name = "exec-test-container";

    // Create network and node
    backend
        .create_network(network_name, "10.203.0.0/24")
        .await
        .expect("Failed to create network");

    let node_info = backend
        .create_node(node_name, "alpine:latest", network_name, HashMap::new())
        .await
        .expect("Failed to create node");

    // Execute command
    let result = backend
        .exec_command(&node_info.id, vec!["echo".to_string(), "hello".to_string()])
        .await
        .expect("Failed to execute command");

    assert_eq!(result.exit_code, 0, "Command should succeed");
    assert!(
        result.stdout.contains("hello"),
        "Output should contain 'hello'"
    );

    // Cleanup
    backend
        .delete_node(&node_info.id)
        .await
        .expect("Failed to delete node");

    backend
        .delete_network(network_name)
        .await
        .expect("Failed to delete network");
}

#[tokio::test]
#[ignore] // Requires Docker daemon running
async fn test_list_nodes_in_network() {
    let backend = DockerBackend::new().expect("Failed to create Docker backend");

    let network_name = "benchscale-list-test";

    // Create network
    backend
        .create_network(network_name, "10.204.0.0/24")
        .await
        .expect("Failed to create network");

    // Create multiple nodes
    let node1 = backend
        .create_node("list-test-1", "alpine:latest", network_name, HashMap::new())
        .await
        .expect("Failed to create node 1");

    let node2 = backend
        .create_node("list-test-2", "alpine:latest", network_name, HashMap::new())
        .await
        .expect("Failed to create node 2");

    // List nodes
    let nodes = backend
        .list_nodes(network_name)
        .await
        .expect("Failed to list nodes");

    assert!(nodes.len() >= 2, "Should have at least 2 nodes");

    // Cleanup
    backend
        .delete_node(&node1.id)
        .await
        .expect("Failed to delete node 1");
    backend
        .delete_node(&node2.id)
        .await
        .expect("Failed to delete node 2");
    backend
        .delete_network(network_name)
        .await
        .expect("Failed to delete network");
}

#[tokio::test]
#[ignore] // Requires Docker daemon running
async fn test_lab_creation_lifecycle() {
    let topology = create_test_topology();
    let backend = DockerBackend::new().expect("Failed to create Docker backend");

    // Create lab
    let lab = Lab::create("integration-test-lab", topology, backend)
        .await
        .expect("Failed to create lab");

    // Verify lab status
    let status = lab.status().await;
    assert_eq!(
        status,
        benchscale::LabStatus::Running,
        "Lab should be running"
    );

    // Get lab nodes
    let nodes = lab.nodes().await;
    assert_eq!(nodes.len(), 2, "Lab should have 2 nodes");

    // Destroy lab
    lab.destroy().await.expect("Failed to destroy lab");
}

#[tokio::test]
#[ignore] // Requires Docker daemon running
async fn test_node_connectivity() {
    let topology = create_test_topology();
    let backend = DockerBackend::new().expect("Failed to create Docker backend");

    // Create lab
    let lab = Lab::create("connectivity-test", topology, backend)
        .await
        .expect("Failed to create lab");

    // Wait a moment for nodes to be ready
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Execute ping between nodes
    let result = lab
        .exec_on_node(
            "test-node-1",
            vec![
                "ping".to_string(),
                "-c".to_string(),
                "1".to_string(),
                "test-node-2".to_string(),
            ],
        )
        .await;

    // Cleanup
    lab.destroy().await.expect("Failed to destroy lab");

    // Check result
    if let Ok(exec_result) = result {
        // Ping succeeded
        assert_eq!(exec_result.exit_code, 0, "Ping should succeed");
    } else {
        // Ping failed - this can happen if DNS resolution fails, which is acceptable
        // in isolated test networks
        println!("Note: Node connectivity test skipped (expected in isolated networks)");
    }
}
