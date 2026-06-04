//! Root safety checks — relocated to `julie_core::workspace::root_safety`.
//!
//! All items re-exported so existing `crate::workspace::root_safety::*` import
//! sites compile unchanged.
pub use julie_core::workspace::root_safety::{
    reject_sensitive_cwd_workspace_root, reject_sensitive_workspace_root,
};

// Used only in cfg(test) — gate to suppress unused-import warning in non-test builds.
#[cfg(test)]
pub use julie_core::workspace::root_safety::{
    is_sensitive_workspace_root, sensitive_root_candidates,
};
