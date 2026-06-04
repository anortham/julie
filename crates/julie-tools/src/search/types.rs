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
/// Supports literal substring matching, quoted phrase verification, and
/// token-based matching with exclusions.
#[derive(Debug, Clone)]
pub enum LineMatchStrategy {
    /// Case-insensitive substring matching; quoted phrases fall back to
    /// ordered tokenizer-term matching inside `line_matches`.
    Substring(String),
    /// Token-based matching with required and excluded terms
    Tokens {
        required: Vec<String>,
        excluded: Vec<String>,
    },
    /// File-level matching: Tantivy guarantees all terms in file,
    /// line matching uses OR to show where each term appears
    FileLevel { terms: Vec<String> },
}
