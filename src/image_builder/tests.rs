// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::backend::{NetworkInfo, NodeInfo};
use std::collections::HashMap;

/// Mock backend for testing without external dependencies
struct MockBackend;

#[async_trait::async_trait]
impl crate::backend::Backend for MockBackend {
    async fn create_network(&self, _name: &str, subnet: &str) -> Result<NetworkInfo> {
        Ok(NetworkInfo {
            name: "test-net".to_string(),
            id: "test-id".to_string(),
            subnet: subnet.to_string(),
            gateway: "192.168.1.1".to_string(),
        })
    }
    async fn delete_network(&self, _name: &str) -> Result<()> { Ok(()) }
    async fn create_node(&self, name: &str, _image: &str, _network: &str, _env: HashMap<String, String>) -> Result<NodeInfo> {
        Ok(NodeInfo {
            id: format!("{}-id", name),
            name: name.to_string(),
            container_id: format!("{}-container", name),
            ip_address: "192.168.1.100".to_string(),
            network: "test-net".to_string(),
            status: crate::backend::NodeStatus::Running,
            metadata: HashMap::new(),
        })
    }
    async fn start_node(&self, _node_id: &str) -> Result<()> { Ok(()) }
    async fn stop_node(&self, _node_id: &str) -> Result<()> { Ok(()) }
    async fn delete_node(&self, _node_id: &str) -> Result<()> { Ok(()) }
    async fn get_node(&self, node_id: &str) -> Result<NodeInfo> {
        Ok(NodeInfo {
            id: node_id.to_string(),
            name: "test-node".to_string(),
            container_id: "test-container".to_string(),
            ip_address: "192.168.1.100".to_string(),
            network: "test-net".to_string(),
            status: crate::backend::NodeStatus::Running,
            metadata: HashMap::new(),
        })
    }
    async fn list_nodes(&self, _network: &str) -> Result<Vec<NodeInfo>> { Ok(vec![]) }
    async fn exec_command(&self, _node_id: &str, _command: Vec<String>) -> Result<crate::backend::ExecResult> {
        Ok(crate::backend::ExecResult {
            exit_code: 0,
            stdout: "".to_string(),
            stderr: "".to_string(),
        })
    }
    async fn copy_to_node(&self, _node_id: &str, _src: &str, _dest: &str) -> Result<()> { Ok(()) }
    async fn get_logs(&self, _node_id: &str) -> Result<String> { Ok("".to_string()) }
    async fn apply_network_conditions(&self, _node_id: &str, _latency_ms: Option<u32>, _packet_loss_percent: Option<f32>, _bandwidth_kbps: Option<u32>) -> Result<()> { Ok(()) }
    async fn is_available(&self) -> Result<bool> { Ok(true) }
}

#[test]
fn test_builder_creation() {
    let backend = Arc::new(MockBackend);
    let builder = ImageBuilder::new("test-image", backend).unwrap();
    assert_eq!(builder.name, "test-image");
    assert_eq!(builder.memory_mb, 4096);
    assert_eq!(builder.vcpus, 2);
}

#[test]
fn test_builder_configuration() {
    let backend = Arc::new(MockBackend);
    let builder = ImageBuilder::new("test", backend)
        .unwrap()
        .with_memory(8192)
        .with_vcpus(4)
        .with_disk_size(50);

    assert_eq!(builder.memory_mb, 8192);
    assert_eq!(builder.vcpus, 4);
    assert_eq!(builder.disk_size_gb, 50);
}

#[test]
fn test_build_steps() {
    let backend = Arc::new(MockBackend);
    let builder = ImageBuilder::new("test", backend)
        .unwrap()
        .add_step(BuildStep::WaitForCloudInit)
        .add_step(BuildStep::InstallPackages(vec!["vim".to_string()]))
        .add_step(BuildStep::Reboot);

    assert_eq!(builder.steps.len(), 3);
}

#[test]
#[cfg(feature = "libvirt")]
fn test_new_libvirt_convenience() {
    let builder = ImageBuilder::new_libvirt("test").unwrap();
    assert_eq!(builder.name, "test");
}
