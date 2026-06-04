//! Root safety checks — relocated to `julie_core::workspace::root_safety`.
//!
//! All items re-exported so existing `crate::workspace::root_safety::*` import
//! sites compile unchanged.
pub use julie_core::workspace::root_safety::{
    reject_sensitive_cwd_workspace_root, reject_sensitive_workspace_root,
};

// Only needed by intra-crate tests (root_safety.rs moved to julie-runtime tests in T2c.3).
// cfg(test) is sufficient: no cross-crate consumer remains.
#[cfg(test)]
pub use julie_core::workspace::root_safety::{
    is_sensitive_workspace_root, sensitive_root_candidates,
};
