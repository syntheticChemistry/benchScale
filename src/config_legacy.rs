// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deprecated compatibility shim for the legacy TOML configuration types.
//!
//! The implementation lives in [`crate::config::legacy`]. Prefer
//! [`crate::config::BenchScaleConfig`] for new code.
#![allow(deprecated)]

pub use crate::config::legacy::*;
