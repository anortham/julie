//! Tantivy-based search index for code symbols and file content.
//!
//! Manages a single Tantivy index containing two document types:
//! - Symbol documents (functions, classes, structs, etc.)
//! - File content documents (full file text for line-level search)
//!
//! The `CodeTokenizer` handles CamelCase/snake_case splitting so that
//! searching "user" finds both `getUserData` and `get_user_data`.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, TermQuery};
use tantivy::schema::{IndexRecordOption, TantivyDocument, Value};
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{Index, IndexReader, IndexWriter, Term};

use crate::search::error::{Result, SearchError};
use crate::search::expansion::expand_query_terms;
use crate::search::language_config::LanguageConfigs;
use crate::search::query::{
    build_content_query_weighted, build_file_query, build_symbol_query,
    build_symbol_query_weighted, build_unified_query, parse_annotation_query,
};
use crate::search::schema::{
    SchemaCompatibilitySignature, SchemaFields, compatibility_signature, create_schema,
};
use crate::search::scoring::{apply_important_patterns_boost, is_nl_like_query, is_test_path};
use crate::search::tokenizer::{
    CodeTokenizer, SimpleCodeTokenizer, TokenizerCompatibilitySignature, split_camel_case,
};
use crate::tools::search::matches_glob_pattern;

// 256MB total budget. Tantivy 0.26's `Index::writer(budget)` auto-clamps thread
// count when per-thread budget falls below the 15MB floor. At 50MB we got 3
// threads at ~16.67MB each; 256MB gives 8 threads at 32MB each — closer to the
// indexing throughput ceiling on multi-core boxes.
const WRITER_HEAP_SIZE: usize = 256_000_000;
const NL_RERANK_OVERFETCH_FACTOR: usize = 4;
const SEARCH_COMPAT_MARKER_VERSION: u32 = 4;
const ANNOTATION_ORIGINAL_GROUP_WEIGHT: f32 = 5.0;
const ANNOTATION_ALIAS_GROUP_WEIGHT: f32 = 3.5;
const ANNOTATION_NORMALIZED_GROUP_WEIGHT: f32 = 2.5;
const ANNOTATION_NAME_FIELD_BOOST: f32 = 5.0;
const ANNOTATION_SIGNATURE_FIELD_BOOST: f32 = 3.0;
const ANNOTATION_DOC_FIELD_BOOST: f32 = 2.0;
const ANNOTATION_BODY_FIELD_BOOST: f32 = 1.0;
const ANNOTATION_OWNER_FIELD_BOOST: f32 = 4.0;
pub const SEARCH_COMPAT_MARKER_FILE: &str = "julie-search-compat.json";

/// A code symbol to be indexed.
pub struct SymbolDocument {
    pub id: String,
    pub name: String,
    pub signature: String,
    pub doc_comment: String,
    pub code_body: String,
    pub file_path: String,
    pub kind: String,
    pub language: String,
    pub start_line: u32,
}

/// A file's content to be indexed for line-level search.
pub struct FileDocument {
    pub file_path: String,
    pub content: String,
    pub language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMatchKind {
    ExactPath,
    ExactBasename,
    PathFragment,
    Glob,
}

impl SymbolDocument {
    /// Create from a Julie Symbol (from tree-sitter extraction).
    pub fn from_symbol(symbol: &crate::extractors::Symbol) -> Self {
        Self {
            id: symbol.id.clone(),
            name: symbol.name.clone(),
            signature: symbol.signature.clone().unwrap_or_default(),
            doc_comment: symbol.doc_comment.clone().unwrap_or_default(),
            code_body: symbol.code_context.clone().unwrap_or_default(),
            file_path: symbol.file_path.clone(),
            kind: symbol.kind.to_string(),
            language: symbol.language.clone(),
            start_line: symbol.start_line,
        }
    }
}

impl FileDocument {
    /// Create from a Julie FileInfo (from database types).
    pub fn from_file_info(file_info: &crate::database::FileInfo) -> Self {
        Self {
            file_path: file_info.path.clone(),
            content: file_info.content.clone().unwrap_or_default(),
            language: file_info.language.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 2 unified document type
// ---------------------------------------------------------------------------

/// Union-shape document that covers both symbol and file rows.
///
/// Replaces `SymbolDocument` + `FileDocument` as the sole write path once
/// T4 cuts the projection over. Until T9, all three types co-exist in the
/// codebase.
///
/// `doc_type` discriminates rows: `"symbol"` or `"file"`.
///
/// Fields that are only meaningful for one row type should be set to
/// empty strings / zero / empty Vec for the other type:
/// - Symbol rows: populate `id`, `name`, `signature`, `doc_comment`,
///   `code_body`, `kind`, `start_line`, annotation fields; leave `content`
///   and `path_text` empty.
/// - File rows: populate `file_path`, `basename`, `path_text`, `content`,
///   `language`, `name` (basename without extension); leave symbol fields
///   empty.
///
/// `pretokenized_code` and `relationship_text` are written as empty strings
/// in T2; T4 and T7 wire them up via the projection layer.
pub struct SearchDocument {
    // ---- discriminator ----
    pub doc_type: String,           // "symbol" | "file"

    // ---- shared fields ----
    pub id: String,
    pub name: String,
    pub language: String,
    pub file_path: String,
    pub basename: String,
    pub kind: String,               // symbol kind string, or "file"
    pub role: String,               // classify_role result
    pub test_role: String,          // test_subrole result

    // ---- symbol fields ----
    pub signature: String,
    pub doc_comment: String,
    /// Body text, already truncated to ≤ 2000 bytes on a UTF-8 boundary.
    pub code_body: String,
    /// Exact annotation keys (lowercased). Written as multi-value field.
    pub annotation_keys: Vec<String>,
    pub annotations_text: String,
    pub owner_names_text: String,
    pub start_line: u32,

    // ---- file fields ----
    /// Full file content for line-level search.
    pub content: String,
    /// Normalised path for path-fragment search.
    pub path_text: String,

    // ---- Phase 2 fields (wired by T4 / T7, empty in T2) ----
    pub pretokenized_code: String,
    pub relationship_text: String,
}

impl SearchDocument {
    /// Build a symbol-row document from a `Symbol`.
    ///
    /// Callers must supply `annotation_keys`, `annotations_text`, and
    /// `owner_names_text` from `SymbolIndexContext` (projection layer).
    /// `code_body` is truncated here to ≤ 2000 bytes.
    pub fn for_symbol(
        symbol: &crate::extractors::Symbol,
        annotation_keys: Vec<String>,
        annotations_text: String,
        owner_names_text: String,
    ) -> Self {
        let raw_body = symbol.code_context.as_deref().unwrap_or("");
        let code_body = truncate_utf8_bytes(raw_body, 2000).to_string();
        let normalized_path = normalize_file_path(&symbol.file_path);
        let basename = basename_for_path(&normalized_path).to_string();
        let role = crate::search::scoring::classify_role(&normalized_path, &symbol.language);
        let test_role = crate::search::scoring::test_subrole(&normalized_path);
        Self {
            doc_type: "symbol".to_string(),
            id: symbol.id.clone(),
            name: symbol.name.clone(),
            language: symbol.language.clone(),
            file_path: normalized_path,
            basename,
            kind: symbol.kind.to_string(),
            role: role.to_string(),
            test_role: test_role.to_string(),
            signature: symbol.signature.clone().unwrap_or_default(),
            doc_comment: symbol.doc_comment.clone().unwrap_or_default(),
            code_body,
            annotation_keys,
            annotations_text,
            owner_names_text,
            start_line: symbol.start_line,
            content: String::new(),
            path_text: String::new(),
            pretokenized_code: String::new(),
            relationship_text: String::new(),
        }
    }

    /// Build a file-row document from a `FileInfo`.
    ///
    /// `name` is set to the basename without its extension (e.g. `"parser"`
    /// for `src/parser.rs`).
    pub fn for_file(file_info: &crate::database::FileInfo) -> Self {
        let normalized_path = normalize_file_path(&file_info.path);
        let basename = basename_for_path(&normalized_path).to_string();
        let name = stem_of_basename(&basename).to_string();
        let language = file_info.language.clone();
        let role = crate::search::scoring::classify_role(&normalized_path, &language);
        let test_role = crate::search::scoring::test_subrole(&normalized_path);
        let content = file_info.content.clone().unwrap_or_default();
        Self {
            doc_type: "file".to_string(),
            id: String::new(),
            name,
            language,
            file_path: normalized_path.clone(),
            basename,
            kind: "file".to_string(),
            role: role.to_string(),
            test_role: test_role.to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            code_body: String::new(),
            annotation_keys: vec![],
            annotations_text: String::new(),
            owner_names_text: String::new(),
            start_line: 0,
            content,
            path_text: normalized_path,
            pretokenized_code: String::new(),
            relationship_text: String::new(),
        }
    }
}

/// Truncate `s` to at most `max_bytes` bytes on a UTF-8 character boundary.
///
/// Returns a `&str` slice of `s`. When `s.len() <= max_bytes` the original
/// slice is returned unchanged.
pub fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Return the portion of a basename before the first `.`.
///
/// `"parser.rs"` → `"parser"`, `"mod.rs"` → `"mod"`, `"noext"` → `"noext"`.
fn stem_of_basename(basename: &str) -> &str {
    match basename.find('.') {
        Some(pos) => &basename[..pos],
        None => basename,
    }
}

/// Search filter for narrowing results.
#[derive(Default, Clone)]
pub struct SearchFilter {
    pub language: Option<String>,
    pub kind: Option<String>,
    pub file_pattern: Option<String>,
    pub exclude_tests: bool,
}

impl SearchFilter {
    pub fn matches_symbol_result(&self, result: &SymbolSearchResult) -> bool {
        if let Some(language) = self.language.as_deref() {
            if result.language != language {
                return false;
            }
        }

        if let Some(kind) = self.kind.as_deref() {
            if result.kind != kind {
                return false;
            }
        }

        if let Some(pattern) = self.file_pattern.as_deref() {
            if !matches_glob_pattern(&result.file_path, pattern) {
                return false;
            }
        }

        if self.exclude_tests && is_test_path(&result.file_path) {
            return false;
        }

        true
    }
}

fn symbol_result_matches_filter(result: &SymbolSearchResult, filter: &SearchFilter) -> bool {
    filter.matches_symbol_result(result)
}

/// A symbol search result with relevance score.
///
/// `role` and `test_role` are populated from the C.3-enriched Tantivy schema
/// fields when present, or re-derived from `file_path + language` for
/// non-Tantivy result sources (e.g. KNN/embedding fallback in `hybrid.rs`).
/// Consumers can therefore rely on these always being set without inspecting
/// which engine produced the result.
#[derive(Debug, Clone)]
pub struct SymbolSearchResult {
    pub id: String,
    pub name: String,
    pub signature: String,
    pub doc_comment: String,
    pub file_path: String,
    pub kind: String,
    pub language: String,
    pub start_line: u32,
    pub score: f32,
    /// `"src"`, `"test"`, `"docs"`, `"vendor"`, `"generated"`, `"config"`,
    /// `"build"`, or `""` if unclassified.
    pub role: String,
    /// `"impl_test"`, `"helper_test"`, `"fixture_test"`, `"smoke_test"`, or `""`.
    pub test_role: String,
}

/// Result from search_symbols, includes metadata about the search.
pub struct SymbolSearchResults {
    pub results: Vec<SymbolSearchResult>,
    /// True if AND-per-term returned zero results and OR fallback was used
    pub relaxed: bool,
}

/// A file content search result with relevance score.
#[derive(Clone)]
pub struct ContentSearchResult {
    pub file_path: String,
    pub language: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct FileSearchResult {
    pub file_path: String,
    pub language: String,
    pub score: f32,
    pub match_kind: FileMatchKind,
}

pub struct FileSearchResults {
    pub results: Vec<FileSearchResult>,
    pub relaxed: bool,
}

/// Result from search_content, includes metadata about the search.
pub struct ContentSearchResults {
    pub results: Vec<ContentSearchResult>,
    /// True if AND-per-term returned zero results and OR fallback was used
    pub relaxed: bool,
    /// Number of candidate documents returned by the initial AND query.
    ///
    /// Instrumentation for the search-quality hardening work: diagnoses
    /// whether zero-hit queries die at the AND stage or further downstream.
    /// Equal to the truncated `TopDocs` length (bounded by `limit`).
    pub and_candidate_count: usize,
    /// Number of candidate documents returned by the OR fallback query.
    ///
    /// `0` when the OR fallback was not invoked (AND produced results, or the
    /// query was a single word so the fallback gate never fired).
    pub or_candidate_count: usize,
}

// ---------------------------------------------------------------------------
// Phase 2 — unified hit type
// ---------------------------------------------------------------------------

/// A single hit from a unified BM25 sweep across all seven FTS fields.
/// Carries `kind` so callers can distinguish symbol rows from file rows.
#[derive(Debug, Clone)]
pub struct UnifiedHit {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub path_text: String,
    pub file_path: String,
    pub basename: String,
    pub signature: String,
    pub doc_comment: String,
    pub code_body: String,
    pub pretokenized_code: String,
    pub relationship_text: String,
    pub language: String,
    pub start_line: u32,
    pub role: String,
    pub test_role: String,
    pub tantivy_score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchIndexOpenDisposition {
    Compatible,
    RecreatedIncompatible,
    RecreatedOpenFailure,
}

impl SearchIndexOpenDisposition {
    pub fn repair_required(self) -> bool {
        !matches!(self, Self::Compatible)
    }
}

pub struct SearchIndexOpenOutcome {
    pub index: SearchIndex,
    pub disposition: SearchIndexOpenDisposition,
}

impl SearchIndexOpenOutcome {
    pub fn repair_required(&self) -> bool {
        self.disposition.repair_required()
    }

    pub fn into_index(self) -> SearchIndex {
        self.index
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SearchCompatMarker {
    marker_version: u32,
    schema_signature: SchemaCompatibilitySignature,
    tokenizer_signature: TokenizerCompatibilitySignature,
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
}

impl SearchIndex {
    /// Create a new index at the given directory path using default patterns.
    pub fn create(path: &Path) -> Result<Self> {
        let tokenizer = CodeTokenizer::with_default_patterns();
        Self::create_with_tokenizer(path, tokenizer, None)
    }

    /// Create a new index with language-specific tokenizer patterns.
    pub fn create_with_language_configs(path: &Path, configs: &LanguageConfigs) -> Result<Self> {
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::create_with_tokenizer(path, tokenizer, Some(configs.clone()))
    }

    /// Open an existing index at the given directory path.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.join("meta.json").exists() {
            return Err(SearchError::IndexNotFound(path.display().to_string()));
        }
        let tokenizer = CodeTokenizer::with_default_patterns();
        Self::open_with_tokenizer(path, tokenizer, None).map(SearchIndexOpenOutcome::into_index)
    }

    /// Open an existing index with language-specific tokenizer patterns.
    pub fn open_with_language_configs(path: &Path, configs: &LanguageConfigs) -> Result<Self> {
        if !path.join("meta.json").exists() {
            return Err(SearchError::IndexNotFound(path.display().to_string()));
        }
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::open_with_tokenizer(path, tokenizer, Some(configs.clone()))
            .map(SearchIndexOpenOutcome::into_index)
    }

    pub fn open_with_language_configs_outcome(
        path: &Path,
        configs: &LanguageConfigs,
    ) -> Result<SearchIndexOpenOutcome> {
        if !path.join("meta.json").exists() {
            return Err(SearchError::IndexNotFound(path.display().to_string()));
        }
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::open_with_tokenizer(path, tokenizer, Some(configs.clone()))
    }

    /// Open an existing index or create a new one if it doesn't exist.
    pub fn open_or_create(path: &Path) -> Result<Self> {
        let tokenizer = CodeTokenizer::with_default_patterns();
        Self::open_or_create_with_tokenizer(path, tokenizer, None)
            .map(SearchIndexOpenOutcome::into_index)
    }

    /// Open an existing index or create a new one, using language-specific tokenizer patterns.
    pub fn open_or_create_with_language_configs(
        path: &Path,
        configs: &LanguageConfigs,
    ) -> Result<Self> {
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::open_or_create_with_tokenizer(path, tokenizer, Some(configs.clone()))
            .map(SearchIndexOpenOutcome::into_index)
    }

    pub fn open_or_create_with_language_configs_outcome(
        path: &Path,
        configs: &LanguageConfigs,
    ) -> Result<SearchIndexOpenOutcome> {
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::open_or_create_with_tokenizer(path, tokenizer, Some(configs.clone()))
    }

    /// Get the total number of documents in the index.
    pub fn num_docs(&self) -> u64 {
        self.reader.reload().ok();
        self.reader.searcher().num_docs()
    }

    /// Add a symbol document to the index.
    pub fn add_symbol(&self, doc: &SymbolDocument) -> Result<()> {
        self.add_symbol_with_context(doc, &[], "", "")
    }

    /// Add a symbol document plus projection-only annotation and owner context.
    pub fn add_symbol_with_context(
        &self,
        doc: &SymbolDocument,
        annotation_keys: &[String],
        annotations_text: &str,
        owner_names_text: &str,
    ) -> Result<()> {
        let f = &self.schema_fields;
        let mut tantivy_doc = TantivyDocument::new();

        tantivy_doc.add_text(f.doc_type, "symbol");
        tantivy_doc.add_text(f.id, &doc.id);
        tantivy_doc.add_text(f.file_path, &doc.file_path);
        tantivy_doc.add_text(f.language, &doc.language);
        tantivy_doc.add_text(f.name, &doc.name);
        tantivy_doc.add_text(f.signature, &doc.signature);
        tantivy_doc.add_text(f.doc_comment, &doc.doc_comment);
        tantivy_doc.add_text(f.code_body, &doc.code_body);
        for key in annotation_keys {
            let key = key.trim().to_ascii_lowercase();
            if !key.is_empty() {
                tantivy_doc.add_text(f.annotations_exact, &key);
            }
        }
        tantivy_doc.add_text(f.annotations_text, annotations_text);
        tantivy_doc.add_text(f.owner_names_text, owner_names_text);
        tantivy_doc.add_text(f.kind, &doc.kind);
        tantivy_doc.add_u64(f.start_line, doc.start_line as u64);

        // C.3 enriched fields. Derived from path + language.
        let role = crate::search::scoring::classify_role(&doc.file_path, &doc.language);
        let test_role = crate::search::scoring::test_subrole(&doc.file_path);
        tantivy_doc.add_text(f.role, role);
        tantivy_doc.add_text(f.test_role, test_role);

        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.add_document(tantivy_doc)?;
        Ok(())
    }

    /// Add a file content document to the index.
    pub fn add_file_content(&self, doc: &FileDocument) -> Result<()> {
        let f = &self.schema_fields;
        let mut tantivy_doc = TantivyDocument::new();
        let normalized_path = normalize_file_path(&doc.file_path);
        let basename = basename_for_path(&normalized_path);

        tantivy_doc.add_text(f.doc_type, "file");
        tantivy_doc.add_text(f.file_path, &normalized_path);
        tantivy_doc.add_text(f.basename, basename);
        tantivy_doc.add_text(f.path_text, &normalized_path);
        tantivy_doc.add_text(f.language, &doc.language);
        tantivy_doc.add_text(f.content, &doc.content);

        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.add_document(tantivy_doc)?;
        Ok(())
    }

    /// Add a unified `SearchDocument` to the index.
    ///
    /// Writes the **union** of all fields covered by `add_symbol_with_context`
    /// and `add_file_content` so that existing search paths continue to find
    /// documents written via this method after T4 cuts projection over.
    ///
    /// Does NOT call `commit`; callers are responsible for batching.
    pub fn add_search_doc(&self, doc: &SearchDocument) -> Result<()> {
        let f = &self.schema_fields;
        let mut tantivy_doc = TantivyDocument::new();

        // ---- discriminator ----
        tantivy_doc.add_text(f.doc_type, &doc.doc_type);

        // ---- shared fields ----
        tantivy_doc.add_text(f.id, &doc.id);
        tantivy_doc.add_text(f.file_path, &doc.file_path);
        tantivy_doc.add_text(f.basename, &doc.basename);
        tantivy_doc.add_text(f.language, &doc.language);
        tantivy_doc.add_text(f.kind, &doc.kind);
        tantivy_doc.add_text(f.role, &doc.role);
        tantivy_doc.add_text(f.test_role, &doc.test_role);

        // ---- symbol fields ----
        tantivy_doc.add_text(f.name, &doc.name);
        tantivy_doc.add_text(f.signature, &doc.signature);
        tantivy_doc.add_text(f.doc_comment, &doc.doc_comment);
        tantivy_doc.add_text(f.code_body, &doc.code_body);
        for key in &doc.annotation_keys {
            let key = key.trim().to_ascii_lowercase();
            if !key.is_empty() {
                tantivy_doc.add_text(f.annotations_exact, &key);
            }
        }
        tantivy_doc.add_text(f.annotations_text, &doc.annotations_text);
        tantivy_doc.add_text(f.owner_names_text, &doc.owner_names_text);
        tantivy_doc.add_u64(f.start_line, doc.start_line as u64);

        // ---- file fields ----
        tantivy_doc.add_text(f.path_text, &doc.path_text);
        tantivy_doc.add_text(f.content, &doc.content);

        // ---- Phase 2 fields (empty in T2; wired by T4 / T7) ----
        tantivy_doc.add_text(f.pretokenized_code, &doc.pretokenized_code);
        tantivy_doc.add_text(f.relationship_text, &doc.relationship_text);

        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.add_document(tantivy_doc)?;
        Ok(())
    }

    /// Commit pending changes to make them searchable.
    pub fn commit(&self) -> Result<()> {
        let mut guard = self.writer.lock().unwrap();
        if let Some(ref mut writer) = *guard {
            writer.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    /// Commit and release the current writer without shutting down search.
    ///
    /// This keeps the reader usable and lets future writes recreate the writer,
    /// while releasing Tantivy's process-wide write lock for path-backed callers.
    pub fn release_writer(&self) -> Result<()> {
        let mut guard = self.writer.lock().unwrap_or_else(|e| {
            tracing::warn!("writer mutex was poisoned during writer release; recovering");
            e.into_inner()
        });
        if let Some(mut writer) = guard.take() {
            writer.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    /// Remove all documents from the index (for force re-index).
    pub fn clear_all(&self) -> Result<()> {
        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.delete_all_documents()?;
        drop(guard);
        self.commit()?;
        Ok(())
    }

    /// Remove all documents (both symbols and file content) for a given file path.
    pub fn remove_by_file_path(&self, path: &str) -> Result<()> {
        let term = Term::from_field_text(self.schema_fields.file_path, path);
        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.delete_term(term);
        Ok(())
    }

    /// Search for symbols matching the query.
    ///
    /// Uses field boosting: name (5x) > signature (3x) > doc_comment (2x) > code_body (1x).
    /// Automatically falls back to OR matching when AND returns zero results and the
    /// query contains multiple terms.
    pub fn search_symbols(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        let f = &self.schema_fields;

        let parsed_annotation_query = parse_annotation_query(query_str);
        let has_annotation_filters = parsed_annotation_query.has_annotation_filters();
        let term_query = if has_annotation_filters {
            parsed_annotation_query.remaining_query.as_str()
        } else {
            query_str
        };
        let expanded = expand_query_terms(term_query);
        let original_terms = if has_annotation_filters {
            self.annotation_context_terms(term_query)
        } else {
            Self::filter_compound_tokens(self.tokenize_terms(&expanded.original_terms))
        };
        let alias_terms = Self::filter_compound_tokens(self.tokenize_terms(&expanded.alias_terms));
        let normalized_terms =
            Self::filter_compound_tokens(self.tokenize_terms(&expanded.normalized_terms));

        if original_terms.is_empty() && !has_annotation_filters {
            return Ok(SymbolSearchResults {
                results: Vec::new(),
                relaxed: false,
            });
        }

        let query = if has_annotation_filters {
            build_annotation_symbol_query(
                &original_terms,
                &alias_terms,
                &normalized_terms,
                &parsed_annotation_query.annotation_keys,
                f,
                filter,
                true,
            )
        } else {
            build_symbol_query_weighted(
                &original_terms,
                &alias_terms,
                &normalized_terms,
                f.name,
                f.signature,
                f.doc_comment,
                f.code_body,
                f.doc_type,
                f.language,
                f.kind,
                filter.language.as_deref(),
                filter.kind.as_deref(),
                true, // require_all_terms: AND mode (strict matching)
            )
        };

        let searcher = self.reader.searcher();
        let candidate_limit = Self::symbol_candidate_limit(query_str, filter, limit);
        let top_docs = searcher.search(
            &query,
            &TopDocs::with_limit(candidate_limit).order_by_score(),
        )?;

        // Auto-fallback: if AND returned nothing and the user typed multiple words, try OR.
        // Use word count from query_str (not terms.len()) because the tokenizer can inflate
        // a single word into multiple tokens via CamelCase splitting, stemming, etc.
        let user_word_count = term_query.split_whitespace().count();
        let (top_docs, relaxed) = if top_docs.is_empty() && user_word_count > 1 {
            let or_query = if has_annotation_filters {
                build_annotation_symbol_query(
                    &original_terms,
                    &alias_terms,
                    &normalized_terms,
                    &parsed_annotation_query.annotation_keys,
                    f,
                    filter,
                    false,
                )
            } else {
                build_symbol_query_weighted(
                    &original_terms,
                    &alias_terms,
                    &normalized_terms,
                    f.name,
                    f.signature,
                    f.doc_comment,
                    f.code_body,
                    f.doc_type,
                    f.language,
                    f.kind,
                    filter.language.as_deref(),
                    filter.kind.as_deref(),
                    false, // OR mode
                )
            };
            (
                searcher.search(
                    &or_query,
                    &TopDocs::with_limit(candidate_limit).order_by_score(),
                )?,
                true,
            )
        } else {
            (top_docs, false)
        };

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            results.push(SymbolSearchResult {
                id: Self::get_text_field(&doc, f.id),
                name: Self::get_text_field(&doc, f.name),
                signature: Self::get_text_field(&doc, f.signature),
                doc_comment: Self::get_text_field(&doc, f.doc_comment),
                file_path: Self::get_text_field(&doc, f.file_path),
                kind: Self::get_text_field(&doc, f.kind),
                language: Self::get_text_field(&doc, f.language),
                start_line: Self::get_u64_field(&doc, f.start_line) as u32,
                score,
                role: Self::get_text_field(&doc, f.role),
                test_role: Self::get_text_field(&doc, f.test_role),
            });
        }

        results.retain(|result| symbol_result_matches_filter(result, filter));

        // Apply important_patterns boost if language configs are available
        if let Some(configs) = &self.language_configs {
            apply_important_patterns_boost(&mut results, configs);
        }
        // NL path prior is owned by the assembly layer
        // (`text_search::definition_search_with_index`) so the multiplier is
        // applied exactly once across the pipeline. Do not apply it here.
        if results.len() > limit {
            results.truncate(limit);
        }

        Ok(SymbolSearchResults { results, relaxed })
    }

    /// Search for symbols using OR-mode (relaxed) matching.
    ///
    /// Unlike `search_symbols` which requires ALL query terms to be present (AND),
    /// this method uses OR-per-term so that symbols matching SOME terms are returned.
    /// BM25 naturally ranks symbols matching more terms higher.
    pub fn search_symbols_relaxed(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        let f = &self.schema_fields;

        let parsed_annotation_query = parse_annotation_query(query_str);
        let has_annotation_filters = parsed_annotation_query.has_annotation_filters();
        let term_query = if has_annotation_filters {
            parsed_annotation_query.remaining_query.as_str()
        } else {
            query_str
        };

        let terms = Self::filter_compound_tokens(self.tokenize_query(term_query));
        if terms.is_empty() && !has_annotation_filters {
            return Ok(SymbolSearchResults {
                results: Vec::new(),
                relaxed: true,
            });
        }

        let query = if has_annotation_filters {
            build_annotation_symbol_query(
                &terms,
                &[],
                &[],
                &parsed_annotation_query.annotation_keys,
                f,
                filter,
                false,
            )
        } else {
            build_symbol_query(
                &terms,
                f.name,
                f.signature,
                f.doc_comment,
                f.code_body,
                f.doc_type,
                f.language,
                f.kind,
                filter.language.as_deref(),
                filter.kind.as_deref(),
                false, // require_all_terms: OR mode (relaxed matching)
            )
        };

        let searcher = self.reader.searcher();
        let candidate_limit = Self::symbol_candidate_limit(query_str, filter, limit);
        let top_docs = searcher.search(
            &query,
            &TopDocs::with_limit(candidate_limit).order_by_score(),
        )?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            results.push(SymbolSearchResult {
                id: Self::get_text_field(&doc, f.id),
                name: Self::get_text_field(&doc, f.name),
                signature: Self::get_text_field(&doc, f.signature),
                doc_comment: Self::get_text_field(&doc, f.doc_comment),
                file_path: Self::get_text_field(&doc, f.file_path),
                kind: Self::get_text_field(&doc, f.kind),
                language: Self::get_text_field(&doc, f.language),
                start_line: Self::get_u64_field(&doc, f.start_line) as u32,
                score,
                role: Self::get_text_field(&doc, f.role),
                test_role: Self::get_text_field(&doc, f.test_role),
            });
        }

        results.retain(|result| symbol_result_matches_filter(result, filter));

        // Apply important_patterns boost if language configs are available
        if let Some(configs) = &self.language_configs {
            apply_important_patterns_boost(&mut results, configs);
        }
        // NL path prior is owned by the assembly layer
        // (`text_search::definition_search_with_index`); not applied here.
        if results.len() > limit {
            results.truncate(limit);
        }

        Ok(SymbolSearchResults {
            results,
            relaxed: true,
        })
    }

    /// Search for file content matching the query.
    ///
    /// Uses AND mode first (all terms must match), then auto-falls back to OR
    /// mode if AND returns zero results and there are multiple terms. The
    /// `relaxed` flag in the returned `ContentSearchResults` indicates whether
    /// OR fallback was used.
    pub fn search_content(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<ContentSearchResults> {
        let f = &self.schema_fields;

        let expanded = expand_query_terms(query_str);
        let original_terms = self.tokenize_terms(&expanded.original_terms);
        let alias_terms = self.tokenize_terms(&expanded.alias_terms);
        let normalized_terms = self.tokenize_terms(&expanded.normalized_terms);

        if original_terms.is_empty() {
            return Ok(ContentSearchResults {
                results: Vec::new(),
                relaxed: false,
                and_candidate_count: 0,
                or_candidate_count: 0,
            });
        }

        let query = build_content_query_weighted(
            &original_terms,
            &alias_terms,
            &normalized_terms,
            f.content,
            f.doc_type,
            f.language,
            filter.language.as_deref(),
            true, // require_all_terms: AND mode (strict matching)
        );

        let searcher = self.reader.searcher();
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit).order_by_score())?;
        let and_candidate_count = top_docs.len();

        // Auto-fallback: if AND returned nothing and the user typed multiple words, try OR.
        // Use word count from query_str (not terms.len()) because the tokenizer can inflate
        // a single word into multiple tokens via CamelCase splitting, stemming, etc.
        let user_word_count = query_str.split_whitespace().count();
        let (top_docs, relaxed, or_candidate_count) = if top_docs.is_empty() && user_word_count > 1
        {
            let or_query = build_content_query_weighted(
                &original_terms,
                &alias_terms,
                &normalized_terms,
                f.content,
                f.doc_type,
                f.language,
                filter.language.as_deref(),
                false, // require_all_terms: OR mode (relaxed matching)
            );
            let or_top_docs =
                searcher.search(&or_query, &TopDocs::with_limit(limit).order_by_score())?;
            let or_count = or_top_docs.len();
            (or_top_docs, true, or_count)
        } else {
            (top_docs, false, 0)
        };

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            results.push(ContentSearchResult {
                file_path: Self::get_text_field(&doc, f.file_path),
                language: Self::get_text_field(&doc, f.language),
                score,
            });
        }

        Ok(ContentSearchResults {
            results,
            relaxed,
            and_candidate_count,
            or_candidate_count,
        })
    }

    pub fn search_files(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<FileSearchResults> {
        if limit == 0 {
            return Ok(FileSearchResults {
                results: Vec::new(),
                relaxed: false,
            });
        }

        let f = &self.schema_fields;
        let normalized_query = normalize_file_path(query_str.trim());
        let glob_matcher = compile_query_glob(&normalized_query)?;
        let is_glob_query = glob_matcher.is_some();
        let exact_path = (!is_glob_query).then_some(normalized_query.as_str());
        let exact_basename =
            (!normalized_query.is_empty()).then_some(basename_for_path(&normalized_query));

        let literal_query = if is_glob_query {
            extract_glob_literals(&normalized_query).join(" ")
        } else {
            normalized_query.clone()
        };
        let expanded = expand_query_terms(&literal_query);
        let mut path_terms =
            Self::filter_compound_tokens(self.tokenize_terms(&expanded.original_terms));
        if path_terms.is_empty() {
            path_terms = Self::filter_compound_tokens(self.tokenize_query(&literal_query));
        }

        if path_terms.is_empty() && exact_path.is_none() && exact_basename.is_none() {
            return Ok(FileSearchResults {
                results: Vec::new(),
                relaxed: false,
            });
        }

        let searcher = self.reader.searcher();
        let candidate_limit = Self::file_candidate_limit(query_str, limit);
        let and_query = build_file_query(
            &path_terms,
            f.file_path,
            f.basename,
            f.path_text,
            f.doc_type,
            f.language,
            filter.language.as_deref(),
            exact_path,
            exact_basename,
            true,
        );
        let top_docs = searcher.search(
            &and_query,
            &TopDocs::with_limit(candidate_limit).order_by_score(),
        )?;

        let (top_docs, relaxed) = if top_docs.is_empty() && path_terms.len() > 1 {
            let or_query = build_file_query(
                &path_terms,
                f.file_path,
                f.basename,
                f.path_text,
                f.doc_type,
                f.language,
                filter.language.as_deref(),
                exact_path,
                exact_basename,
                false,
            );
            (
                searcher.search(
                    &or_query,
                    &TopDocs::with_limit(candidate_limit).order_by_score(),
                )?,
                true,
            )
        } else {
            (top_docs, false)
        };

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            let file_path = Self::get_text_field(&doc, f.file_path);
            if let Some(glob_matcher) = &glob_matcher
                && !glob_matcher.is_match(&file_path)
            {
                continue;
            }

            results.push(FileSearchResult {
                language: Self::get_text_field(&doc, f.language),
                match_kind: classify_file_match(query_str, &normalized_query, &file_path),
                file_path,
                score,
            });
        }

        results.sort_by(|left, right| {
            file_search_rank(&normalized_query, left)
                .cmp(&file_search_rank(&normalized_query, right))
                .then_with(|| right.score.total_cmp(&left.score))
                .then_with(|| left.file_path.cmp(&right.file_path))
        });
        if results.len() > limit {
            results.truncate(limit);
        }

        Ok(FileSearchResults { results, relaxed })
    }

    // --- Phase 2 — unified search ---

    /// Single BM25 sweep across all seven core FTS fields, returning mixed-kind
    /// [`UnifiedHit`]s.  No `doc_type` filter — symbol rows, file rows, and any
    /// other document type are all eligible.
    ///
    /// Over-fetches by `NL_RERANK_OVERFETCH_FACTOR` before applying post-filters
    /// and reranking (via `rerank_symbol_score` placeholder; T6 replaces this).
    /// Falls back to OR mode when AND returns zero results on a multi-term query.
    pub fn search_unified(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<Vec<UnifiedHit>> {
        use crate::search::query_parse::parse_query;
        use crate::search::reranker::{Candidate, rerank_unified};
        use crate::extractors::SymbolKind;
        use crate::search::scoring::{classify_role, test_subrole, DOC_LANGUAGES};

        let f = &self.schema_fields;

        let expanded = expand_query_terms(query_str);
        let original_terms =
            Self::filter_compound_tokens(self.tokenize_terms(&expanded.original_terms));
        let alias_terms =
            Self::filter_compound_tokens(self.tokenize_terms(&expanded.alias_terms));
        let normalized_terms =
            Self::filter_compound_tokens(self.tokenize_terms(&expanded.normalized_terms));

        if original_terms.is_empty() {
            return Ok(Vec::new());
        }

        let candidate_limit = limit * NL_RERANK_OVERFETCH_FACTOR;

        let and_query = build_unified_query(
            &original_terms,
            &alias_terms,
            &normalized_terms,
            f.name,
            f.path_text,
            f.signature,
            f.doc_comment,
            f.relationship_text,
            f.code_body,
            f.pretokenized_code,
            true, // require_all_terms — AND mode
        );

        let searcher = self.reader.searcher();
        let top_docs = searcher.search(
            &and_query,
            &TopDocs::with_limit(candidate_limit).order_by_score(),
        )?;

        // Auto-fallback to OR when AND returns nothing and query has >1 word.
        let user_word_count = query_str.split_whitespace().count();
        let top_docs = if top_docs.is_empty() && user_word_count > 1 {
            let or_query = build_unified_query(
                &original_terms,
                &alias_terms,
                &normalized_terms,
                f.name,
                f.path_text,
                f.signature,
                f.doc_comment,
                f.relationship_text,
                f.code_body,
                f.pretokenized_code,
                false, // OR mode
            );
            searcher.search(
                &or_query,
                &TopDocs::with_limit(candidate_limit).order_by_score(),
            )?
        } else {
            top_docs
        };

        // Materialize hits.
        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            hits.push(UnifiedHit {
                id: Self::get_text_field(&doc, f.id),
                kind: Self::get_text_field(&doc, f.kind),
                name: Self::get_text_field(&doc, f.name),
                path_text: Self::get_text_field(&doc, f.path_text),
                file_path: Self::get_text_field(&doc, f.file_path),
                basename: Self::get_text_field(&doc, f.basename),
                signature: Self::get_text_field(&doc, f.signature),
                doc_comment: Self::get_text_field(&doc, f.doc_comment),
                code_body: Self::get_text_field(&doc, f.code_body),
                pretokenized_code: Self::get_text_field(&doc, f.pretokenized_code),
                relationship_text: Self::get_text_field(&doc, f.relationship_text),
                language: Self::get_text_field(&doc, f.language),
                start_line: Self::get_u64_field(&doc, f.start_line) as u32,
                role: Self::get_text_field(&doc, f.role),
                test_role: Self::get_text_field(&doc, f.test_role),
                tantivy_score: score,
            });
        }

        // Post-fetch filters (language / kind / file_pattern / exclude_tests).
        if let Some(ref lang) = filter.language {
            hits.retain(|h| &h.language == lang);
        }
        if let Some(ref kind) = filter.kind {
            hits.retain(|h| &h.kind == kind);
        }
        if let Some(ref pattern) = filter.file_pattern {
            hits.retain(|h| {
                crate::tools::search::matches_glob_pattern(&h.file_path, pattern)
            });
        }
        if filter.exclude_tests {
            hits.retain(|h| !is_test_path(&h.file_path));
        }

        // T6 unified reranking — builds Candidate structs for every hit and
        // delegates to `rerank_unified` which handles both symbol rows
        // (`is_file_doc == false`) and file rows (`is_file_doc == true`) in a
        // single pass with Eros-recipe field-score boosts.
        if !hits.is_empty() {
            let parsed = parse_query(query_str);
            let candidates: Vec<Candidate> = hits
                .iter()
                .map(|hit| {
                    let kind =
                        SymbolKind::try_from_string(&hit.kind).unwrap_or(SymbolKind::Variable);
                    let role = if hit.role.is_empty() {
                        classify_role(&hit.file_path, &hit.language).to_string()
                    } else {
                        hit.role.clone()
                    };
                    let test_role = if hit.test_role.is_empty() {
                        test_subrole(&hit.file_path).to_string()
                    } else {
                        hit.test_role.clone()
                    };
                    let is_test = role == "test";
                    // is_file_doc == true for file rows (kind field == "file"),
                    // not just for doc-role rows. The kind field is the
                    // authoritative discriminator.
                    let is_file_doc = hit.kind == "file";
                    let is_source_language =
                        !DOC_LANGUAGES.contains(&hit.language.as_str());

                    let mut body = String::with_capacity(
                        hit.signature.len() + hit.doc_comment.len() + 1,
                    );
                    body.push_str(&hit.signature);
                    if !hit.signature.is_empty() && !hit.doc_comment.is_empty() {
                        body.push(' ');
                    }
                    body.push_str(&hit.doc_comment);

                    Candidate::builder()
                        .title(hit.name.clone())
                        .path(hit.file_path.clone())
                        .body(body)
                        .kind(kind)
                        .role(role)
                        .test_role(test_role)
                        .is_test(is_test)
                        .is_file_doc(is_file_doc)
                        .is_source_language(is_source_language)
                        .tantivy_score(hit.tantivy_score)
                        .build()
                })
                .collect();

            // rerank_unified returns sorted output; we need to write scores
            // back in original `hits` order (hits[i] ↔ candidates[i]).
            // Build an index: position in candidates → final_score.
            // Since `rerank_unified` clones candidates into Ranked, we match
            // by enumerating the original candidates and finding the
            // corresponding Ranked entry by (path, title) key.
            let ranked = rerank_unified(&parsed, &candidates);

            // Build a score lookup: (file_path, name) → final_score.
            // Duplicates are resolved by taking the highest score (the
            // reranker returns them sorted, so the first entry wins).
            let mut score_map: std::collections::HashMap<(&str, &str), f32> =
                std::collections::HashMap::with_capacity(ranked.len());
            for r in &ranked {
                let key = (r.candidate.path.as_str(), r.candidate.title.as_str());
                score_map.entry(key).or_insert(r.final_score);
            }

            for hit in hits.iter_mut() {
                let key = (hit.file_path.as_str(), hit.name.as_str());
                if let Some(&s) = score_map.get(&key) {
                    hit.tantivy_score = s;
                }
            }

            hits.sort_by(|a, b| {
                b.tantivy_score
                    .partial_cmp(&a.tantivy_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.file_path.cmp(&b.file_path))
            });
        }

        hits.truncate(limit);
        Ok(hits)
    }

    // --- Private helpers ---

    fn open_or_create_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<SearchIndexOpenOutcome> {
        let expected_schema = create_schema();
        let expected_marker = Self::expected_compat_marker(&expected_schema, &tokenizer);

        let (index, disposition) = if path.join("meta.json").exists() {
            match Index::open_in_dir(path) {
                Ok(existing) => {
                    if Self::index_is_compatible(
                        path,
                        &expected_schema,
                        &existing.schema(),
                        &expected_marker,
                    ) {
                        (existing, SearchIndexOpenDisposition::Compatible)
                    } else {
                        tracing::warn!(
                            "Tantivy index at {} is incompatible with Julie expectations, recreating empty index",
                            path.display()
                        );
                        drop(existing);
                        (
                            Self::recreate_index_with_lock(
                                path,
                                &expected_schema,
                                &expected_marker,
                            )?,
                            SearchIndexOpenDisposition::RecreatedIncompatible,
                        )
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "Failed to open Tantivy index at {} ({err}), recreating empty index",
                        path.display()
                    );
                    (
                        Self::recreate_index_with_lock(path, &expected_schema, &expected_marker)?,
                        SearchIndexOpenDisposition::RecreatedOpenFailure,
                    )
                }
            }
        } else {
            let index = Index::builder()
                .schema(expected_schema.clone())
                .create_in_dir(path)?;
            Self::write_compat_marker(path, &expected_marker)?;
            (index, SearchIndexOpenDisposition::Compatible)
        };

        let search_index =
            Self::build_search_index(index, &expected_schema, tokenizer, language_configs)?;

        Ok(SearchIndexOpenOutcome {
            index: search_index,
            disposition,
        })
    }

    fn create_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<Self> {
        let schema = create_schema();
        let expected_marker = Self::expected_compat_marker(&schema, &tokenizer);
        let index = Index::create_in_dir(path, schema.clone())?;
        Self::write_compat_marker(path, &expected_marker)?;
        Self::build_search_index(index, &schema, tokenizer, language_configs)
    }

    fn open_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<SearchIndexOpenOutcome> {
        let expected_schema = create_schema();
        let expected_marker = Self::expected_compat_marker(&expected_schema, &tokenizer);

        let (index, disposition) = match Index::open_in_dir(path) {
            Ok(index) => {
                if Self::index_is_compatible(
                    path,
                    &expected_schema,
                    &index.schema(),
                    &expected_marker,
                ) {
                    (index, SearchIndexOpenDisposition::Compatible)
                } else {
                    tracing::warn!(
                        "Tantivy index at {} is incompatible with Julie expectations, recreating empty index",
                        path.display()
                    );
                    drop(index);
                    (
                        Self::recreate_index(path, &expected_schema, &expected_marker)?,
                        SearchIndexOpenDisposition::RecreatedIncompatible,
                    )
                }
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to open Tantivy index at {} ({err}), recreating empty index",
                    path.display()
                );
                (
                    Self::recreate_index(path, &expected_schema, &expected_marker)?,
                    SearchIndexOpenDisposition::RecreatedOpenFailure,
                )
            }
        };

        let search_index =
            Self::build_search_index(index, &expected_schema, tokenizer, language_configs)?;
        Ok(SearchIndexOpenOutcome {
            index: search_index,
            disposition,
        })
    }

    fn register_tokenizer(index: &Index, tokenizer: CodeTokenizer) {
        index
            .tokenizers()
            .register("code", TextAnalyzer::builder(tokenizer).build());
        // Register the simple tokenizer for the pretokenized_code field (T3 wiring;
        // schema fields are retargeted to "simple_code" at T4/T5).
        index
            .tokenizers()
            .register("simple_code", TextAnalyzer::builder(SimpleCodeTokenizer::new()).build());
    }

    fn build_search_index(
        index: Index,
        schema: &tantivy::schema::Schema,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<Self> {
        let schema_fields = SchemaFields::new(schema);
        Self::register_tokenizer(&index, tokenizer);
        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(None),
            schema_fields,
            language_configs,
            shutdown: AtomicBool::new(false),
        })
    }

    fn expected_compat_marker(
        schema: &tantivy::schema::Schema,
        tokenizer: &CodeTokenizer,
    ) -> SearchCompatMarker {
        SearchCompatMarker {
            marker_version: SEARCH_COMPAT_MARKER_VERSION,
            schema_signature: compatibility_signature(schema),
            tokenizer_signature: tokenizer.compatibility_signature(),
        }
    }

    fn read_compat_marker(path: &Path) -> std::result::Result<Option<SearchCompatMarker>, String> {
        let marker_path = path.join(SEARCH_COMPAT_MARKER_FILE);
        if !marker_path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(&marker_path)
            .map_err(|err| format!("failed to read {}: {err}", marker_path.display()))?;
        let marker = serde_json::from_str::<SearchCompatMarker>(&raw)
            .map_err(|err| format!("failed to parse {}: {err}", marker_path.display()))?;

        Ok(Some(marker))
    }

    fn write_compat_marker(path: &Path, marker: &SearchCompatMarker) -> Result<()> {
        let marker_path = path.join(SEARCH_COMPAT_MARKER_FILE);
        let payload = serde_json::to_string_pretty(marker).map_err(|err| {
            SearchError::IndexError(format!(
                "failed to serialize compatibility marker for {}: {err}",
                marker_path.display()
            ))
        })?;
        std::fs::write(marker_path, payload)?;
        Ok(())
    }

    fn index_is_compatible(
        path: &Path,
        expected_schema: &tantivy::schema::Schema,
        actual_schema: &tantivy::schema::Schema,
        expected_marker: &SearchCompatMarker,
    ) -> bool {
        if !Self::schema_is_compatible(expected_schema, actual_schema) {
            return false;
        }

        match Self::read_compat_marker(path) {
            Ok(Some(marker)) => {
                if marker == *expected_marker {
                    true
                } else {
                    tracing::warn!(
                        "Compatibility marker mismatch at {} (expected Julie marker v{}, found v{}), recreating",
                        path.display(),
                        SEARCH_COMPAT_MARKER_VERSION,
                        marker.marker_version
                    );
                    false
                }
            }
            Ok(None) => {
                tracing::warn!(
                    "Compatibility marker missing at {} ({}), recreating",
                    path.display(),
                    SEARCH_COMPAT_MARKER_FILE
                );
                false
            }
            Err(err) => {
                tracing::warn!(
                    "Compatibility marker unreadable at {} ({}), recreating",
                    path.display(),
                    err
                );
                false
            }
        }
    }

    fn recreate_index(
        path: &Path,
        schema: &tantivy::schema::Schema,
        marker: &SearchCompatMarker,
    ) -> Result<Index> {
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }
        std::fs::create_dir_all(path)?;
        let index = Index::create_in_dir(path, schema.clone())?;
        Self::write_compat_marker(path, marker)?;
        Ok(index)
    }

    fn recreate_index_with_lock(
        path: &Path,
        schema: &tantivy::schema::Schema,
        marker: &SearchCompatMarker,
    ) -> Result<Index> {
        let lock_path = path.join(".recreating");
        let _lock = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                tracing::warn!(
                    "Concurrent index recreation detected at {} — reusing existing index",
                    path.display()
                );
                let existing = Index::open_in_dir(path)?;
                if Self::index_is_compatible(path, schema, &existing.schema(), marker) {
                    return Ok(existing);
                }

                tracing::warn!(
                    "Concurrent recreation at {} yielded an incompatible index, forcing local recreation",
                    path.display()
                );
                drop(existing);
                return Self::recreate_index(path, schema, marker);
            }
            Err(err) => return Err(err.into()),
        };

        let recreate_result = Self::recreate_index(path, schema, marker);
        let _ = std::fs::remove_file(&lock_path);
        recreate_result
    }

    /// Check whether on-disk schema metadata matches Julie's expected schema shape.
    fn schema_is_compatible(
        expected: &tantivy::schema::Schema,
        actual: &tantivy::schema::Schema,
    ) -> bool {
        compatibility_signature(expected) == compatibility_signature(actual)
    }

    fn get_or_create_writer(&self) -> Result<std::sync::MutexGuard<'_, Option<IndexWriter>>> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err(SearchError::Shutdown);
        }
        let mut guard = self.writer.lock().unwrap_or_else(|e| {
            tracing::warn!("writer mutex was poisoned (a previous writer panicked); recovering");
            e.into_inner()
        });
        if guard.is_none() {
            // Double-check after acquiring mutex: shutdown() may have run between
            // the flag check above and the mutex acquisition, dropping the writer.
            // Without this, a watcher task can re-create the writer after shutdown
            // released it, causing LockBusy for the next IndexWriter.
            if self.shutdown.load(Ordering::Acquire) {
                return Err(SearchError::Shutdown);
            }
            *guard = Some(self.index.writer(WRITER_HEAP_SIZE)?);
        }
        Ok(guard)
    }

    /// Gracefully shut down this index: commit pending writes, release the
    /// Tantivy file lock, and prevent any future writes.
    ///
    /// After shutdown, `get_or_create_writer()` returns `Err(Shutdown)`.
    /// Reads (search) continue to work — the `IndexReader` is independent.
    pub fn shutdown(&self) -> Result<()> {
        self.shutdown.store(true, Ordering::Release);

        let mut guard = self.writer.lock().unwrap_or_else(|e| {
            tracing::warn!("writer mutex was poisoned during shutdown; recovering");
            e.into_inner()
        });
        if let Some(mut writer) = guard.take() {
            // Best-effort commit — if it fails, we still drop the writer to release the lock
            let _ = writer.commit();
            // writer is dropped here, releasing the Tantivy file lock
        }
        Ok(())
    }

    /// Returns true if this index has been shut down.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Tokenize a query string using the registered code tokenizer.
    fn tokenize_query(&self, query_str: &str) -> Vec<String> {
        use std::collections::HashSet;

        let mut tokenizer = self
            .index
            .tokenizers()
            .get("code")
            .expect("code tokenizer not registered");

        let mut stream = tokenizer.token_stream(query_str);
        let mut terms = Vec::new();
        let mut seen = HashSet::new();
        while stream.advance() {
            let token = stream.token().text.clone();
            if seen.insert(token.clone()) {
                terms.push(token);
            }
        }
        terms
    }

    /// Public wrapper around `tokenize_query` for the debug search module.
    ///
    /// Shows how the CodeTokenizer splits a query string into individual
    /// search terms (CamelCase splitting, snake_case splitting, stemming, etc.).
    pub fn tokenize_query_public(&self, query_str: &str) -> Vec<String> {
        self.tokenize_query(query_str)
    }

    fn tokenize_terms(&self, terms: &[String]) -> Vec<String> {
        use std::collections::HashSet;

        let mut tokenized_terms = Vec::new();
        let mut seen = HashSet::new();
        for term in terms {
            for token in self.tokenize_query(term) {
                if seen.insert(token.clone()) {
                    tokenized_terms.push(token);
                }
            }
        }
        tokenized_terms
    }

    fn annotation_context_terms(&self, query: &str) -> Vec<String> {
        use std::collections::HashSet;

        let terms = query
            .split_whitespace()
            .map(|term| {
                term.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
            })
            .filter(|term| !term.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let tokenized_terms = self.tokenize_terms(&terms);
        let token_set: HashSet<String> = tokenized_terms.iter().cloned().collect();
        let mut compound_tokens_to_drop = HashSet::new();

        for term in &terms {
            let camel_parts = split_camel_case(term);
            if camel_parts.len() <= 1 {
                continue;
            }

            let part_tokens = camel_parts
                .iter()
                .flat_map(|part| self.tokenize_query(part))
                .collect::<HashSet<_>>();
            if part_tokens.is_empty() || !part_tokens.iter().all(|part| token_set.contains(part)) {
                continue;
            }

            let term_lower = term.to_ascii_lowercase();
            if token_set.contains(&term_lower) {
                compound_tokens_to_drop.insert(term_lower);
            }
        }

        Self::filter_compound_tokens(
            tokenized_terms
                .into_iter()
                .filter(|token| !compound_tokens_to_drop.contains(token))
                .collect(),
        )
    }

    fn rerank_candidate_limit(query_str: &str, limit: usize) -> usize {
        if limit == 0 || !is_nl_like_query(query_str) {
            return limit;
        }

        limit.saturating_mul(NL_RERANK_OVERFETCH_FACTOR)
    }

    fn symbol_candidate_limit(query_str: &str, filter: &SearchFilter, limit: usize) -> usize {
        let rerank_limit = Self::rerank_candidate_limit(query_str, limit);
        if limit == 0 || (filter.file_pattern.is_none() && !filter.exclude_tests) {
            return rerank_limit;
        }

        let filtered_limit = limit.saturating_mul(20).clamp(50, 1000);
        rerank_limit.max(filtered_limit)
    }

    fn file_candidate_limit(query_str: &str, limit: usize) -> usize {
        if limit == 0 {
            return 0;
        }

        let factor = if query_contains_glob_syntax(query_str) {
            50
        } else {
            20
        };
        limit.saturating_mul(factor).clamp(50, 1000)
    }

    /// Remove compound tokens whose snake_case sub-parts are all present in the list.
    ///
    /// The CodeTokenizer emits the full form plus atomic sub-parts, but never
    /// partial compounds. For example, `search_term_one` produces tokens
    /// `[search_term_one, search, term, one]` — there is no `search_term` token.
    ///
    /// When a query like `"search_term"` tokenizes to `[search_term, search, term]`,
    /// requiring ALL tokens via AND would fail because `search_term` doesn't exist
    /// in documents indexed as `search_term_one`. By filtering out `search_term`
    /// (whose parts `search` and `term` are already present), we get clean AND
    /// semantics on just the atomic parts.
    fn filter_compound_tokens(tokens: Vec<String>) -> Vec<String> {
        use std::collections::HashSet;
        let token_set: HashSet<String> = tokens.iter().cloned().collect();
        tokens
            .into_iter()
            .filter(|token| {
                let parts: Vec<&str> = token.split('_').collect();
                if parts.len() <= 1 {
                    return true; // Not a snake_case compound, keep it
                }
                // Keep if any sub-part is missing from the token set
                !parts
                    .iter()
                    .all(|part| !part.is_empty() && token_set.contains(*part))
            })
            .collect()
    }

    fn get_text_field(doc: &TantivyDocument, field: tantivy::schema::Field) -> String {
        doc.get_first(field)
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_default()
    }

    fn get_u64_field(doc: &TantivyDocument, field: tantivy::schema::Field) -> u64 {
        doc.get_first(field)
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    }
}

fn build_annotation_symbol_query(
    original_terms: &[String],
    alias_terms: &[String],
    normalized_terms: &[String],
    annotation_keys: &[String],
    f: &SchemaFields,
    filter: &SearchFilter,
    require_all_terms: bool,
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    let type_term = Term::from_field_text(f.doc_type, "symbol");
    subqueries.push((
        Occur::Must,
        Box::new(TermQuery::new(type_term, IndexRecordOption::Basic)),
    ));

    if let Some(language) = filter.language.as_deref() {
        let lang_term = Term::from_field_text(f.language, language);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(lang_term, IndexRecordOption::Basic)),
        ));
    }
    if let Some(kind) = filter.kind.as_deref() {
        let kind_term = Term::from_field_text(f.kind, kind);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(kind_term, IndexRecordOption::Basic)),
        ));
    }
    for key in annotation_keys {
        let key = key.trim().to_ascii_lowercase();
        if !key.is_empty() {
            let annotation_term = Term::from_field_text(f.annotations_exact, &key);
            subqueries.push((
                Occur::Must,
                Box::new(TermQuery::new(annotation_term, IndexRecordOption::Basic)),
            ));
        }
    }

    let mut term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
    let grouped_terms = [
        (original_terms, ANNOTATION_ORIGINAL_GROUP_WEIGHT, true),
        (alias_terms, ANNOTATION_ALIAS_GROUP_WEIGHT, false),
        (normalized_terms, ANNOTATION_NORMALIZED_GROUP_WEIGHT, false),
    ];

    for (terms, group_weight, is_original_group) in grouped_terms {
        let group_factor = group_weight / ANNOTATION_ORIGINAL_GROUP_WEIGHT;
        let mut group_term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

        for term in terms {
            let term_lower = term.to_lowercase();
            let mut field_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
            push_boosted_term(
                &mut field_clauses,
                f.name,
                &term_lower,
                ANNOTATION_NAME_FIELD_BOOST * group_factor,
            );
            push_boosted_term(
                &mut field_clauses,
                f.signature,
                &term_lower,
                ANNOTATION_SIGNATURE_FIELD_BOOST * group_factor,
            );
            push_boosted_term(
                &mut field_clauses,
                f.doc_comment,
                &term_lower,
                ANNOTATION_DOC_FIELD_BOOST * group_factor,
            );
            push_boosted_term(
                &mut field_clauses,
                f.code_body,
                &term_lower,
                ANNOTATION_BODY_FIELD_BOOST * group_factor,
            );
            push_boosted_term(
                &mut field_clauses,
                f.owner_names_text,
                &term_lower,
                ANNOTATION_OWNER_FIELD_BOOST * group_factor,
            );

            let term_occur = if require_all_terms && is_original_group {
                Occur::Must
            } else {
                Occur::Should
            };
            group_term_clauses.push((term_occur, Box::new(BooleanQuery::new(field_clauses))));
        }

        if !group_term_clauses.is_empty() {
            let group_occur = if require_all_terms && is_original_group {
                Occur::Must
            } else {
                Occur::Should
            };
            term_clauses.push((group_occur, Box::new(BooleanQuery::new(group_term_clauses))));
        }
    }

    if term_clauses.is_empty() {
        return BooleanQuery::new(subqueries);
    }
    if require_all_terms {
        subqueries.extend(term_clauses);
    } else {
        subqueries.push((Occur::Must, Box::new(BooleanQuery::new(term_clauses))));
    }

    BooleanQuery::new(subqueries)
}

fn push_boosted_term(
    clauses: &mut Vec<(Occur, Box<dyn tantivy::query::Query>)>,
    field: tantivy::schema::Field,
    term: &str,
    boost: f32,
) {
    let term = Term::from_field_text(field, term);
    clauses.push((
        Occur::Should,
        Box::new(BoostQuery::new(
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            boost,
        )),
    ));
}

fn normalize_file_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn basename_for_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn query_contains_glob_syntax(query: &str) -> bool {
    query
        .chars()
        .any(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}'))
}

fn compile_query_glob(query: &str) -> Result<Option<GlobMatcher>> {
    if !query_contains_glob_syntax(query) {
        return Ok(None);
    }

    let glob = Glob::new(query)
        .map_err(|err| SearchError::QueryError(format!("Invalid file glob {query:?}: {err}")))?;
    Ok(Some(glob.compile_matcher()))
}

fn extract_glob_literals(query: &str) -> Vec<String> {
    query
        .split(|ch: char| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}' | ',' | '!'))
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn classify_file_match(
    query: &str,
    normalized_query: &str,
    file_path: &str,
) -> FileMatchKind {
    if query_contains_glob_syntax(query) {
        return FileMatchKind::Glob;
    }
    if file_path == normalized_query {
        return FileMatchKind::ExactPath;
    }

    let file_basename = basename_for_path(file_path);
    let query_basename = basename_for_path(normalized_query);

    if file_basename == query_basename {
        return FileMatchKind::ExactBasename;
    }

    // Extension-blind: strip the *last* extension from the file basename only.
    // This lets query "bar" match file "src/foo/bar.rs" as ExactBasename.
    // Only the last extension is stripped: "foo.tar.gz" → stem "foo.tar".
    // Hidden files like ".gitignore" have empty stems and must NOT match
    // an extensionless query of the suffix (query "gitignore" against file
    // ".gitignore" stays PathFragment; only ".gitignore" matches ".gitignore"
    // via the equality path above).
    if let Some((stem, _ext)) = file_basename.rsplit_once('.')
        && !stem.is_empty()
        && stem == query_basename
    {
        return FileMatchKind::ExactBasename;
    }

    FileMatchKind::PathFragment
}

fn file_match_rank(kind: FileMatchKind) -> u8 {
    match kind {
        FileMatchKind::ExactPath => 0,
        FileMatchKind::ExactBasename => 1,
        FileMatchKind::PathFragment => 2,
        FileMatchKind::Glob => 3,
    }
}

pub(crate) fn rank_file_search_result(query: &str, result: &FileSearchResult) -> u8 {
    let normalized_query = normalize_file_path(query.trim());
    file_search_rank(&normalized_query, result)
}

fn file_search_rank(normalized_query: &str, result: &FileSearchResult) -> u8 {
    if result.match_kind != FileMatchKind::ExactPath
        && hidden_directory_path_matches(normalized_query, &result.file_path)
    {
        return 1;
    }

    match result.match_kind {
        FileMatchKind::ExactPath => 0,
        _ => file_match_rank(result.match_kind) + 1,
    }
}

fn hidden_directory_path_matches(normalized_query: &str, file_path: &str) -> bool {
    let query_path = normalized_query.trim_matches('/');
    if query_path.is_empty() {
        return false;
    }

    let query_basename = basename_for_path(query_path);
    if !is_hidden_path_component(query_basename) {
        return false;
    }

    if query_path.contains('/') {
        return file_path == query_path
            || file_path
                .strip_prefix(query_path)
                .is_some_and(|suffix| suffix.starts_with('/'));
    }

    file_path
        .split('/')
        .any(|component| component == query_basename)
}

fn is_hidden_path_component(component: &str) -> bool {
    component
        .strip_prefix('.')
        .is_some_and(|suffix| !suffix.is_empty())
}

/// Normalise a name or query to its lowercase, alphanumeric-only compact form.
///
/// Strips separators (`_`, `-`, ` `, ...) and case so that `displayTemplate`,
/// `display_template`, `display-template`, and `display template` all map to
/// `displaytemplate`.  Used by the title-exact reranker for both the files
/// and content search paths to avoid the per-term matching footgun where a
/// multi-word query would boost a file whose only matching symbol is a
/// generic one-word name.
pub(crate) fn compact_alnum_lc(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Apply a symbol-title exact-match score boost to file search results.
///
/// For each result whose file contains a symbol whose lowercase name exactly
/// matches a lowercase query term, `+EXACT_TITLE_BOOST` is added to the
/// result's score.  Results are then re-sorted so the boosted files surface
/// before BM25-only matches.
///
/// This closes the Pattern-A gap on the **files** search path: a file like
/// `res.redirect.js` that defines `requestedRedirect` should rank above
/// `res.location.js` whose basename merely shares a query token, when the
/// query is `requestedRedirect`.
///
/// The DB lookup is batched (one query for all `results`) and capped at
/// `FILE_TITLE_LOOKUP_CAP` to bound cost on large result sets.
pub(crate) fn apply_symbol_title_boost_to_file_results(
    query: &str,
    results: &mut Vec<FileSearchResult>,
    db: &crate::database::SymbolDatabase,
) {
    if results.is_empty() || query.trim().is_empty() {
        return;
    }

    // Compact-form normalisation: strip non-alphanumerics and lowercase, so
    // that `displayTemplate`, `display_template`, and the multi-token query
    // `display template` all normalise to `displaytemplate` and compare equal.
    //
    // Why not per-term `t == q`?  For a compound query like `display template`,
    // per-term matching boosts any file whose only matching symbol is the
    // generic one-word `display` — completely unrelated to the compound
    // concept the user asked about.  Compact-form equality is stricter: it
    // boosts files whose symbol *spells the same concept*, regardless of
    // CamelCase / snake_case / separator differences.
    let query_compact = compact_alnum_lc(query);
    if query_compact.is_empty() {
        return;
    }

    const FILE_TITLE_LOOKUP_CAP: usize = 200;
    let paths: Vec<&str> = results
        .iter()
        .take(FILE_TITLE_LOOKUP_CAP)
        .map(|r| r.file_path.as_str())
        .collect();

    let symbol_titles = match db.titles_for_files(&paths) {
        Ok(map) => map,
        Err(_) => return,
    };

    use crate::search::reranker::EXACT_TITLE_BOOST;
    let mut any_boosted = false;
    for result in results.iter_mut().take(FILE_TITLE_LOOKUP_CAP) {
        if let Some(titles) = symbol_titles.get(&result.file_path) {
            let has_exact = titles
                .iter()
                .any(|t| compact_alnum_lc(t) == query_compact);
            if has_exact {
                result.score += EXACT_TITLE_BOOST;
                any_boosted = true;
            }
        }
    }

    if any_boosted {
        // Re-sort: rank (ExactPath/ExactBasename/PathFragment) first, then
        // boosted score descending, then path for stability.
        let normalized_query = normalize_file_path(query.trim());
        results.sort_by(|left, right| {
            file_search_rank(&normalized_query, left)
                .cmp(&file_search_rank(&normalized_query, right))
                .then_with(|| right.score.total_cmp(&left.score))
                .then_with(|| left.file_path.cmp(&right.file_path))
        });
    }
}
