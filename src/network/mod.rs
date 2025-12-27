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
}

impl Default for NetworkSimulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
