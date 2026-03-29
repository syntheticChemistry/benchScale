// SPDX-License-Identifier: AGPL-3.0-only
//! Deployment utilities for benchScale.
//!
//! Handles cross-architecture binary resolution for primal binaries
//! from `plasmidBin` or local build artifacts.

pub mod arch;
pub mod plasmid;

pub use arch::{Arch, BinaryResolver};
pub use plasmid::{deploy_primals_to_node, list_available_primals, DeployedBinary};
