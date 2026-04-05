// SPDX-License-Identifier: AGPL-3.0-or-later
//! VM Lifecycle Persistence and State Management
//!
//! This module provides production-grade VM lifecycle management with:
//! - Persistent state tracking (SQLite)
//! - State machine validation
//! - Crash recovery
//! - Live handoff capability
//! - Full audit trail

pub mod lifecycle;
pub mod registry;
pub mod state;

pub use lifecycle::{LifecycleManager, VmConfig};
pub use registry::{VmFilter, VmRecord, VmRegistry};
pub use state::{EventType, LifecycleEvent, VmState};
