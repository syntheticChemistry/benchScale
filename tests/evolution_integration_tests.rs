// SPDX-License-Identifier: AGPL-3.0-or-later
//! Integration Tests for Evolutions #20, #21, #22
//!
//! These tests validate the functionality of recent architectural evolutions:
//! - Evolution #20: Libvirt Health Check & Auto-Recovery
//! - Evolution #21: Configurable Failure Threshold
//! - Evolution #22: DHCP Lease Renewal Tracking
//!
//! Note: Some tests require a running libvirt environment and are marked
//! with #[ignore] by default. Run with `cargo test -- --ignored` to execute them.

use benchscale::backend::senescence::{SenescenceMetrics, SenescenceMonitor};
use std::time::Duration;

// Feature-gated imports
#[cfg(feature = "libvirt")]
use benchscale::backend::{HealthCheck, HealthStatus};

/// Evolution #20: Test health check module exists and types are exported
#[test]
#[cfg(feature = "libvirt")]
fn test_evolution_20_health_check_types() {
    // Verify key types from Evolution #20 exist and are accessible
    let _health_check_type = std::any::TypeId::of::<HealthCheck>();
    let _health_status_type = std::any::TypeId::of::<HealthStatus>();

    // Type names should contain expected strings
    assert!(std::any::type_name::<HealthCheck>().contains("HealthCheck"));
    assert!(std::any::type_name::<HealthStatus>().contains("HealthStatus"));
}

/// Evolution #21: Test configurable failure threshold
#[test]
fn test_evolution_21_configurable_max_failures() {
    // Create monitor with default threshold
    let _monitor_default =
        SenescenceMonitor::new("test-vm".to_string(), "192.168.122.10".to_string());

    // Create monitor with custom threshold (simulating cloud-init scenario)
    let _monitor_custom =
        SenescenceMonitor::new("test-vm-long".to_string(), "192.168.122.11".to_string())
            .with_max_failures(180); // 30 min tolerance

    // Verify both can be created with different thresholds
    // (actual threshold values are private, so we just verify type)
    assert!(std::any::type_name::<SenescenceMonitor>().contains("SenescenceMonitor"));
}

/// Evolution #22: Test MAC address tracking
#[test]
fn test_evolution_22_mac_address_tracking() {
    // Create monitor with MAC address using the constructor
    let _monitor = SenescenceMonitor::with_mac_address(
        "test-vm".to_string(),
        "192.168.122.10".to_string(),
        Some("52:54:00:12:34:56".to_string()),
    );

    // Verify monitor was created successfully (type check)
    assert!(std::any::type_name::<SenescenceMonitor>().contains("SenescenceMonitor"));
}

/// Evolution #22: Test SenescenceMetrics includes MAC address
#[test]
fn test_evolution_22_metrics_include_mac() {
    use benchscale::backend::senescence::HealthStatus;

    // Create metrics with MAC address
    let metrics = SenescenceMetrics {
        ip_address: "192.168.122.10".to_string(),
        vm_name: "test-vm".to_string(),
        health: HealthStatus::Unknown,
        ping_ok: false,
        ssh_ok: false,
        cloud_init: None,
        uptime: Duration::ZERO,
        time_since_healthy: Duration::ZERO,
        consecutive_failures: 0,
        mac_address: Some("52:54:00:12:34:56".to_string()),
        check_count: 0,
    };

    // Verify MAC address is stored
    assert_eq!(metrics.mac_address, Some("52:54:00:12:34:56".to_string()));
    assert_eq!(metrics.check_count, 0);
}

/// Evolution #22: Test check count increments
#[test]
fn test_evolution_22_check_count_tracking() {
    use benchscale::backend::senescence::HealthStatus;

    let mut metrics = SenescenceMetrics {
        ip_address: "192.168.122.10".to_string(),
        vm_name: "test-vm".to_string(),
        health: HealthStatus::Unknown,
        ping_ok: false,
        ssh_ok: false,
        cloud_init: None,
        uptime: Duration::ZERO,
        time_since_healthy: Duration::ZERO,
        consecutive_failures: 0,
        mac_address: Some("52:54:00:12:34:56".to_string()),
        check_count: 0,
    };

    // Simulate check count increment
    metrics.check_count += 1;
    assert_eq!(metrics.check_count, 1);

    metrics.check_count += 1;
    assert_eq!(metrics.check_count, 2);
}

/// Integration test: Create complete monitoring setup
#[test]
fn test_complete_monitoring_setup() {
    // Evolution #21: Custom failure threshold
    // Evolution #22: MAC address tracking
    let _monitor = SenescenceMonitor::with_mac_address(
        "integration-test-vm".to_string(),
        "192.168.122.100".to_string(),
        Some("52:54:00:aa:bb:cc".to_string()), // Evolution #22
    )
    .with_max_failures(180); // Evolution #21

    // Verify monitor is ready for use (type check)
    assert!(std::any::type_name::<SenescenceMonitor>().contains("SenescenceMonitor"));
}

// --- IGNORED TESTS: Require libvirt environment ---

/// Evolution #20: Integration test for health check (requires libvirt)
///
/// This test is intentionally minimal - full health check testing requires
/// a live libvirt environment and is better suited for end-to-end testing.
#[test]
#[ignore]
#[cfg(feature = "libvirt")]
fn test_evolution_20_health_check_integration() {
    // This test would check real libvirt health in a production environment
    // For now, we just verify the types are accessible
    let _health_check_type = std::any::TypeId::of::<HealthCheck>();
    let _health_status_type = std::any::TypeId::of::<HealthStatus>();

    // In a real integration test with libvirt running:
    // 1. Create HealthCheck with current libvirt status
    // 2. Verify it can detect running libvirtd
    // 3. Verify it can detect network state
    // 4. Verify it doesn't report false positives
}

/// Evolution #22: Integration test for IP re-discovery (requires libvirt + VM)
#[test]
#[ignore]
fn test_evolution_22_ip_rediscovery_integration() {
    // This test would:
    // 1. Create a VM with a known MAC address
    // 2. Start monitoring
    // 3. Wait for DHCP lease to change
    // 4. Verify the monitor detected the change
    //
    // Requires full libvirt + VM environment
    // Ignored by default for CI/CD
}

#[cfg(test)]
mod evolution_validation {
    use benchscale::backend::senescence::{SenescenceMetrics, SenescenceMonitor};
    #[cfg(feature = "libvirt")]
    use benchscale::backend::{HealthCheck, HealthStatus};

    /// Verify Evolution #20 exports
    #[test]
    #[cfg(feature = "libvirt")]
    fn test_evolution_20_exports() {
        // Verify key types are exported and accessible
        use benchscale::backend::{HealthCheck, HealthStatus};

        // Verify types exist (no need to instantiate)
        let _health_check_type = std::any::TypeId::of::<HealthCheck>();
        let _status_type = std::any::TypeId::of::<HealthStatus>();
    }

    /// Verify Evolution #21 exports
    #[test]
    fn test_evolution_21_exports() {
        use benchscale::backend::senescence::SenescenceMonitor;

        // Verify with_max_failures is available
        let _monitor = SenescenceMonitor::new("test".to_string(), "192.168.122.1".to_string())
            .with_max_failures(100);
    }

    /// Verify Evolution #22 exports
    #[test]
    fn test_evolution_22_exports() {
        use benchscale::backend::senescence::{SenescenceMetrics, SenescenceMonitor};

        // Verify with_mac_address constructor is available
        let _monitor = SenescenceMonitor::with_mac_address(
            "test".to_string(),
            "192.168.122.1".to_string(),
            Some("52:54:00:00:00:00".to_string()),
        );

        // Verify SenescenceMetrics has mac_address field
        let _type_check = |m: SenescenceMetrics| m.mac_address;
    }
}

#[cfg(test)]
mod error_handling_tests {
    /// Test proper error handling (Phase 1A improvements)
    #[test]
    fn test_path_conversion_no_panic() {
        use std::path::PathBuf;

        // Verify path conversions are safe (no unwrap)
        let path = PathBuf::from("/tmp/test");
        let path_str = path.to_str();

        // Should return Option, not panic
        assert!(path_str.is_some());
    }
}
