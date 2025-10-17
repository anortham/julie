//! Helper types for line-mode searching
//!
//! Contains data structures used in grep-style line-level search operations.

/// Represents a single line match in a file
#[derive(Debug, Clone)]
pub struct LineMatch {
    pub file_path: String,
    pub line_number: usize,
    pub line_content: String,
}

/// Strategy for matching individual lines
///
/// Used to determine how to filter lines when performing line-level searches.
/// Supports both substring matching and token-based matching with exclusions.
#[derive(Debug, Clone)]
pub enum LineMatchStrategy {
    /// Simple substring matching (case-insensitive)
    Substring(String),
    /// Token-based matching with required and excluded terms
    Tokens {
        required: Vec<String>,
        excluded: Vec<String>,
    },
}
