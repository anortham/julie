use super::query::{basename_for_path, normalize_file_path};
use crate::search::scoring::is_test_path;
use julie_core::glob::matches_glob_pattern;

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
        symbol: &julie_extractors::Symbol,
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
    #[cfg(any(test, feature = "test-support"))]
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
    #[cfg(any(test, feature = "test-support"))]
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
    pub fn for_file(file_info: &julie_core::database::FileInfo) -> Self {
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
#[cfg(any(test, feature = "test-support"))]
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
#[cfg(any(test, feature = "test-support"))]
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
