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
#![warn(missing_docs)]

pub mod backend;
pub mod config;
pub mod error;
pub mod lab;
pub mod network;
pub mod tests;
pub mod topology;

// Re-exports
pub use backend::{Backend, DockerBackend};
pub use config::Config;
pub use error::{Error, Result};
pub use lab::{Lab, LabHandle, LabMetadata, LabRegistry, LabStatus};
pub use network::{NetworkConditions, NetworkSimulator};
pub use tests::{TestResult, TestRunner, TestScenario};
pub use topology::{NodeConfig, Topology, TopologyConfig};

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
