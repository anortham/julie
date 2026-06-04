//! Workspace startup hint — relocated to `julie_core::workspace::startup_hint`.
//!
//! All items re-exported so existing `crate::workspace::startup_hint::*` import
//! sites compile unchanged.
pub use julie_core::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};
