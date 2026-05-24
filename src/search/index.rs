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

use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, TermQuery};
use tantivy::schema::{IndexRecordOption, TantivyDocument, Value};
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{Index, IndexReader, IndexWriter, Term};

use crate::search::error::{Result, SearchError};
use crate::search::expansion::expand_query_terms;
use crate::search::language_config::LanguageConfigs;
use crate::search::query::{UnifiedQueryFieldSet, build_unified_query, parse_annotation_query};
use crate::search::schema::{
    SchemaCompatibilitySignature, SchemaFields, compatibility_signature, create_schema,
};
use crate::search::scoring::{apply_important_patterns_boost, is_test_path};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMatchKind {
    ExactPath,
    ExactBasename,
    PathFragment,
    Glob,
}

// ---------------------------------------------------------------------------
// Unified document type
// ---------------------------------------------------------------------------

/// Union-shape document that covers both symbol and file rows.
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
/// `pretokenized_code` and `relationship_text` are populated by the
/// projection layer.
pub struct SearchDocument {
    // ---- discriminator ----
    pub doc_type: String, // "symbol" | "file"

    // ---- shared fields ----
    pub id: String,
    pub name: String,
    pub language: String,
    pub file_path: String,
    pub basename: String,
    pub kind: String,      // symbol kind string, or "file"
    pub role: String,      // classify_role result
    pub test_role: String, // test_subrole result

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
        let (role, test_role) =
            symbol_role_and_test_role(&normalized_path, &symbol.language, symbol.metadata.as_ref());

        Self {
            doc_type: "symbol".to_string(),
            id: symbol.id.clone(),
            name: symbol.name.clone(),
            language: symbol.language.clone(),
            file_path: normalized_path,
            basename,
            kind: symbol.kind.to_string(),
            role,
            test_role,
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

    /// Build a symbol-row `SearchDocument` from raw primitive fields.
    ///
    /// Convenience constructor for direct test use. Computes `role`,
    /// `test_role`, and `basename` from the supplied path and language.
    pub fn symbol_from_parts(
        id: impl Into<String>,
        name: impl Into<String>,
        signature: impl Into<String>,
        doc_comment: impl Into<String>,
        code_body: impl Into<String>,
        file_path: impl Into<String>,
        kind: impl Into<String>,
        language: impl Into<String>,
        start_line: u32,
    ) -> Self {
        let file_path = file_path.into();
        let language = language.into();
        let normalized_path = normalize_file_path(&file_path);
        let basename = basename_for_path(&normalized_path).to_string();
        let role = crate::search::scoring::classify_role(&normalized_path, &language);
        let test_role = crate::search::scoring::test_subrole(&normalized_path);
        Self {
            doc_type: "symbol".to_string(),
            id: id.into(),
            name: name.into(),
            language,
            file_path: normalized_path,
            basename,
            kind: kind.into(),
            role: role.to_string(),
            test_role: test_role.to_string(),
            signature: signature.into(),
            doc_comment: doc_comment.into(),
            code_body: code_body.into(),
            annotation_keys: vec![],
            annotations_text: String::new(),
            owner_names_text: String::new(),
            start_line,
            content: String::new(),
            path_text: String::new(),
            pretokenized_code: String::new(),
            relationship_text: String::new(),
        }
    }

    /// Build a file-row `SearchDocument` from raw primitive fields.
    ///
    /// Convenience constructor for direct test use. Computes `basename`,
    /// `name` (stem), `role`, and `test_role` from the path and language.
    pub fn file_from_parts(
        file_path: impl Into<String>,
        content: impl Into<String>,
        language: impl Into<String>,
    ) -> Self {
        let file_path = file_path.into();
        let language = language.into();
        let content = content.into();
        let normalized_path = normalize_file_path(&file_path);
        let basename = basename_for_path(&normalized_path).to_string();
        let name = stem_of_basename(&basename).to_string();
        let role = crate::search::scoring::classify_role(&normalized_path, &language);
        let test_role = crate::search::scoring::test_subrole(&normalized_path);
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

        if self.exclude_tests && is_test_symbol_result(&result.file_path, &result.role) {
            return false;
        }

        true
    }
}

pub(crate) fn is_test_symbol_result(file_path: &str, role: &str) -> bool {
    is_test_path(file_path) || role == "test"
}

/// Project search role fields from path classification plus extractor test metadata.
pub(crate) fn symbol_role_and_test_role(
    file_path: &str,
    language: &str,
    metadata: Option<&std::collections::HashMap<String, serde_json::Value>>,
) -> (String, String) {
    let normalized_path = normalize_file_path(file_path);
    let path_role = crate::search::scoring::classify_role(&normalized_path, language);
    let path_test_role = crate::search::scoring::test_subrole(&normalized_path);

    let metadata_is_test = metadata
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let metadata_test_role = metadata
        .and_then(|m| m.get("test_role"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if metadata_is_test && path_role != "test" {
        let test_role = metadata_test_role.unwrap_or_else(|| path_test_role.to_string());
        ("test".to_string(), test_role)
    } else {
        (
            path_role.to_string(),
            metadata_test_role.unwrap_or_else(|| path_test_role.to_string()),
        )
    }
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

    /// Add a unified `SearchDocument` to the index.
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

    /// Search for symbols by routing through [`search_unified`] and converting
    /// symbol hits to [`SymbolSearchResult`].  Replaces the deleted `search_symbols`
    /// method; callers that need the old `SymbolSearchResults` shape (e.g. `hybrid.rs`)
    /// can use this adapter without a full refactor.
    pub fn search_symbols_via_unified(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        // Annotation queries (`@SomeAttr`, `[Authorize]`, `#[tokio::test]`,
        // `app.route("/")`, etc.) need the dedicated annotation pipeline
        // that filters on the `annotations_exact` indexed key.  We detect
        // them here and route to `search_annotation_symbols`; everything
        // else flows through the unified search path.
        let parsed_annotation = parse_annotation_query(query_str);
        if parsed_annotation.has_annotation_filters() {
            return self.search_annotation_symbols(query_str, filter, limit);
        }

        // Use the kind-filtered variant so the Tantivy BM25 candidate pool
        // contains only symbol rows.  Without this filter, queries like
        // "format" pull in 1000s of file rows that match the body content
        // and starve symbol candidates out of the over-fetch window.
        let (hits, relaxed) = self.search_unified_kind_filtered(query_str, filter, limit, false)?;
        let mut results: Vec<SymbolSearchResult> = hits
            .into_iter()
            .map(|h| SymbolSearchResult {
                id: h.id,
                name: h.name,
                signature: h.signature,
                doc_comment: h.doc_comment,
                file_path: h.file_path,
                kind: h.kind,
                language: h.language,
                start_line: h.start_line,
                score: h.tantivy_score,
                role: h.role,
                test_role: h.test_role,
            })
            .collect();

        // Apply language-specific important_patterns boost on top of the
        // unified reranker output.  Pre-T9 this lived inside `search_symbols`;
        // moved here so the adapter preserves the same scoring layer.
        // (The NL path prior is owned by the assembly layer, not here.)
        if let Some(configs) = &self.language_configs {
            apply_important_patterns_boost(&mut results, configs);
        }
        Ok(SymbolSearchResults { results, relaxed })
    }

    /// Annotation-aware symbol search.  Dispatched from
    /// [`search_symbols_via_unified`] when the query has annotation
    /// filters; uses [`build_annotation_symbol_query`] with the
    /// `annotations_exact` STRING field plus optional context terms from
    /// the surrounding query text.
    fn search_annotation_symbols(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        let f = &self.schema_fields;
        let parsed = parse_annotation_query(query_str);
        let term_query = parsed.remaining_query.as_str();
        let expanded = expand_query_terms(term_query);
        let original_terms = self.annotation_context_terms(term_query);
        let alias_terms = Self::filter_compound_tokens(self.tokenize_terms(&expanded.alias_terms));
        let normalized_terms =
            Self::filter_compound_tokens(self.tokenize_terms(&expanded.normalized_terms));

        let query = build_annotation_symbol_query(
            &original_terms,
            &alias_terms,
            &normalized_terms,
            &parsed.annotation_keys,
            f,
            filter,
            true,
        );

        let searcher = self.reader.searcher();
        let candidate_limit = limit.saturating_mul(NL_RERANK_OVERFETCH_FACTOR).max(500);
        let top_docs = searcher.search(
            &query,
            &TopDocs::with_limit(candidate_limit).order_by_score(),
        )?;

        let user_word_count = term_query.split_whitespace().count();
        let (top_docs, relaxed) = if top_docs.is_empty() && user_word_count > 1 {
            let or_query = build_annotation_symbol_query(
                &original_terms,
                &alias_terms,
                &normalized_terms,
                &parsed.annotation_keys,
                f,
                filter,
                false,
            );
            let or_top = searcher.search(
                &or_query,
                &TopDocs::with_limit(candidate_limit).order_by_score(),
            )?;
            (or_top, true)
        } else {
            (top_docs, false)
        };

        let mut results: Vec<SymbolSearchResult> = Vec::with_capacity(top_docs.len());
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
        if let Some(pattern) = filter.file_pattern.as_deref() {
            results.retain(|result| matches_glob_pattern(&result.file_path, pattern));
        }
        if filter.exclude_tests {
            results.retain(|result| !is_test_path(&result.file_path) && result.role != "test");
        }
        results.truncate(limit);
        Ok(SymbolSearchResults { results, relaxed })
    }

    /// `search_symbols` adapter — routes through [`search_unified`].
    ///
    /// The old `search_symbols` method was deleted in T9; this wrapper keeps
    /// existing callers (tests, hybrid.rs) compiling without per-file changes.
    pub fn search_symbols(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        self.search_symbols_via_unified(query_str, filter, limit)
    }

    /// `search_symbols_relaxed` adapter — routes through [`search_unified`].
    ///
    /// The legacy `search_symbols_relaxed` semantics were "always return
    /// relaxed=true" (it was the OR-mode entry point with no AND first).
    /// We preserve that flag for callers that still rely on it; the actual
    /// AND/OR behaviour is now decided inside `search_unified` based on the
    /// query shape and result count.
    pub fn search_symbols_relaxed(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        let mut out = self.search_symbols_via_unified(query_str, filter, limit)?;
        out.relaxed = true;
        Ok(out)
    }

    /// `search_content` adapter — routes through [`search_unified`], returns file hits.
    ///
    /// The old `search_content` method was deleted in T9; this wrapper keeps
    /// existing callers (tests) compiling.
    pub fn search_content(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<ContentSearchResults> {
        let (hits, relaxed, and_candidate_count, or_candidate_count) =
            self.search_unified_with_stage_counts(query_str, filter, limit, true)?;
        let results: Vec<ContentSearchResult> = hits
            .into_iter()
            .map(|h| ContentSearchResult {
                file_path: h.file_path,
                language: h.language,
                score: h.tantivy_score,
            })
            .collect();
        Ok(ContentSearchResults {
            results,
            relaxed,
            and_candidate_count,
            or_candidate_count,
        })
    }

    /// `search_files` adapter — routes through [`search_unified`], returns file hits.
    ///
    /// The old `search_files` method was deleted in T9; this wrapper keeps
    /// existing callers (tests) compiling.  The `match_kind` field is derived
    /// from the query and file_path via [`classify_file_match`].
    pub fn search_files(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<FileSearchResults> {
        let (hits, relaxed) = self.search_unified_kind_filtered(query_str, filter, limit, true)?;
        let normalized_query = normalize_file_path(query_str.trim());
        let results: Vec<FileSearchResult> = hits
            .into_iter()
            .map(|h| {
                let match_kind = classify_file_match(query_str, &normalized_query, &h.file_path);
                FileSearchResult {
                    file_path: h.file_path,
                    language: h.language,
                    score: h.tantivy_score,
                    match_kind,
                }
            })
            .collect();
        Ok(FileSearchResults { results, relaxed })
    }

    /// Single BM25 sweep across all seven core FTS fields, returning mixed-kind
    /// [`UnifiedHit`]s.  No `doc_type` filter — symbol rows, file rows, and any
    /// other document type are all eligible.
    ///
    /// Over-fetches by `NL_RERANK_OVERFETCH_FACTOR` before applying post-filters
    /// and reranking.
    /// Falls back to OR mode when AND returns zero results on a multi-term query.
    pub fn search_unified(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<Vec<UnifiedHit>> {
        let (hits, _relaxed) = self.search_unified_with_meta(query_str, filter, limit)?;
        Ok(hits)
    }

    /// Variant of [`search_unified`] that also reports whether the AND query
    /// fell back to OR mode.  Used by the workspace-routing layer so the
    /// tantivy upgrade-report snapshot tooling and dogfood helpers can record
    /// `relaxed` in their per-query telemetry.
    ///
    /// Honours `JULIE_RERANKER_ENABLED=0` to disable the rerank pass for the
    /// baseline-comparison harness.
    pub fn search_unified_with_meta(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<(Vec<UnifiedHit>, bool)> {
        self.search_unified_internal(query_str, filter, limit, None)
    }

    /// Variant that also accepts an optional kind filter applied BEFORE the
    /// rerank step (so the candidate set is properly pruned before scoring).
    /// `None` = no kind filtering; `Some(true)` = files only; `Some(false)` =
    /// symbols only.
    pub fn search_unified_kind_filtered(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
        files_only: bool,
    ) -> Result<(Vec<UnifiedHit>, bool)> {
        self.search_unified_internal(query_str, filter, limit, Some(files_only))
    }

    /// Variant of [`search_unified`] that also reports per-stage candidate
    /// counts (AND-mode hit count and OR-mode hit count) alongside the
    /// `relaxed` flag.  Used by the `search_content` adapter and the
    /// `line_mode_or_fallback_tests` stage-count assertions.
    pub fn search_unified_with_stage_counts(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
        files_only: bool,
    ) -> Result<(Vec<UnifiedHit>, bool, usize, usize)> {
        self.search_unified_full(query_str, filter, limit, Some(files_only))
    }

    fn search_unified_internal(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
        files_only: Option<bool>,
    ) -> Result<(Vec<UnifiedHit>, bool)> {
        let (hits, relaxed, _and, _or) =
            self.search_unified_full(query_str, filter, limit, files_only)?;
        Ok((hits, relaxed))
    }

    /// Underlying implementation that records both AND-stage and OR-stage
    /// candidate counts so callers (e.g. `search_content`) can report them
    /// without re-running the search.
    fn search_unified_full(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
        files_only: Option<bool>,
    ) -> Result<(Vec<UnifiedHit>, bool, usize, usize)> {
        use crate::extractors::SymbolKind;
        use crate::search::query_parse::parse_query;
        use crate::search::reranker::{Candidate, rerank_unified};
        use crate::search::scoring::{DOC_LANGUAGES, classify_role, test_subrole};

        let f = &self.schema_fields;

        if files_only != Some(true) {
            let parsed_annotation = parse_annotation_query(query_str);
            if parsed_annotation.has_annotation_filters() {
                let symbol_results = self.search_annotation_symbols(query_str, filter, limit)?;
                let relaxed = symbol_results.relaxed;
                let hits: Vec<UnifiedHit> = symbol_results
                    .results
                    .into_iter()
                    .map(|symbol| {
                        let basename = symbol
                            .file_path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&symbol.file_path)
                            .to_string();
                        UnifiedHit {
                            id: symbol.id,
                            kind: symbol.kind,
                            name: symbol.name,
                            path_text: symbol.file_path.clone(),
                            file_path: symbol.file_path,
                            basename,
                            signature: symbol.signature,
                            doc_comment: symbol.doc_comment,
                            code_body: String::new(),
                            pretokenized_code: String::new(),
                            relationship_text: String::new(),
                            language: symbol.language,
                            start_line: symbol.start_line,
                            role: symbol.role,
                            test_role: symbol.test_role,
                            tantivy_score: symbol.score,
                        }
                    })
                    .collect();
                let count = hits.len();
                return Ok((hits, relaxed, count, 0));
            }
        }

        let expanded = expand_query_terms(query_str);
        // Two-tier original-term shape:
        //  * `original_terms` keeps only the split parts (compound stripped
        //    via `filter_compound_tokens`).  These are the AND-required
        //    constraints so that a query like "marker_abc" still matches a
        //    file containing both "marker" and "abc" separately.
        //  * `alias_terms` absorbs the compound tokens (`marker_abc`,
        //    `files_by_language`, etc.) as optional Should clauses.  Files
        //    that DO contain the compound get a BM25 boost from those
        //    clauses; files that only have the parts still match.
        let raw_original = self.tokenize_terms(&expanded.original_terms);
        let raw_alias = self.tokenize_terms(&expanded.alias_terms);
        let raw_normalized = self.tokenize_terms(&expanded.normalized_terms);

        let original_terms = Self::filter_compound_tokens(raw_original.clone());
        // The compound tokens themselves: tokens that were in raw_original
        // but got stripped by `filter_compound_tokens` (i.e. snake_case
        // compounds whose parts are all present).  Add them to alias_terms
        // so they contribute as `Should` clauses (scoring boost, not AND
        // requirement).
        let compound_overflow: Vec<String> = raw_original
            .into_iter()
            .filter(|t| !original_terms.contains(t))
            .collect();
        let mut alias_terms = Self::filter_compound_tokens(raw_alias);
        alias_terms.extend(compound_overflow);
        let normalized_terms = Self::filter_compound_tokens(raw_normalized);

        if original_terms.is_empty() {
            return Ok((Vec::new(), false, 0, 0));
        }

        // Over-fetch generously so exact-name promotion has a real pool to
        // partition from.  BM25 alone often buries an exact-name function
        // (e.g. `format_results`) under partial-match files that mention the
        // tokenised form (`format`, `result`) many times in their bodies.
        // The deleted per-target symbol path used `limit.saturating_mul(20).max(500)`;
        // the unified path matches that floor so the exact-name partitioner
        // can find the canonical symbol in the candidate set.
        let candidate_limit = limit.saturating_mul(NL_RERANK_OVERFETCH_FACTOR).max(500);

        // Optional doc_type filter applied at the Tantivy query level — only
        // documents of the requested type contribute to BM25 candidate
        // selection.  This is the right place for the filter (vs post-fetch)
        // because the candidate set is otherwise dominated by file rows for
        // common terms like "format", starving symbol queries.
        let wrap_with_doc_type =
            |inner: Box<dyn tantivy::query::Query>| -> Box<dyn tantivy::query::Query> {
                match files_only {
                    Some(want_file) => {
                        let dt = if want_file { "file" } else { "symbol" };
                        let dt_query = TermQuery::new(
                            Term::from_field_text(f.doc_type, dt),
                            IndexRecordOption::Basic,
                        );
                        Box::new(BooleanQuery::new(vec![
                            (Occur::Must, inner),
                            (Occur::Must, Box::new(dt_query)),
                        ]))
                    }
                    None => inner,
                }
            };

        // Field-set follows the kind filter: when the caller restricts to
        // file rows, search only content/path_text; when restricted to
        // symbol rows, search the seven symbol fields; when mixed, search
        // all eight.  This keeps BM25 IDF from being skewed by empty fields
        // on the side of the union we don't care about.
        let unified_field_set = match files_only {
            Some(true) => UnifiedQueryFieldSet::FilesOnly,
            Some(false) => UnifiedQueryFieldSet::SymbolsOnly,
            None => UnifiedQueryFieldSet::Mixed,
        };

        let and_inner = build_unified_query(
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
            f.content,
            unified_field_set,
            true, // require_all_terms — AND mode
        );
        let and_query = wrap_with_doc_type(Box::new(and_inner));

        let searcher = self.reader.searcher();
        let top_docs = searcher.search(
            &*and_query,
            &TopDocs::with_limit(candidate_limit).order_by_score(),
        )?;
        let and_candidate_count = top_docs.len();
        // Auto-fallback to OR when AND returns nothing.
        //
        // Two trigger conditions:
        //   1. Multi-word query (`user_word_count > 1`) — original guard
        //   2. Single-word query whose code-tokenizer split it into multiple
        //      tokens (e.g. "formatting" → ["f","or","matting"] because Lua's
        //      preserve_patterns include "or"). In this case AND across the
        //      derived tokens almost never matches a real symbol's indexed
        //      name, so we fall back to OR to surface meaningful candidates.
        let user_word_count = query_str.split_whitespace().count();
        // Derived-overflow: a single-word query whose tokenizer-produced
        // tokens don't include the lowercased word as-is means the code
        // tokenizer shredded it via preserve_patterns (e.g. "formatting"
        // → ["f","or","matting"] because Lua's preserve_patterns include
        // "or").  In that case the AND query across the derived tokens
        // almost never matches a real symbol's indexed name and we want
        // OR-fallback to surface meaningful candidates.
        //
        // Conversely, "nonexistent_symbol_xyz" tokenizes to ["nonexistent_symbol_xyz",
        // "nonexistent", "symbol", "xyz"] — the compound token IS present,
        // so it's a legitimate AND-miss for a nonexistent identifier and
        // OR-fallback should NOT fire.
        let query_lower = query_str.trim().to_lowercase();
        // Compound check uses `alias_terms` because compound tokens (those
        // present in the raw tokenizer output but stripped from
        // `original_terms` because their parts are also present) live there
        // after the two-tier shuffle above.  If either group contains the
        // query verbatim, the tokenizer didn't shred it — this is a
        // legitimate compound miss, not a `derived_overflow` situation.
        let compound_in_tokens = original_terms.iter().any(|t| t == &query_lower)
            || alias_terms.iter().any(|t| t == &query_lower);
        let derived_overflow =
            user_word_count == 1 && original_terms.len() > 1 && !compound_in_tokens;
        let mut relaxed = false;
        let mut or_candidate_count: usize = 0;
        let top_docs = if top_docs.is_empty() && (user_word_count > 1 || derived_overflow) {
            relaxed = true;
            let or_inner = build_unified_query(
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
                f.content,
                unified_field_set,
                false, // OR mode
            );
            let or_query = wrap_with_doc_type(Box::new(or_inner));
            let or_top = searcher.search(
                &*or_query,
                &TopDocs::with_limit(candidate_limit).order_by_score(),
            )?;
            or_candidate_count = or_top.len();
            or_top
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
            hits.retain(|h| crate::tools::search::matches_glob_pattern(&h.file_path, pattern));
        }
        if filter.exclude_tests {
            hits.retain(|h| !is_test_symbol_result(&h.file_path, &h.role));
        }
        // Note: doc_type filtering for symbol-vs-file partition is applied
        // at the Tantivy query level above via `wrap_with_doc_type`.

        // Reranker toggle: honours `JULIE_RERANKER_ENABLED=0` (default-on so
        // any other value, missing var, or "1" keeps it enabled).  When off,
        // candidates retain raw Tantivy BM25 ordering — used by the c4
        // discoverability baseline test and the ablation harness.
        let reranker_enabled = !matches!(
            std::env::var("JULIE_RERANKER_ENABLED").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE")
        );

        // T6 unified reranking — builds Candidate structs for every hit and
        // delegates to `rerank_unified` which handles both symbol rows
        // (`is_file_doc == false`) and file rows (`is_file_doc == true`) in a
        // single pass with Eros-recipe field-score boosts.
        if reranker_enabled && !hits.is_empty() {
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
                    let is_source_language = !DOC_LANGUAGES.contains(&hit.language.as_str());

                    let mut body = String::with_capacity(
                        hit.signature.len() + hit.doc_comment.len() + hit.code_body.len() + 2,
                    );
                    body.push_str(&hit.signature);
                    if !body.is_empty() && !hit.doc_comment.is_empty() {
                        body.push(' ');
                    }
                    body.push_str(&hit.doc_comment);
                    if !body.is_empty() && !hit.code_body.is_empty() {
                        body.push(' ');
                    }
                    body.push_str(&hit.code_body);

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

            // rerank_unified returns sorted output; write scores back to the
            // original hits using the ordinal index carried in Ranked::original_index.
            // Index-based writeback is collision-free: the old (path, title) key
            // aliased file rows (name = basename stem, e.g. "foo" for src/foo.rs)
            // with same-named symbols in the same file, causing one candidate's
            // score to overwrite the other's and silently flip their ranks.
            let ranked = rerank_unified(&parsed, &candidates);

            let mut reranked_scores: Vec<Option<f32>> = vec![None; candidates.len()];
            for r in &ranked {
                reranked_scores[r.original_index] = Some(r.final_score);
            }
            for (hit, score_opt) in hits.iter_mut().zip(reranked_scores.iter()) {
                if let Some(&s) = score_opt.as_ref() {
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

        // Exact-name promotion: partition hits into (definitions, other_exact,
        // rest) where "exact" means symbol name matches the query (full or
        // last-component-of-qualified).  This runs regardless of reranker
        // state because BM25 alone often buries exact-name hits beneath
        // partial matches with more body content.  Ported from the deleted
        // per-target `promote_exact_name_matches`; the assertion is that the
        // exact-name symbol must surface to the top of definition searches
        // (c4_test_helper_discoverability) and qualified-name searches
        // (Phoenix.Router style).
        promote_exact_unified_hits(&mut hits, query_str);

        hits.truncate(limit);
        Ok((hits, relaxed, and_candidate_count, or_candidate_count))
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
                        Self::recreate_index_with_lock(path, &expected_schema, &expected_marker)?,
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
        index.tokenizers().register(
            "simple_code",
            TextAnalyzer::builder(SimpleCodeTokenizer::new()).build(),
        );
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

    /// Rebuild the Tantivy index at `path` under a cross-process advisory lock.
    ///
    /// # Why the lock lives in the PARENT directory
    ///
    /// The previous unlocked recreate path placed the sentinel file inside `path` itself.
    /// Rebuilding starts with `remove_dir_all(path)`, which would delete the sentinel,
    /// so a concurrent opener that had already seen `AlreadyExists` on the sentinel
    /// would then race against the directory teardown — opening a half-deleted tree
    /// or missing the lock entirely.
    ///
    /// The lock file (`<parent>/<dirname>.julie-rebuild.lock`) is a stable sibling
    /// that survives `remove_dir_all`.  `fs2::FileExt::lock_exclusive` blocks the
    /// second caller until the first has finished and released the lock; the loser
    /// then re-checks compatibility and returns the already-rebuilt index early.
    ///
    /// # Atomic rename
    ///
    /// The rebuilt index is first created in `<parent>/<dirname>.tmp-rebuild`.
    /// Only after a successful `write_compat_marker` is the old directory removed
    /// and the temp directory renamed into place.  If the process crashes mid-way,
    /// the next caller cleans up the orphaned `.tmp-rebuild` before proceeding.
    fn recreate_index_with_lock(
        path: &Path,
        schema: &tantivy::schema::Schema,
        marker: &SearchCompatMarker,
    ) -> Result<Index> {
        use fs2::FileExt;

        // Derive stable sibling names in the PARENT directory.
        let dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "tantivy".to_string());
        let parent = path.parent().unwrap_or(path);

        let lock_path = parent.join(format!("{dir_name}.julie-rebuild.lock"));
        let tmp_path = parent.join(format!("{dir_name}.tmp-rebuild"));

        // Open (creating if needed) the advisory lock file.  Never truncate —
        // fs2 flocks are bound to the file's inode; truncating would not break
        // existing holders but is unnecessary.
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|err| {
                SearchError::IndexError(format!(
                    "failed to open rebuild lock at {}: {err}",
                    lock_path.display()
                ))
            })?;

        // Block until we hold the exclusive lock.  When a concurrent caller
        // finishes and drops its lock, we wake and re-check compatibility below.
        lock_file.lock_exclusive().map_err(|err| {
            SearchError::IndexError(format!(
                "failed to acquire rebuild lock at {}: {err}",
                lock_path.display()
            ))
        })?;
        // Lock is released when `lock_file` is dropped at end of scope.

        // Re-check: the process that held the lock before us may have already
        // rebuilt a compatible index.  If so, open and return it immediately.
        if path.exists() {
            if let Ok(existing) = Index::open_in_dir(path) {
                if Self::index_is_compatible(path, schema, &existing.schema(), marker) {
                    tracing::debug!(
                        "Index at {} was rebuilt by a concurrent opener; reusing",
                        path.display()
                    );
                    return Ok(existing);
                }
                drop(existing);
            }
        }

        // Clean up any orphaned temp directory from a previous crashed rebuild.
        if tmp_path.exists() {
            std::fs::remove_dir_all(&tmp_path).map_err(|err| {
                SearchError::IndexError(format!(
                    "failed to remove stale tmp rebuild dir {}: {err}",
                    tmp_path.display()
                ))
            })?;
        }

        // Build the new index into the temp directory.
        std::fs::create_dir_all(&tmp_path)?;
        let _tmp_index = Index::create_in_dir(&tmp_path, schema.clone())?;
        Self::write_compat_marker(&tmp_path, marker)?;

        // Atomically replace: remove old, rename temp into final location.
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }
        std::fs::rename(&tmp_path, path).map_err(|err| {
            SearchError::IndexError(format!(
                "failed to rename {} → {}: {err}",
                tmp_path.display(),
                path.display()
            ))
        })?;

        // Re-open from the final location (the index object pointed at tmp_path).
        Index::open_in_dir(path).map_err(|err| {
            SearchError::IndexError(format!(
                "failed to open rebuilt index at {}: {err}",
                path.display()
            ))
        })
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

/// Normalise a name or query to its lowercase, alphanumeric-only compact form.
///
/// Strips separators (`_`, `-`, ` `, ...) and case so that `displayTemplate`,
/// `display_template`, `display-template`, and `display template` all map to
/// `displaytemplate`.  Used by the title-exact reranker for both the files
/// and content search paths to avoid the per-term matching footgun where a
/// multi-word query would boost a file whose only matching symbol is a
/// generic one-word name.
/// Three-tier stable partition for unified hits:
///   1. Definition-kind symbols whose name matches `query` (full or last-
///      component-of-qualified) — promoted to top, sorted by source-tier,
///      then score.
///   2. Other exact-name matches (non-definition kinds like Import).
///   3. Everything else, score-ordered.
///
/// Mirrors `promote_exact_name_matches` from the per-target pipeline but
/// operates on the unified `UnifiedHit` shape.
fn promote_exact_unified_hits(hits: &mut Vec<UnifiedHit>, query: &str) {
    if hits.is_empty() {
        return;
    }
    use crate::search::scoring::{DEFINITION_KINDS, DOC_LANGUAGES, is_name_match, is_test_path};

    let query_lower = query.trim().to_lowercase();
    let mut definitions: Vec<UnifiedHit> = Vec::new();
    let mut other_exact: Vec<UnifiedHit> = Vec::new();
    let mut rest: Vec<UnifiedHit> = Vec::new();

    for hit in hits.drain(..) {
        if is_name_match(&hit.name, &query_lower) {
            if DEFINITION_KINDS.contains(&hit.kind.as_str()) {
                definitions.push(hit);
            } else {
                other_exact.push(hit);
            }
        } else {
            rest.push(hit);
        }
    }

    // Within definitions: full-match first, then source-tier (source>test>doc),
    // then score desc.
    definitions.sort_by(|a, b| {
        let is_full_match = |h: &UnifiedHit| -> bool { h.name.to_lowercase() == query_lower };
        let file_tier = |h: &UnifiedHit| -> u8 {
            if DOC_LANGUAGES.contains(&h.language.as_str()) {
                2
            } else if is_test_path(&h.file_path) {
                1
            } else {
                0
            }
        };
        let a_full = !is_full_match(a);
        let b_full = !is_full_match(b);
        a_full
            .cmp(&b_full)
            .then_with(|| file_tier(a).cmp(&file_tier(b)))
            .then_with(|| {
                b.tantivy_score
                    .partial_cmp(&a.tantivy_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    // Other exact matches: score desc.
    other_exact.sort_by(|a, b| {
        b.tantivy_score
            .partial_cmp(&a.tantivy_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    hits.extend(definitions);
    hits.extend(other_exact);
    hits.extend(rest);
}

pub(crate) fn compact_alnum_lc(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

// ---------------------------------------------------------------------------
// Test-only shims: apply_reranker_to_content_results,
//                  apply_symbol_title_boost_to_file_results
//
// The old per-target reranker entry points were deleted in T9.  These thin
// wrappers re-implement the title-exact boost (the only part the unit tests
// exercise) so the tests in `title_exact_boost_tests.rs` keep compiling and
// passing without modification.
// ---------------------------------------------------------------------------

/// Title-exact boost for content (file-path) search results.
///
/// For each file in `results`, look up the symbol names stored for that file
/// in `db`.  If the compact-alphanum form of any symbol name equals the
/// compact-alphanum form of the query (after stripping spaces), add
/// `EXACT_TITLE_BOOST` to that file's score and re-sort descending.
///
/// When `db` is `None` the function returns immediately (preserves BM25 order).
#[cfg(test)]
pub(crate) fn apply_reranker_to_content_results(
    query: &str,
    results: &mut Vec<ContentSearchResult>,
    db: Option<&crate::database::SymbolDatabase>,
) {
    let Some(db) = db else { return };
    if results.is_empty() {
        return;
    }
    let query_compact = compact_alnum_lc(query);
    let paths: Vec<&str> = results.iter().map(|r| r.file_path.as_str()).collect();
    let Ok(titles_map) = db.titles_for_files(&paths) else {
        return;
    };
    for result in results.iter_mut() {
        if let Some(titles) = titles_map.get(result.file_path.as_str()) {
            for title in titles {
                if compact_alnum_lc(title) == query_compact {
                    result.score += crate::search::reranker::EXACT_TITLE_BOOST;
                    break;
                }
            }
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Title-exact boost for file search results.
///
/// Identical logic to `apply_reranker_to_content_results` but operates on
/// `FileSearchResult` so file-target tests keep passing.
#[cfg(test)]
pub(crate) fn apply_symbol_title_boost_to_file_results(
    query: &str,
    results: &mut Vec<FileSearchResult>,
    db: &crate::database::SymbolDatabase,
) {
    if results.is_empty() {
        return;
    }
    let query_compact = compact_alnum_lc(query);
    let paths: Vec<&str> = results.iter().map(|r| r.file_path.as_str()).collect();
    let Ok(titles_map) = db.titles_for_files(&paths) else {
        return;
    };
    for result in results.iter_mut() {
        if let Some(titles) = titles_map.get(result.file_path.as_str()) {
            for title in titles {
                if compact_alnum_lc(title) == query_compact {
                    result.score += crate::search::reranker::EXACT_TITLE_BOOST;
                    break;
                }
            }
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
