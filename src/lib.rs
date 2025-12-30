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
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
// Allow some pedantic lints that conflict with our style or are too noisy
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)] // Older format! style is still readable
#![allow(clippy::must_use_candidate)] // Too many false positives
#![allow(clippy::multiple_crate_versions)] // Dependency issue, not ours
#![allow(clippy::significant_drop_tightening)] // Overly pedantic for async code
#![allow(clippy::missing_const_for_fn)] // Async fns can't be const
#![allow(clippy::map_unwrap_or)] // map().unwrap_or_else() is idiomatic
#![allow(clippy::use_self)] // Explicit types are clearer sometimes
#![allow(clippy::doc_markdown)] // Too strict on URL formatting
#![allow(clippy::unused_async)] // Some async fns are trait impls or future-proofing

pub mod backend;
pub mod cloud_init;
pub mod cloud_init_simplified;
pub mod config;
pub mod constants;
pub mod error;
pub mod image_builder;
pub mod lab;
pub mod network;
pub mod tests;
pub mod topology;

#[cfg(feature = "persistence")]
pub mod persistence;

// Re-exports
pub use backend::{Backend, DockerBackend};
pub use cloud_init::{CloudInit, CloudInitBuilder, CloudInitFile, CloudInitUser};
pub use config::Config;
pub use error::{Error, Result};
pub use image_builder::{BuildResult, BuildStep, ImageBuilder};
pub use lab::{Lab, LabHandle, LabMetadata, LabRegistry, LabStatus};
pub use network::{NetworkConditions, NetworkSimulator};
pub use tests::{TestResult, TestRunner, TestScenario};
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
