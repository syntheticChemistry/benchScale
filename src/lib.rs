// SPDX-License-Identifier: AGPL-3.0-or-later
//! # benchScale: Laboratory Substrate for Distributed System Testing
//!
//! A pure Rust framework for creating reproducible, isolated test environments
//! for distributed systems, P2P networks, and multi-node applications.
//!
//! ## Features
//!
//! - **Pure Rust**: No shell scripts, full type safety
//! - **Docker-based**: Uses Docker containers for isolation
//! - **Network Simulation**: Latency, packet loss, bandwidth limits, NAT
//! - **Topology-Driven**: YAML manifests define network topologies
//! - **Hardened Images**: Support for Docker hardened images
//! - **Cross-Platform**: Works on Linux, macOS, and Windows
//!
//! ## Example
//!
//! ```rust,no_run
//! use benchscale::{Lab, Topology, DockerBackend};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Load topology from YAML
//! let topology = Topology::from_file("topologies/simple-lan.yaml").await?;
//!
//! // Create backend
//! let backend = DockerBackend::new()?;
//!
//! // Create lab with backend
//! let lab = Lab::create("my-lab", topology, backend).await?;
//!
//! // Deploy applications
//! lab.deploy_to_node("node-1", "/path/to/binary").await?;
//!
//! // Cleanup
//! lab.destroy().await?;
//! # Ok(())
//! # }
//! ```

#![deny(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
// Allow some pedantic lints that conflict with our style or are too noisy
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)] // Older format! style is still readable
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)] // Builder pattern methods
#![allow(clippy::multiple_crate_versions)] // Transitive dependency issue
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::use_self)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::unused_async)] // Trait impls and future-proofing
#![allow(clippy::option_if_let_else)] // if-let is often clearer than map_or_else
#![allow(clippy::needless_raw_string_hashes)] // r#" often clearer for embedded quotes; pedantic noise

pub mod backend;
pub mod capabilities;
pub mod cloud_init;
/// Simplified cloud-init configuration for common use cases
pub mod cloud_init_simplified;
/// Phase 2: New configuration system
pub mod config;
/// Legacy configuration (backward compatibility)
pub mod config_legacy;
pub mod constants;
/// Cross-architecture binary resolution for primal deployment
pub mod deploy;
pub mod error;
pub mod image_builder;
pub mod lab;
pub mod network;
pub mod scenarios;
/// JSON-RPC 2.0 server (UniBin `server --port` mode)
pub mod server;
pub mod topology;
/// IPC compliance validation for deployed primals
pub mod validation;

#[cfg(feature = "persistence")]
pub mod persistence;

// Re-exports
pub use backend::{Backend, DockerBackend};
pub use cloud_init::{CloudInit, CloudInitBuilder, CloudInitFile, CloudInitUser};
// Phase 2: New configuration system
pub use config::{BenchScaleConfig, MonitoringConfig, TimeoutConfig};
// Legacy config for backward compatibility
#[expect(
    deprecated,
    reason = "Re-export legacy Config until callers migrate to BenchScaleConfig"
)]
pub use config_legacy::Config;
pub use config_legacy::PciPassthroughDevice;
pub use error::{Error, Result};
pub use image_builder::{BuildResult, BuildStep, ImageBuilder};
pub use lab::{Lab, LabHandle, LabMetadata, LabRegistry, LabStatus};
pub use network::{NetworkConditions, NetworkSimulator};
pub use scenarios::{TestResult, TestRunner, TestScenario};
pub use topology::{NodeConfig, Topology, TopologyConfig};

#[cfg(feature = "libvirt")]
pub use backend::LibvirtBackend;

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize benchScale with logging
pub fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}
