//! Content type classification for search results.
//!
//! Tags search results by their source: code symbols, memories, or documentation.
//! Used for filtering and per-type weighting in cross-content search.

use std::fmt;

use serde::Serialize;

/// The type of content a search result represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    /// Code symbol (function, class, struct, etc.)
    Code,
    /// Developer memory (checkpoint, decision, plan)
    Memory,
    /// Documentation (markdown, README, etc.)
    Doc,
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentType::Code => write!(f, "code"),
            ContentType::Memory => write!(f, "memory"),
            ContentType::Doc => write!(f, "doc"),
        }
    }
}

impl ContentType {
    /// Parse from a string (case-insensitive).
    /// Parse from a string (case-insensitive). Returns `None` for "all" or unknown.
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "code" => Some(ContentType::Code),
            "memory" | "memories" => Some(ContentType::Memory),
            "doc" | "docs" | "documentation" => Some(ContentType::Doc),
            _ => None,
        }
    }
}

/// A search result tagged with its content type.
///
/// Wraps any result (code symbol, memory, etc.) with a `ContentType` tag
/// for use in unified cross-content search without modifying the core
/// `SymbolSearchResult` struct (which has 30+ construction sites).
#[derive(Debug, Clone)]
pub struct TaggedResult<T> {
    pub content_type: ContentType,
    pub result: T,
    /// Unified score after cross-content RRF merge.
    pub score: f32,
}

impl<T> TaggedResult<T> {
    pub fn new(content_type: ContentType, result: T, score: f32) -> Self {
        Self {
            content_type,
            result,
            score,
        }
    }
}
