//! Shared constants and types — relocated to `julie_core::shared`.
//!
//! All items re-exported from `julie_core` so existing `julie_core::shared::*`
//! import sites compile unchanged.
pub use julie_core::shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES, NOISE_CALLEE_NAMES,
    OptimizedResponse,
};

/// Trailer for a paged result: the exact call that returns the next page.
pub fn next_line(tool: &str, args: &[(&str, &str)], next_offset: usize) -> String {
    let mut line = format!("next: {tool}");
    for (key, value) in args {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        line.push_str(value);
    }
    line.push_str(&format!(" offset={next_offset}"));
    line
}
