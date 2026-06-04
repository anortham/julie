//! Workspace registry — relocated to `julie_core::workspace::registry`.
//!
//! All items re-exported so existing `crate::workspace::registry::*` import
//! sites compile unchanged.
pub use julie_core::workspace::registry::{
    OrphanReason, OrphanedIndex, RegistryConfig, RegistryStatistics, WorkspaceEntry,
    WorkspaceRegistry, WorkspaceStatus, WorkspaceType, current_timestamp, generate_workspace_id,
};
