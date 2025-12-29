//! VM Lifecycle Persistence and State Management
//!
//! This module provides production-grade VM lifecycle management with:
//! - Persistent state tracking (SQLite)
//! - State machine validation
//! - Crash recovery
//! - Live handoff capability
//! - Full audit trail

pub mod registry;
pub mod state;
pub mod lifecycle;

pub use registry::{VmRegistry, VmRecord, VmFilter};
pub use state::{VmState, LifecycleEvent, EventType};
pub use lifecycle::{LifecycleManager, VmConfig};

