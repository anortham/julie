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
    build_symbol_query_weighted, parse_annotation_query,
};
use crate::search::schema::{
    SchemaCompatibilitySignature, SchemaFields, compatibility_signature, create_schema,
};
use crate::search::scoring::{
    apply_important_patterns_boost, apply_nl_path_prior, is_nl_like_query,
};
use crate::search::tokenizer::{CodeTokenizer, TokenizerCompatibilitySignature, split_camel_case};

// 256MB total budget. Tantivy 0.26's `Index::writer(budget)` auto-clamps thread
// count when per-thread budget falls below the 15MB floor. At 50MB we got 3
// threads at ~16.67MB each; 256MB gives 8 threads at 32MB each — closer to the
// indexing throughput ceiling on multi-core boxes.
const WRITER_HEAP_SIZE: usize = 256_000_000;
const NL_RERANK_OVERFETCH_FACTOR: usize = 4;
const SEARCH_COMPAT_MARKER_VERSION: u32 = 3;
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

/// Search filter for narrowing results.
#[derive(Default, Clone)]
pub struct SearchFilter {
    pub language: Option<String>,
    pub kind: Option<String>,
    pub file_pattern: Option<String>,
    pub exclude_tests: bool,
}

/// A symbol search result with relevance score.
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

    /// Commit pending changes to make them searchable.
    pub fn commit(&self) -> Result<()> {
        let mut guard = self.writer.lock().unwrap();
        if let Some(ref mut writer) = *guard {
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
        let candidate_limit = Self::rerank_candidate_limit(query_str, limit);
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
            });
        }

        // Apply important_patterns boost if language configs are available
        if let Some(configs) = &self.language_configs {
            apply_important_patterns_boost(&mut results, configs);
        }
        apply_nl_path_prior(&mut results, query_str);
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
        let candidate_limit = Self::rerank_candidate_limit(query_str, limit);
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
            });
        }

        // Apply important_patterns boost if language configs are available
        if let Some(configs) = &self.language_configs {
            apply_important_patterns_boost(&mut results, configs);
        }
        apply_nl_path_prior(&mut results, query_str);
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
            file_match_rank(left.match_kind)
                .cmp(&file_match_rank(right.match_kind))
                .then_with(|| right.score.total_cmp(&left.score))
                .then_with(|| left.file_path.cmp(&right.file_path))
        });
        if results.len() > limit {
            results.truncate(limit);
        }

        Ok(FileSearchResults { results, relaxed })
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

fn classify_file_match(query: &str, normalized_query: &str, file_path: &str) -> FileMatchKind {
    if query_contains_glob_syntax(query) {
        return FileMatchKind::Glob;
    }
    if file_path == normalized_query {
        return FileMatchKind::ExactPath;
    }
    if basename_for_path(file_path) == basename_for_path(normalized_query) {
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
