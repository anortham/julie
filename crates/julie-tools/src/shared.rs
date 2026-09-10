//! Shared constants and types — relocated to `julie_core::shared`.
//!
//! All items re-exported from `julie_core` so existing `julie_core::shared::*`
//! import sites compile unchanged.
pub use julie_core::shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES, NOISE_CALLEE_NAMES,
    OptimizedResponse,
};

/// One-line trailer for a bounded tool result that had more rows than it kept.
pub fn truncation_line(kept: usize) -> String {
    format!("Output truncated at {kept} results; narrow the query or pass a smaller limit.")
}
