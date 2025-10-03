// SearchIndexWriter - Handles all write operations to Tantivy index
//
// Separating the IndexWriter from SearchEngine eliminates RwLock contention:
// - SearchEngine (Arc<RwLock<>>) - read-only operations, concurrent searches
// - SearchIndexWriter (Arc<Mutex<>>) - write operations, serial indexing
//
// This allows searches to proceed during background indexing without blocking.

use crate::extractors::Symbol;
use anyhow::Result;
use tantivy::{Index, IndexWriter, Term};
use tracing::debug;

use super::super::schema::{CodeSearchSchema, LanguageBoosting};

/// Writer for Tantivy search index
///
/// This struct holds the IndexWriter and handles all write operations
/// (indexing, commits, deletions) separately from the SearchEngine's reader.
pub struct SearchIndexWriter {
    writer: IndexWriter,
    schema: CodeSearchSchema,
    language_boosting: LanguageBoosting,
}

impl SearchIndexWriter {
    /// Create a new search index writer
    pub fn new(index: &Index, schema: CodeSearchSchema) -> Result<Self> {
        let writer = index.writer(50_000_000)?; // 50MB heap
        let language_boosting = LanguageBoosting::new();

        Ok(Self {
            writer,
            schema,
            language_boosting,
        })
    }

    /// Create a search index writer for testing (smaller heap)
    pub fn in_memory(index: &Index, schema: CodeSearchSchema) -> Result<Self> {
        let writer = index.writer(15_000_000)?; // 15MB heap minimum for testing
        let language_boosting = LanguageBoosting::new();

        Ok(Self {
            writer,
            schema,
            language_boosting,
        })
    }

    /// Index a batch of symbols
    pub async fn index_symbols(&mut self, symbols: Vec<Symbol>) -> Result<()> {
        let symbol_count = symbols.len();

        for (index, symbol) in symbols.into_iter().enumerate() {
            if index % 100 == 0 {
                debug!("📝 Indexed {}/{} symbols", index, symbol_count);
            }

            let doc = super::super::schema::SearchDocument {
                symbol_id: symbol.id.clone(),
                symbol_name: symbol.name.clone(),
                symbol_kind: symbol.kind.to_string(),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                signature: symbol.signature.clone(),
                doc_comment: symbol.doc_comment.clone(),
                code_context: symbol.code_context.clone(),
                start_line: symbol.start_line,
                end_line: symbol.end_line,
                start_column: symbol.start_column,
                end_column: symbol.end_column,
                start_byte: symbol.start_byte,
                end_byte: symbol.end_byte,
                visibility: symbol.visibility.map(|v| v.to_string()),
                parent_id: symbol.parent_id.clone(),
                metadata: symbol
                    .metadata
                    .as_ref()
                    .map(|metadata| serde_json::to_string(metadata).unwrap_or_default()),
                semantic_group: symbol.semantic_group.clone(),
                confidence: symbol.confidence.map(|c| c as f64),
            };

            self.add_document(doc)?;
        }

        self.commit().await?;
        Ok(())
    }

    /// Add a single document to the index
    fn add_document(&mut self, doc: super::super::schema::SearchDocument) -> Result<()> {
        let mut tantivy_doc = tantivy::doc!();
        let fields = self.schema.fields();

        tantivy_doc.add_text(fields.symbol_id, &doc.symbol_id);
        tantivy_doc.add_text(fields.symbol_name, &doc.symbol_name);
        tantivy_doc.add_text(fields.symbol_name_exact, &doc.symbol_name);
        tantivy_doc.add_text(fields.symbol_kind, &doc.symbol_kind);
        tantivy_doc.add_text(fields.language, &doc.language);
        tantivy_doc.add_text(fields.file_path, &doc.file_path);
        tantivy_doc.add_text(fields.file_path_exact, &doc.file_path);

        if let Some(signature) = &doc.signature {
            tantivy_doc.add_text(fields.signature, signature);
            tantivy_doc.add_text(fields.signature_exact, signature);
        }

        if let Some(doc_comment) = &doc.doc_comment {
            tantivy_doc.add_text(fields.doc_comment, doc_comment);
        }

        if let Some(context) = &doc.code_context {
            tantivy_doc.add_text(fields.code_context, context);
        }

        tantivy_doc.add_u64(fields.start_line, doc.start_line as u64);
        tantivy_doc.add_u64(fields.end_line, doc.end_line as u64);
        tantivy_doc.add_u64(fields.start_column, doc.start_column as u64);
        tantivy_doc.add_u64(fields.end_column, doc.end_column as u64);
        tantivy_doc.add_u64(fields.start_byte, doc.start_byte as u64);
        tantivy_doc.add_u64(fields.end_byte, doc.end_byte as u64);

        if let Some(visibility) = &doc.visibility {
            tantivy_doc.add_text(fields.visibility, visibility);
        }

        if let Some(parent_id) = &doc.parent_id {
            tantivy_doc.add_text(fields.parent_id, parent_id);
        }

        if let Some(metadata) = &doc.metadata {
            tantivy_doc.add_text(fields.metadata, metadata);
        }

        if let Some(semantic_group) = &doc.semantic_group {
            tantivy_doc.add_text(fields.semantic_group, semantic_group);
        }

        if let Some(confidence) = doc.confidence {
            tantivy_doc.add_f64(fields.confidence, confidence);
        }

        let all_text = doc.generate_all_text();
        tantivy_doc.add_text(fields.all_text, &all_text);

        let exact_matches = doc.generate_exact_matches();
        tantivy_doc.add_text(fields.exact_matches, &exact_matches);

        let language_boost = self.language_boosting.get_language_boost(&doc.language);
        tantivy_doc.add_f64(fields.language_boost, language_boost);

        self.writer.add_document(tantivy_doc)?;
        Ok(())
    }

    /// Commit pending changes to the index
    ///
    /// NOTE: After calling this, the caller MUST reload the SearchEngine's reader
    /// to see the new changes. This is done separately to avoid lock contention.
    pub async fn commit(&mut self) -> Result<()> {
        self.writer.commit()?;
        Ok(())
    }

    /// Delete symbols for a file (for incremental updates)
    pub async fn delete_file_symbols(&mut self, file_path: &str) -> Result<()> {
        let fields = self.schema.fields();
        let term = Term::from_field_text(fields.file_path_exact, file_path);
        self.writer.delete_term(term);
        Ok(())
    }
}
