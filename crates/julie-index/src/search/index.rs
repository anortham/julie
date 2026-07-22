//! Tantivy-based search index for code symbols and file content.
//!
//! Manages a single Tantivy index containing two document types:
//! - Symbol documents (functions, classes, structs, etc.)
//! - File content documents (full file text for line-level search)
//!
//! The `CodeTokenizer` handles CamelCase/snake_case splitting so that
//! searching "user" finds both `getUserData` and `get_user_data`.

mod compatibility;
mod lifecycle;
mod mutation;
mod query;
mod types;

pub use compatibility::SEARCH_COMPAT_MARKER_FILE;
pub use lifecycle::{SearchIndexOpenDisposition, SearchIndexOpenOutcome};
#[cfg(any(test, feature = "test-support"))]
#[allow(unused_imports)]
pub(crate) use query::classify_file_match;
pub use query::compact_alnum_lc;
#[cfg(any(test, feature = "test-support"))]
pub use query::{apply_reranker_to_content_results, apply_symbol_title_boost_to_file_results};
#[cfg(any(test, feature = "test-support"))]
pub use types::{ContentSearchResult, ContentSearchResults};
pub use types::{
    FileMatchKind, FileSearchResult, FileSearchResults, SearchDocument, SearchFilter,
    SymbolSearchResult, SymbolSearchResults, UnifiedHit, truncate_utf8_bytes,
};
pub(crate) use types::{is_test_symbol_result, symbol_role_and_test_role};

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[cfg(any(test, feature = "test-support"))]
use std::sync::mpsc::{Receiver, SyncSender};

use tantivy::{Index, IndexReader, IndexWriter};

use crate::search::language_config::LanguageConfigs;
use crate::search::schema::SchemaFields;

#[cfg(any(test, feature = "test-support"))]
struct RebuildPauseForTest {
    cleared: SyncSender<()>,
    resume: Receiver<()>,
}

/// Tantivy-backed search index for code intelligence.
///
/// Supports indexing code symbols and file content, with code-aware
/// tokenization that handles CamelCase, snake_case, and language-specific
/// operator patterns.
pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<Option<IndexWriter>>,
    schema_fields: SchemaFields,
    /// Language configs for post-search scoring (important_patterns boost).
    /// Present when created via `_with_language_configs` constructors.
    language_configs: Option<LanguageConfigs>,
    /// When true, `get_or_create_writer()` returns `Err(Shutdown)` and no new
    /// writes are accepted. Set by `shutdown()` after committing + dropping the writer.
    shutdown: AtomicBool,
    #[cfg(any(test, feature = "test-support"))]
    rebuild_pause_for_test: Mutex<Option<RebuildPauseForTest>>,
    #[cfg(any(test, feature = "test-support"))]
    rebuild_failure_for_test: AtomicBool,
}

/// Shared handle for a workspace search index.
///
/// `SearchIndex` already uses `&self` APIs with an interior writer mutex and a
/// Tantivy `IndexReader` that supports concurrent searchers. Callers share this
/// via `Arc` — do **not** wrap it in an outer `Mutex`, which would serialize
/// readers and block search behind long-running hybrid embedding RPCs.
pub type SearchIndexHandle = Arc<SearchIndex>;
