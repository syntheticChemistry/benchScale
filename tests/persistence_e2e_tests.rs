//! End-to-End Tests for Persistence Layer
//!
//! Tests complete lifecycle scenarios including:
//! - VM creation through lifecycle manager
//! - Startup recovery
//! - Live handoffs
//! - Multi-VM coordination

#[cfg(feature = "persistence")]
use benchscale::backend::{Backend, ExecResult, NetworkInfo, NodeInfo, NodeStatus};
#[cfg(feature = "persistence")]
use benchscale::persistence::{LifecycleManager, VmConfig, VmFilter, VmRegistry, VmState};
#[cfg(feature = "persistence")]
use async_trait::async_trait;
#[cfg(feature = "persistence")]
use std::collections::HashMap;
#[cfg(feature = "persistence")]
use std::sync::Arc;

#[cfg(feature = "persistence")]
mod persistence_e2e {
    use super::*;

    // Mock backend for E2E testing
    struct TestBackend;

    #[async_trait]
    impl Backend for TestBackend {
        async fn create_network(
            &self,
            _name: &str,
            _subnet: &str,
        ) -> benchscale::Result<NetworkInfo> {
            Ok(NetworkInfo {
                name: "test-net".to_string(),
                id: "test-net-id".to_string(),
                subnet: "192.168.1.0/24".to_string(),
                gateway: "192.168.1.1".to_string(),
            })
        }

        async fn delete_network(&self, _name: &str) -> benchscale::Result<()> {
            Ok(())
        }

        async fn create_node(
            &self,
            _name: &str,
            _image: &str,
            _network: &str,
            _env: HashMap<String, String>,
        ) -> benchscale::Result<NodeInfo> {
            Ok(NodeInfo {
                id: "test-node".to_string(),
                name: "test-vm".to_string(),
                container_id: "test-container".to_string(),
                ip_address: "192.168.1.100".to_string(),
                network: "test-network".to_string(),
                status: NodeStatus::Running,
                metadata: HashMap::new(),
            })
        }

        async fn start_node(&self, _node_id: &str) -> benchscale::Result<()> {
            Ok(())
        }

        async fn stop_node(&self, _node_id: &str) -> benchscale::Result<()> {
            Ok(())
        }

        async fn delete_node(&self, _node_id: &str) -> benchscale::Result<()> {
            Ok(())
        }

        async fn get_node(&self, _node_id: &str) -> benchscale::Result<NodeInfo> {
            Ok(NodeInfo {
                id: "test-node".to_string(),
                name: "test-vm".to_string(),
                container_id: "test-container".to_string(),
                ip_address: "192.168.1.100".to_string(),
                network: "test-network".to_string(),
                status: NodeStatus::Running,
                metadata: HashMap::new(),
            })
        }

        async fn list_nodes(&self, _network: &str) -> benchscale::Result<Vec<NodeInfo>> {
            Ok(vec![])
        }

        async fn exec_command(
            &self,
            _node_id: &str,
            _command: Vec<String>,
        ) -> benchscale::Result<ExecResult> {
            Ok(ExecResult {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }

        async fn copy_to_node(
            &self,
            _node_id: &str,
            _src_path: &str,
            _dest_path: &str,
        ) -> benchscale::Result<()> {
            Ok(())
        }

        async fn get_logs(&self, _node_id: &str) -> benchscale::Result<String> {
            Ok(String::new())
        }

        async fn apply_network_conditions(
            &self,
            _node_id: &str,
            _latency_ms: Option<u32>,
            _packet_loss_percent: Option<f32>,
            _bandwidth_kbps: Option<u32>,
        ) -> benchscale::Result<()> {
            Ok(())
        }

        async fn is_available(&self) -> benchscale::Result<bool> {
            Ok(true)
        }
    }

    async fn create_test_manager() -> LifecycleManager<TestBackend> {
        let registry = VmRegistry::new(":memory:").await.unwrap();
        let backend = Arc::new(TestBackend);
        LifecycleManager::new(registry, backend)
    }

    #[tokio::test]
    async fn test_e2e_complete_vm_lifecycle() {
        let manager = create_test_manager().await;

        // 1. Create VM
        let vm_id = manager
            .create_vm(VmConfig {
                name: "app-server".to_string(),
                owner: Some("team-a".to_string()),
                project: Some("myapp".to_string()),
                tags: vec!["production".to_string(), "web".to_string()],
                backend_config: serde_json::json!({
                    "memory": "2048",
                    "cpus": 2
                }),
            })
            .await
            .unwrap();

        // Verify initial state
        let vm = manager.get_vm(&vm_id).await.unwrap();
        assert_eq!(vm.state, VmState::Running);
        assert_eq!(vm.owner, Some("team-a".to_string()));

        // 2. Stop VM
        manager.stop_vm(&vm_id).await.unwrap();
        let vm = manager.get_vm(&vm_id).await.unwrap();
        assert_eq!(vm.state, VmState::Stopped);

        // 3. Restart VM
        manager.restart_vm(&vm_id).await.unwrap();
        let vm = manager.get_vm(&vm_id).await.unwrap();
        assert_eq!(vm.state, VmState::Running);

        // 4. Handoff VM
        manager
            .handoff_vm(&vm_id, "team-a", "team-b")
            .await
            .unwrap();
        let vm = manager.get_vm(&vm_id).await.unwrap();
        assert_eq!(vm.owner, Some("team-b".to_string()));

        // 5. Verify history
        let history = manager.get_vm_history(&vm_id).await.unwrap();
        assert!(history.len() >= 5); // Multiple state transitions
    }

    #[tokio::test]
    async fn test_e2e_startup_recovery() {
        let manager = create_test_manager().await;

        // Create multiple VMs
        let vm1 = manager
            .create_vm(VmConfig {
                name: "vm1".to_string(),
                owner: Some("alice".to_string()),
                project: None,
                tags: vec![],
                backend_config: serde_json::json!({}),
            })
            .await
            .unwrap();

        let vm2 = manager
            .create_vm(VmConfig {
                name: "vm2".to_string(),
                owner: Some("bob".to_string()),
                project: None,
                tags: vec![],
                backend_config: serde_json::json!({}),
            })
            .await
            .unwrap();

        // Stop one VM
        manager.stop_vm(&vm2).await.unwrap();

        // Simulate recovery
        let recovered = manager.recover_on_startup().await.unwrap();

        // Only running VMs should be recovered
        assert_eq!(recovered.len(), 1);
        assert!(recovered.contains(&vm1));
        assert!(!recovered.contains(&vm2));
    }

    #[tokio::test]
    async fn test_e2e_multi_tenant_isolation() {
        let manager = create_test_manager().await;

        // Create VMs for different teams
        let _alice_vm1 = manager
            .create_vm(VmConfig {
                name: "alice-vm1".to_string(),
                owner: Some("alice".to_string()),
                project: Some("project-a".to_string()),
                tags: vec!["dev".to_string()],
                backend_config: serde_json::json!({}),
            })
            .await
            .unwrap();

        let _alice_vm2 = manager
            .create_vm(VmConfig {
                name: "alice-vm2".to_string(),
                owner: Some("alice".to_string()),
                project: Some("project-a".to_string()),
                tags: vec!["staging".to_string()],
                backend_config: serde_json::json!({}),
            })
            .await
            .unwrap();

        let _bob_vm1 = manager
            .create_vm(VmConfig {
                name: "bob-vm1".to_string(),
                owner: Some("bob".to_string()),
                project: Some("project-b".to_string()),
                tags: vec!["production".to_string()],
                backend_config: serde_json::json!({}),
            })
            .await
            .unwrap();

        // List Alice's VMs
        let alice_vms = manager
            .list_vms(VmFilter {
                owner: Some("alice".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(alice_vms.len(), 2);
        assert!(alice_vms.iter().all(|vm| vm.owner == Some("alice".to_string())));

        // List Bob's VMs
        let bob_vms = manager
            .list_vms(VmFilter {
                owner: Some("bob".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(bob_vms.len(), 1);
        assert_eq!(bob_vms[0].owner, Some("bob".to_string()));

        // List project-a VMs
        let project_a_vms = manager
            .list_vms(VmFilter {
                project: Some("project-a".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(project_a_vms.len(), 2);
    }

    #[tokio::test]
    async fn test_e2e_live_handoff_workflow() {
        let manager = create_test_manager().await;

        // Create VM for team-a
        let vm_id = manager
            .create_vm(VmConfig {
                name: "critical-service".to_string(),
                owner: Some("team-a".to_string()),
                project: Some("prod".to_string()),
                tags: vec!["critical".to_string()],
                backend_config: serde_json::json!({}),
            })
            .await
            .unwrap();

        // VM is running
        let vm = manager.get_vm(&vm_id).await.unwrap();
        assert_eq!(vm.state, VmState::Running);

        // Handoff to team-b (live!)
        manager
            .handoff_vm(&vm_id, "team-a", "team-b")
            .await
            .unwrap();

        // VM still running, new owner
        let vm = manager.get_vm(&vm_id).await.unwrap();
        assert_eq!(vm.state, VmState::Running);
        assert_eq!(vm.owner, Some("team-b".to_string()));

        // Verify handoff was logged
        let history = manager.get_vm_history(&vm_id).await.unwrap();
        let handoff_events: Vec<_> = history
            .iter()
            .filter(|e| {
                matches!(
                    e.event_type,
                    benchscale::persistence::EventType::Handoff { .. }
                )
            })
            .collect();

        assert_eq!(handoff_events.len(), 1);
    }

    #[tokio::test]
    async fn test_e2e_concurrent_vm_creation() {
        let manager = Arc::new(create_test_manager().await);

        // Create 5 VMs concurrently
        let mut handles = vec![];
        for i in 0..5 {
            let mgr = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                mgr.create_vm(VmConfig {
                    name: format!("vm-{}", i),
                    owner: Some(format!("user-{}", i)),
                    project: None,
                    tags: vec![],
                    backend_config: serde_json::json!({}),
                })
                .await
            });
            handles.push(handle);
        }

        // Wait for all
        let mut results = vec![];
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // All should succeed
        assert_eq!(results.len(), 5);
        for result in results {
            assert!(result.is_ok());
        }

        // Verify all VMs exist
        let all_vms = manager
            .list_vms(VmFilter {
                state: Some(vec![VmState::Running]),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(all_vms.len(), 5);
    }

    #[tokio::test]
    async fn test_e2e_failure_handling() {
        let manager = create_test_manager().await;

        // Create VM
        let vm_id = manager
            .create_vm(VmConfig {
                name: "test-vm".to_string(),
                owner: None,
                project: None,
                tags: vec![],
                backend_config: serde_json::json!({}),
            })
            .await
            .unwrap();

        // Note: In real scenario, external monitoring would detect and mark as Failed

        // Try to restart from running state (should work via stop first)
        manager.stop_vm(&vm_id).await.unwrap();
        manager.restart_vm(&vm_id).await.unwrap();

        let vm = manager.get_vm(&vm_id).await.unwrap();
        assert_eq!(vm.state, VmState::Running);
    }
}

