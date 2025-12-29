//! VM State Machine and Lifecycle Events
//!
//! Defines the VM lifecycle states and valid transitions for production-grade
//! orchestration with handoff support.

use serde::{Deserialize, Serialize};
use std::fmt;

/// VM lifecycle states
///
/// Represents all possible states a VM can be in during its lifecycle.
/// State transitions are validated to ensure correctness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmState {
    /// VM record created, resources allocated but VM not started
    Created,
    /// VM is booting
    Starting,
    /// VM is running and operational
    Running,
    /// VM is shutting down gracefully
    Stopping,
    /// VM is stopped cleanly
    Stopped,
    /// VM encountered an error
    Failed,
    /// VM is paused (for maintenance or handoff)
    Paused,
}

impl VmState {
    /// Check if state transition is valid
    ///
    /// Enforces the state machine rules:
    /// - Created → Starting
    /// - Starting → Running | Failed
    /// - Running → Stopping | Paused | Failed
    /// - Stopping → Stopped | Failed
    /// - Stopped → Starting (restart)
    /// - Paused → Running | Stopping
    /// - Failed → Starting (recover)
    pub fn can_transition_to(&self, next: VmState) -> bool {
        use VmState::*;
        matches!(
            (self, next),
            (Created, Starting)
                | (Starting, Running)
                | (Starting, Failed)
                | (Running, Stopping)
                | (Running, Paused)
                | (Running, Failed)
                | (Stopping, Stopped)
                | (Stopping, Failed)
                | (Stopped, Starting)
                | (Paused, Running)
                | (Paused, Stopping)
                | (Failed, Starting) // Restart after failure
        )
    }

    /// Is VM in a terminal state?
    pub fn is_terminal(&self) -> bool {
        matches!(self, VmState::Stopped | VmState::Failed)
    }

    /// Is VM operational (running or paused)?
    pub fn is_operational(&self) -> bool {
        matches!(self, VmState::Running | VmState::Paused)
    }

    /// Can VM be restarted?
    pub fn can_restart(&self) -> bool {
        matches!(self, VmState::Stopped | VmState::Failed)
    }
}

impl fmt::Display for VmState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmState::Created => write!(f, "created"),
            VmState::Starting => write!(f, "starting"),
            VmState::Running => write!(f, "running"),
            VmState::Stopping => write!(f, "stopping"),
            VmState::Stopped => write!(f, "stopped"),
            VmState::Failed => write!(f, "failed"),
            VmState::Paused => write!(f, "paused"),
        }
    }
}

/// VM lifecycle event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    /// When the event occurred
    pub timestamp: i64,
    /// Type of event
    pub event_type: EventType,
    /// Optional additional details
    pub details: Option<String>,
}

/// Types of lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventType {
    /// State transition
    StateChange {
        /// Previous state
        from: VmState,
        /// New state
        to: VmState,
    },
    /// Error occurred
    Error {
        /// Error code
        code: String,
        /// Error message
        message: String,
    },
    /// User action
    Action {
        /// Action taken
        action: String,
        /// User who performed action
        user: String,
    },
    /// VM handoff
    Handoff {
        /// Previous owner
        from: String,
        /// New owner
        to: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_state_transitions() {
        use VmState::*;

        // Valid transitions
        assert!(Created.can_transition_to(Starting));
        assert!(Starting.can_transition_to(Running));
        assert!(Starting.can_transition_to(Failed));
        assert!(Running.can_transition_to(Stopping));
        assert!(Running.can_transition_to(Paused));
        assert!(Running.can_transition_to(Failed));
        assert!(Stopping.can_transition_to(Stopped));
        assert!(Stopped.can_transition_to(Starting)); // Restart
        assert!(Paused.can_transition_to(Running));
        assert!(Failed.can_transition_to(Starting)); // Recover
    }

    #[test]
    fn test_invalid_state_transitions() {
        use VmState::*;

        // Invalid transitions
        assert!(!Created.can_transition_to(Running)); // Must go through Starting
        assert!(!Created.can_transition_to(Stopped));
        assert!(!Starting.can_transition_to(Stopped)); // Must go through Running or fail
        assert!(!Running.can_transition_to(Created)); // Can't go backwards
        assert!(!Stopped.can_transition_to(Running)); // Must go through Starting
    }

    #[test]
    fn test_terminal_states() {
        use VmState::*;

        assert!(Stopped.is_terminal());
        assert!(Failed.is_terminal());
        assert!(!Running.is_terminal());
        assert!(!Paused.is_terminal());
    }

    #[test]
    fn test_operational_states() {
        use VmState::*;

        assert!(Running.is_operational());
        assert!(Paused.is_operational());
        assert!(!Stopped.is_operational());
        assert!(!Failed.is_operational());
    }

    #[test]
    fn test_can_restart() {
        use VmState::*;

        assert!(Stopped.can_restart());
        assert!(Failed.can_restart());
        assert!(!Running.can_restart());
        assert!(!Paused.can_restart());
    }

    #[test]
    fn test_state_display() {
        use VmState::*;

        assert_eq!(format!("{}", Running), "running");
        assert_eq!(format!("{}", Failed), "failed");
        assert_eq!(format!("{}", Paused), "paused");
    }
}

