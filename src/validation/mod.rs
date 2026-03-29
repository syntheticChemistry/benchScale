// SPDX-License-Identifier: AGPL-3.0-only
//! IPC compliance validation for deployed primals.
//!
//! Connects to primal endpoints and verifies they respond to
//! mandatory JSON-RPC 2.0 methods per `PRIMAL_IPC_PROTOCOL.md`.

pub mod ipc_compliance;

pub use ipc_compliance::{ComplianceReport, ComplianceResult, IpcComplianceValidator};
