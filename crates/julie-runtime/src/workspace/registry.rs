//! Workspace registry — relocated to `julie_core::workspace::registry`.
//!
//! All items re-exported so existing `crate::workspace::registry::*` import
//! sites compile unchanged.
pub use julie_core::workspace::registry::{
    OrphanReason, OrphanedIndex, RegistryConfig, RegistryStatistics, WorkspaceEntry,
    WorkspaceRegistry, WorkspaceStatus, WorkspaceType, current_timestamp, generate_workspace_id,
};

// Only needed by intra-crate tests (registry.rs moved to julie-runtime tests in T2c.3).
// cfg(test) is sufficient: no cross-crate consumer remains.
#[cfg(test)]
pub use julie_core::workspace::registry::sanitize_name;
