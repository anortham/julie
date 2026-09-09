//! Fast search tool for code intelligence
//!
//! Provides Tantivy-powered code search with support for:
//! - Code-aware tokenization (CamelCase/snake_case splitting at index time)
//! - Language and file pattern filtering
//! - Line-level grep-style search
//! - Per-workspace isolation

// Public API re-exports
pub use self::backend::SearchBackend;
pub use self::params::{FastSearchExecution, FastSearchParams, FastSearchTool};
pub use self::query::matches_glob_pattern;
pub use self::query_preprocessor::{
    PreprocessedQuery, QueryType, detect_query_type, preprocess_query, sanitize_query,
    validate_query,
};
pub use self::trace::{
    FilePatternDiagnostic, HintKind, LineEnrichmentStatus, SearchExecutionResult, SearchHit,
    SearchTrace, ZeroHitReason,
};
pub use self::types::{LineMatch, LineMatchStrategy};

// Internal modules
pub(crate) mod backend;
pub mod execution;
pub mod formatting; // Exposed for testing
pub mod hint_formatter;
pub mod input_diagnostics;
pub(crate) mod line_enrichment;
pub mod line_mode;
pub mod nl_embeddings;
pub(crate) mod params;
pub mod query;
pub mod query_preprocessor; // Public for testing
pub(crate) mod region_search;
pub mod regions;
pub mod text_search;
pub(crate) mod tool_execution;
pub mod trace;
pub(crate) mod types;
