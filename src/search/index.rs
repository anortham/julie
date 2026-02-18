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

use tantivy::collector::TopDocs;
use tantivy::schema::{OwnedValue, TantivyDocument};
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{Index, IndexReader, IndexWriter, Term};

use crate::search::error::{Result, SearchError};
use crate::search::language_config::LanguageConfigs;
use crate::search::query::{build_content_query, build_symbol_query};
use crate::search::schema::{create_schema, SchemaFields};
use crate::search::scoring::apply_important_patterns_boost;
use crate::search::tokenizer::CodeTokenizer;

const WRITER_HEAP_SIZE: usize = 50_000_000; // 50MB

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
#[derive(Default)]
pub struct SearchFilter {
    pub language: Option<String>,
    pub kind: Option<String>,
    pub file_pattern: Option<String>,
}

/// A symbol search result with relevance score.
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

/// A file content search result with relevance score.
pub struct ContentSearchResult {
    pub file_path: String,
    pub language: String,
    pub score: f32,
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
        Self::open_with_tokenizer(path, tokenizer, None)
    }

    /// Open an existing index with language-specific tokenizer patterns.
    pub fn open_with_language_configs(path: &Path, configs: &LanguageConfigs) -> Result<Self> {
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
    }

    /// Open an existing index or create a new one, using language-specific tokenizer patterns.
    pub fn open_or_create_with_language_configs(
        path: &Path,
        configs: &LanguageConfigs,
    ) -> Result<Self> {
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

        tantivy_doc.add_text(f.doc_type, "file");
        tantivy_doc.add_text(f.file_path, &doc.file_path);
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
    pub fn search_symbols(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<Vec<SymbolSearchResult>> {
        let f = &self.schema_fields;

        // Tokenize the query using the same code tokenizer, then remove compound
        // tokens whose sub-parts are all present. This prevents AND-per-term logic
        // from requiring partial compounds (e.g., "search_term") that are never
        // produced when indexing longer names (e.g., "search_term_one").
        let terms = Self::filter_compound_tokens(self.tokenize_query(query_str));
        if terms.is_empty() {
            return Ok(Vec::new());
        }

        let query = build_symbol_query(
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
        );

        let searcher = self.reader.searcher();
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

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

        Ok(results)
    }

    /// Search for file content matching the query.
    pub fn search_content(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<Vec<ContentSearchResult>> {
        let f = &self.schema_fields;

        // Note: no filter_compound_tokens here — compound tokens (e.g. "search_term")
        // are boosted via SHOULD+BoostQuery in build_content_query instead of stripped.
        let terms = self.tokenize_query(query_str);
        if terms.is_empty() {
            return Ok(Vec::new());
        }

        let query = build_content_query(
            &terms,
            f.content,
            f.doc_type,
            f.language,
            filter.language.as_deref(),
        );

        let searcher = self.reader.searcher();
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            results.push(ContentSearchResult {
                file_path: Self::get_text_field(&doc, f.file_path),
                language: Self::get_text_field(&doc, f.language),
                score,
            });
        }

        Ok(results)
    }

    // --- Private helpers ---

    fn open_or_create_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<Self> {
        let schema = create_schema();
        let schema_fields = SchemaFields::new(&schema);

        let index = Index::builder()
            .schema(schema)
            .create_in_dir(path)
            .or_else(|_| Index::open_in_dir(path))?;

        Self::register_tokenizer(&index, tokenizer);
        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(None),
            schema_fields,
            language_configs,
        })
    }

    fn create_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<Self> {
        let schema = create_schema();
        let schema_fields = SchemaFields::new(&schema);

        let index = Index::create_in_dir(path, schema)?;
        Self::register_tokenizer(&index, tokenizer);
        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(None),
            schema_fields,
            language_configs,
        })
    }

    fn open_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<Self> {
        let index = Index::open_in_dir(path)?;
        let schema = index.schema();
        let schema_fields = SchemaFields::new(&schema);

        Self::register_tokenizer(&index, tokenizer);
        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(None),
            schema_fields,
            language_configs,
        })
    }

    fn register_tokenizer(index: &Index, tokenizer: CodeTokenizer) {
        index
            .tokenizers()
            .register("code", TextAnalyzer::builder(tokenizer).build());
    }

    fn get_or_create_writer(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<IndexWriter>>> {
        let mut guard = self.writer.lock().unwrap();
        if guard.is_none() {
            *guard = Some(self.index.writer(WRITER_HEAP_SIZE)?);
        }
        Ok(guard)
    }

    /// Tokenize a query string using the registered code tokenizer.
    fn tokenize_query(&self, query_str: &str) -> Vec<String> {
        let mut tokenizer = self
            .index
            .tokenizers()
            .get("code")
            .expect("code tokenizer not registered");

        let mut stream = tokenizer.token_stream(query_str);
        let mut terms = Vec::new();
        while stream.advance() {
            terms.push(stream.token().text.clone());
        }
        terms.dedup();
        terms
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
            .and_then(|v| match v {
                OwnedValue::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn get_u64_field(doc: &TantivyDocument, field: tantivy::schema::Field) -> u64 {
        doc.get_first(field)
            .and_then(|v| match v {
                OwnedValue::U64(n) => Some(*n),
                _ => None,
            })
            .unwrap_or(0)
    }
}
