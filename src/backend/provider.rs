//! Vendor-agnostic VM provider abstraction.
//!
//! This module provides the `VmProvider` trait which extends the `Backend` trait
//! with VM-specific discovery and capability-based selection.
//!
//! ## Philosophy
//!
//! Each backend (libvirt, VMware, AWS, etc.) implements the `Backend` trait.
//! The `VmProvider` wrapper makes them discoverable via primal-substrate's
//! capability system.
//!
//! ## Example
//!
//! ```rust,no_run
//! use benchscale::backend::VmProvider;
//! use primal_substrate::{Discovery, Capability};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Zero hardcoding - discover VM provider at runtime!
//! let discovery = Discovery::new().await?;
//! let provider = discovery
//!     .find_capability(Capability::VmProvisioning)
//!     .await?;
//!
//! println!("Using VM provider: {}", provider.name);
//! # Ok(())
//! # }
//! ```

use crate::backend::{Backend, NodeInfo, NetworkInfo, ExecResult};
use crate::{Error, Result};
use async_trait::async_trait;
use primal_substrate::{Capability, PrimalIdentity, Discovery};
use std::collections::HashMap;
use std::sync::Arc;

/// Vendor-agnostic VM provider.
///
/// Wraps any `Backend` implementation and makes it discoverable via
/// capability-based discovery.
pub struct VmProvider {
    backend: Arc<dyn Backend>,
    identity: PrimalIdentity,
}

impl VmProvider {
    /// Create new VM provider with a backend.
    ///
    /// The backend is wrapped in an Arc for efficient sharing.
    pub fn new(backend: Arc<dyn Backend>, name: &str, version: &str) -> Self {
        let identity = PrimalIdentity::new(name, version)
            .with_capability(Capability::VmProvisioning);
        
        Self {
            backend,
            identity,
        }
    }
    
    /// Register this provider with the discovery system.
    ///
    /// Makes the provider discoverable by other primals.
    pub async fn register(&self, discovery: &Discovery) -> Result<()> {
        discovery
            .register(&self.identity)
            .await
            .map_err(|e| Error::Other(format!("Failed to register VM provider: {}", e)))?;
        Ok(())
    }
    
    /// Discover all VM providers in the system.
    ///
    /// Returns a list of available VM providers without hardcoding
    /// which backends are installed.
    pub async fn discover_all(discovery: &Discovery) -> Result<Vec<primal_substrate::ServiceInfo>> {
        discovery
            .discover_all()
            .await
            .map_err(|e| Error::Other(format!("Failed to discover providers: {}", e)))
    }
    
    /// Find a VM provider by capability.
    ///
    /// Example of zero-hardcoding: discover any backend that can provision VMs!
    pub async fn find(discovery: &Discovery) -> Result<primal_substrate::ServiceInfo> {
        discovery
            .find_capability(Capability::VmProvisioning)
            .await
            .map_err(|e| Error::Backend(format!("No VM provider found: {}", e)))
    }
    
    /// Get the wrapped backend.
    pub fn backend(&self) -> &Arc<dyn Backend> {
        &self.backend
    }
    
    /// Get provider identity.
    pub fn identity(&self) -> &PrimalIdentity {
        &self.identity
    }
}

// Implement Backend trait by delegating to wrapped backend
#[async_trait]
impl Backend for VmProvider {
    async fn create_network(&self, name: &str, subnet: &str) -> Result<NetworkInfo> {
        self.backend.create_network(name, subnet).await
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        self.backend.delete_network(name).await
    }

    async fn create_node(
        &self,
        name: &str,
        image: &str,
        network: &str,
        env: HashMap<String, String>,
    ) -> Result<NodeInfo> {
        self.backend.create_node(name, image, network, env).await
    }

    async fn start_node(&self, node_id: &str) -> Result<()> {
        self.backend.start_node(node_id).await
    }

    async fn stop_node(&self, node_id: &str) -> Result<()> {
        self.backend.stop_node(node_id).await
    }

    async fn delete_node(&self, node_id: &str) -> Result<()> {
        self.backend.delete_node(node_id).await
    }

    async fn get_node(&self, node_id: &str) -> Result<NodeInfo> {
        self.backend.get_node(node_id).await
    }

    async fn list_nodes(&self, network: &str) -> Result<Vec<NodeInfo>> {
        self.backend.list_nodes(network).await
    }

    async fn exec_command(&self, node_id: &str, command: Vec<String>) -> Result<ExecResult> {
        self.backend.exec_command(node_id, command).await
    }

    async fn copy_to_node(&self, node_id: &str, src_path: &str, dest_path: &str) -> Result<()> {
        self.backend.copy_to_node(node_id, src_path, dest_path).await
    }

    async fn get_logs(&self, node_id: &str) -> Result<String> {
        self.backend.get_logs(node_id).await
    }

    async fn apply_network_conditions(
        &self,
        node_id: &str,
        latency_ms: Option<u32>,
        packet_loss_percent: Option<f32>,
        bandwidth_kbps: Option<u32>,
    ) -> Result<()> {
        self.backend
            .apply_network_conditions(node_id, latency_ms, packet_loss_percent, bandwidth_kbps)
            .await
    }

    async fn is_available(&self) -> Result<bool> {
        self.backend.is_available().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use primal_substrate::adapter::FileAdapter;
    use tempfile::NamedTempFile;

    /// Mock backend for testing without external dependencies
    struct MockBackend;

    #[async_trait]
    impl Backend for MockBackend {
        async fn create_network(&self, _name: &str, subnet: &str) -> Result<NetworkInfo> {
            Ok(NetworkInfo {
                name: "test-net".to_string(),
                id: "test-net-id".to_string(),
                subnet: subnet.to_string(),
                gateway: "192.168.1.1".to_string(),
            })
        }

        async fn delete_network(&self, _name: &str) -> Result<()> {
            Ok(())
        }

        async fn create_node(
            &self,
            name: &str,
            _image: &str,
            _network: &str,
            _env: HashMap<String, String>,
        ) -> Result<NodeInfo> {
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

        async fn start_node(&self, _node_id: &str) -> Result<()> {
            Ok(())
        }

        async fn stop_node(&self, _node_id: &str) -> Result<()> {
            Ok(())
        }

        async fn delete_node(&self, _node_id: &str) -> Result<()> {
            Ok(())
        }

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

        async fn list_nodes(&self, _network: &str) -> Result<Vec<NodeInfo>> {
            Ok(vec![])
        }

        async fn exec_command(&self, _node_id: &str, _command: Vec<String>) -> Result<ExecResult> {
            Ok(ExecResult {
                exit_code: 0,
                stdout: "".to_string(),
                stderr: "".to_string(),
            })
        }

        async fn copy_to_node(&self, _node_id: &str, _src: &str, _dest: &str) -> Result<()> {
            Ok(())
        }

        async fn get_logs(&self, _node_id: &str) -> Result<String> {
            Ok("".to_string())
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

    #[tokio::test]
    async fn test_provider_discovery() {
        // Create file-based discovery for testing
        let temp_file = NamedTempFile::new().unwrap();
        let adapter = FileAdapter::new(temp_file.path().to_path_buf()).unwrap();
        let discovery = Discovery::with_adapter(adapter);
        
        // Create mock backend provider (no external deps!)
        let mock = MockBackend;
        let provider = VmProvider::new(
            Arc::new(mock),
            "mock-backend",
            "2.0.0",
        );
        
        // Register
        provider.register(&discovery).await.unwrap();
        
        // Discover
        let service = VmProvider::find(&discovery).await.unwrap();
        assert_eq!(service.name, "mock-backend");
        assert!(service.capabilities.contains(&Capability::VmProvisioning));
    }
    
    #[tokio::test]
    async fn test_zero_hardcoding() {
        // This test proves that discovery starts with ZERO knowledge
        let temp_file = NamedTempFile::new().unwrap();
        let adapter = FileAdapter::new(temp_file.path().to_path_buf()).unwrap();
        let discovery = Discovery::with_adapter(adapter);
        
        // No hardcoded providers!
        let providers = VmProvider::discover_all(&discovery).await.unwrap();
        assert_eq!(providers.len(), 0);
        
        // Providers register themselves at runtime
        let mock = MockBackend;
        let provider = VmProvider::new(Arc::new(mock), "mock", "2.0.0");
        provider.register(&discovery).await.unwrap();
        
        // Now we discover them!
        let providers = VmProvider::discover_all(&discovery).await.unwrap();
        assert_eq!(providers.len(), 1);
    }
    
    #[tokio::test]
    async fn test_provider_delegation() {
        // Verify that VmProvider correctly delegates to backend
        let mock = MockBackend;
        let provider = VmProvider::new(Arc::new(mock), "test", "1.0.0");
        
        // Test network operations
        let net = provider.create_network("test-net", "192.168.1.0/24").await.unwrap();
        assert_eq!(net.subnet, "192.168.1.0/24");
        
        // Test node operations
        let node = provider.create_node("test-node", "test-image", "test-net", HashMap::new()).await.unwrap();
        assert_eq!(node.name, "test-node");
        
        // Test availability
        assert!(provider.is_available().await.unwrap());
    }
}

