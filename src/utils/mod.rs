// Julie's Utilities Module
//
// Common utilities and helper functions used throughout the Julie codebase.


/// File utilities — relocated to `julie_core::file_utils`.
///
/// Re-exported so existing `crate::utils::file_utils::*` import sites
/// compile unchanged.
pub mod file_utils {
    pub use julie_core::file_utils::*;
}

/// Token estimation utilities
pub mod token_estimation;

/// Context truncation utilities
pub mod context_truncation;

/// Progressive reduction utilities
pub mod progressive_reduction;

/// Cross-language intelligence utilities (THE secret sauce!)
pub mod cross_language_intelligence;

/// Path relevance scoring utilities
pub mod path_relevance;

/// Exact match boost utilities
pub mod exact_match_boost;

/// String similarity utilities for fuzzy matching
pub mod string_similarity;

/// Path conversion utilities (absolute ↔ relative Unix-style)
pub mod paths;

/// Shared file walker builder (wraps `ignore` crate for .gitignore + .julieignore support)
pub mod walk;

/// Lenient serde deserializers for MCP tool parameters (string-or-number u32)
pub mod serde_lenient;

/// Language detection utilities — relocated to `julie_core::language`.
///
/// Re-exported so existing `crate::utils::language::*` import sites
/// compile unchanged.
pub mod language {
    pub use julie_core::language::*;
}
