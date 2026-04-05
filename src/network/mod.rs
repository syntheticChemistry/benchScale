// SPDX-License-Identifier: AGPL-3.0-or-later
//! Network simulation capabilities

pub use crate::topology::NetworkConditions;

/// Network simulator for applying conditions to containers
pub struct NetworkSimulator;

impl NetworkSimulator {
    /// Create a new network simulator
    pub fn new() -> Self {
        Self
    }

    /// Apply network conditions to a node via backend
    pub async fn apply_conditions(
        &self,
        backend: std::sync::Arc<dyn crate::backend::Backend>,
        node_id: &str,
        conditions: &NetworkConditions,
    ) -> crate::Result<()> {
        backend
            .apply_network_conditions(
                node_id,
                conditions.latency_ms,
                conditions.packet_loss_percent,
                conditions.bandwidth_kbps,
            )
            .await
    }

    /// Simulate LAN conditions (low latency, no loss)
    pub fn lan_conditions() -> NetworkConditions {
        NetworkConditions {
            latency_ms: Some(1),
            packet_loss_percent: Some(0.0),
            bandwidth_kbps: Some(1_000_000), // 1 Gbps
        }
    }

    /// Simulate WAN conditions (higher latency, some loss)
    pub fn wan_conditions() -> NetworkConditions {
        NetworkConditions {
            latency_ms: Some(50),
            packet_loss_percent: Some(0.1),
            bandwidth_kbps: Some(100_000), // 100 Mbps
        }
    }

    /// Simulate slow network (high latency, packet loss)
    pub fn slow_network_conditions() -> NetworkConditions {
        NetworkConditions {
            latency_ms: Some(200),
            packet_loss_percent: Some(5.0),
            bandwidth_kbps: Some(10_000), // 10 Mbps
        }
    }

    /// Simulate cellular/mobile conditions
    pub fn cellular_conditions() -> NetworkConditions {
        NetworkConditions {
            latency_ms: Some(100),
            packet_loss_percent: Some(2.0),
            bandwidth_kbps: Some(50_000), // 50 Mbps
        }
    }

    /// Simulate NAT/firewall scenario (no bandwidth/latency limits, just isolation)
    pub fn nat_conditions() -> NetworkConditions {
        NetworkConditions {
            latency_ms: None,
            packet_loss_percent: None,
            bandwidth_kbps: None,
        }
    }

    /// Resolve a named preset to its network conditions.
    ///
    /// Ecosystem presets (from wateringHole standards):
    /// - `basement_lan` — 1ms, 0% loss, 1 Gbps
    /// - `campus`       — 5ms, 0.01% loss, 500 Mbps
    /// - `broadband`    — 20ms, 0.1% loss, 100 Mbps
    /// - `cellular`     — 80ms, 2% loss, 30 Mbps
    /// - `satellite`    — 600ms, 5% loss, 5 Mbps
    pub fn from_preset(name: &str) -> Option<NetworkConditions> {
        match name {
            "basement_lan" => Some(Self::lan_conditions()),
            "campus" => Some(NetworkConditions {
                latency_ms: Some(5),
                packet_loss_percent: Some(0.01),
                bandwidth_kbps: Some(500_000),
            }),
            "broadband" => Some(NetworkConditions {
                latency_ms: Some(20),
                packet_loss_percent: Some(0.1),
                bandwidth_kbps: Some(100_000),
            }),
            "cellular" => Some(Self::cellular_conditions()),
            "satellite" => Some(NetworkConditions {
                latency_ms: Some(600),
                packet_loss_percent: Some(5.0),
                bandwidth_kbps: Some(5_000),
            }),
            _ => None,
        }
    }
}

impl Default for NetworkSimulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Backend;
    use async_trait::async_trait;
    use std::sync::Arc;

    struct MockBackend {
        fail: bool,
    }

    /// Deep debt solution: MockBackend returns proper errors instead of panicking.
    /// Mocks should be isolated to testing and return errors for unimplemented methods
    /// rather than causing panics.
    #[async_trait]
    impl Backend for MockBackend {
        async fn create_network(
            &self,
            _: &str,
            _: &str,
        ) -> crate::Result<crate::backend::NetworkInfo> {
            Err(crate::Error::Test(
                "MockBackend::create_network not implemented - use real backend or implement for test".to_string()
            ))
        }
        async fn delete_network(&self, _: &str) -> crate::Result<()> {
            Err(crate::Error::Test(
                "MockBackend::delete_network not implemented - use real backend or implement for test".to_string()
            ))
        }
        async fn create_node(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: std::collections::HashMap<String, String>,
        ) -> crate::Result<crate::backend::NodeInfo> {
            Err(crate::Error::Test(
                "MockBackend::create_node not implemented - use real backend or implement for test"
                    .to_string(),
            ))
        }
        async fn start_node(&self, _: &str) -> crate::Result<()> {
            Err(crate::Error::Test(
                "MockBackend::start_node not implemented - use real backend or implement for test"
                    .to_string(),
            ))
        }
        async fn stop_node(&self, _: &str) -> crate::Result<()> {
            Err(crate::Error::Test(
                "MockBackend::stop_node not implemented - use real backend or implement for test"
                    .to_string(),
            ))
        }
        async fn delete_node(&self, _: &str) -> crate::Result<()> {
            Err(crate::Error::Test(
                "MockBackend::delete_node not implemented - use real backend or implement for test"
                    .to_string(),
            ))
        }
        async fn get_node(&self, _: &str) -> crate::Result<crate::backend::NodeInfo> {
            Err(crate::Error::Test(
                "MockBackend::get_node not implemented - use real backend or implement for test"
                    .to_string(),
            ))
        }
        async fn list_nodes(&self, _: &str) -> crate::Result<Vec<crate::backend::NodeInfo>> {
            Err(crate::Error::Test(
                "MockBackend::list_nodes not implemented - use real backend or implement for test"
                    .to_string(),
            ))
        }
        async fn exec_command(
            &self,
            _: &str,
            _: Vec<String>,
        ) -> crate::Result<crate::backend::ExecResult> {
            Err(crate::Error::Test(
                "MockBackend::exec_command not implemented - use real backend or implement for test".to_string()
            ))
        }
        async fn copy_to_node(&self, _: &str, _: &str, _: &str) -> crate::Result<()> {
            Err(crate::Error::Test(
                "MockBackend::copy_to_node not implemented - use real backend or implement for test".to_string()
            ))
        }
        async fn get_logs(&self, _: &str) -> crate::Result<String> {
            Err(crate::Error::Test(
                "MockBackend::get_logs not implemented - use real backend or implement for test"
                    .to_string(),
            ))
        }

        async fn apply_network_conditions(
            &self,
            _node_id: &str,
            _latency_ms: Option<u32>,
            _packet_loss_percent: Option<f32>,
            _bandwidth_kbps: Option<u32>,
        ) -> crate::Result<()> {
            if self.fail {
                Err(crate::Error::Network(
                    "Mock network condition failure".to_string(),
                ))
            } else {
                Ok(())
            }
        }

        async fn is_available(&self) -> crate::Result<bool> {
            Ok(true)
        }
    }

    #[test]
    fn test_preset_conditions() {
        let lan = NetworkSimulator::lan_conditions();
        assert_eq!(lan.latency_ms, Some(1));
        assert_eq!(lan.packet_loss_percent, Some(0.0));

        let wan = NetworkSimulator::wan_conditions();
        assert_eq!(wan.latency_ms, Some(50));
        assert!(wan.packet_loss_percent.unwrap() > 0.0);

        let slow = NetworkSimulator::slow_network_conditions();
        assert!(slow.latency_ms.unwrap() > 100);
        assert!(slow.packet_loss_percent.unwrap() > 1.0);
    }

    #[test]
    fn test_lan_conditions() {
        let conditions = NetworkSimulator::lan_conditions();
        assert_eq!(conditions.latency_ms, Some(1));
        assert_eq!(conditions.packet_loss_percent, Some(0.0));
        assert_eq!(conditions.bandwidth_kbps, Some(1_000_000));
    }

    #[test]
    fn test_wan_conditions() {
        let conditions = NetworkSimulator::wan_conditions();
        assert_eq!(conditions.latency_ms, Some(50));
        assert_eq!(conditions.packet_loss_percent, Some(0.1));
        assert_eq!(conditions.bandwidth_kbps, Some(100_000));
    }

    #[test]
    fn test_slow_network_conditions() {
        let conditions = NetworkSimulator::slow_network_conditions();
        assert_eq!(conditions.latency_ms, Some(200));
        assert_eq!(conditions.packet_loss_percent, Some(5.0));
        assert_eq!(conditions.bandwidth_kbps, Some(10_000));
    }

    #[test]
    fn test_cellular_conditions() {
        let conditions = NetworkSimulator::cellular_conditions();
        assert_eq!(conditions.latency_ms, Some(100));
        assert_eq!(conditions.packet_loss_percent, Some(2.0));
        assert_eq!(conditions.bandwidth_kbps, Some(50_000));
    }

    #[test]
    fn test_nat_conditions() {
        let conditions = NetworkSimulator::nat_conditions();
        assert!(conditions.latency_ms.is_none());
        assert!(conditions.packet_loss_percent.is_none());
        assert!(conditions.bandwidth_kbps.is_none());
    }

    #[test]
    fn test_simulator_creation() {
        let _simulator = NetworkSimulator::new();
        let _default = NetworkSimulator::default();
    }

    #[tokio::test]
    async fn test_apply_conditions_success() {
        let simulator = NetworkSimulator::new();
        let backend: Arc<dyn Backend> = Arc::new(MockBackend { fail: false });
        let conditions = NetworkSimulator::lan_conditions();

        let result = simulator
            .apply_conditions(backend, "test-node", &conditions)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_apply_conditions_failure() {
        let simulator = NetworkSimulator::new();
        let backend: Arc<dyn Backend> = Arc::new(MockBackend { fail: true });
        let conditions = NetworkSimulator::wan_conditions();

        let result = simulator
            .apply_conditions(backend, "test-node", &conditions)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_apply_all_preset_conditions() {
        let simulator = NetworkSimulator::new();
        let backend: Arc<dyn Backend> = Arc::new(MockBackend { fail: false });

        let presets = vec![
            NetworkSimulator::lan_conditions(),
            NetworkSimulator::wan_conditions(),
            NetworkSimulator::slow_network_conditions(),
            NetworkSimulator::cellular_conditions(),
            NetworkSimulator::nat_conditions(),
        ];

        for conditions in presets {
            let result = simulator
                .apply_conditions(backend.clone(), "test-node", &conditions)
                .await;
            assert!(
                result.is_ok(),
                "Failed to apply conditions: {:?}",
                conditions
            );
        }
    }
}
