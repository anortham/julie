//! Tantivy-based search index for developer memory checkpoints.
//!
//! Separate from the code symbol index (`SearchIndex`), this index stores
//! checkpoint memories and supports natural-language BM25 search across
//! body, tags, symbols, decision, and impact fields.
//!
//! Uses Tantivy's default tokenizer (not `CodeTokenizer`) since memories
//! are natural language, not code identifiers.
//!
//! Index location: `.julie/indexes/memories/tantivy/`

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, OwnedValue, Schema, STORED, STRING, TEXT, TantivyDocument};
use tantivy::{Index, IndexReader, IndexWriter};

use super::storage::parse_checkpoint;
use super::{Checkpoint, MemorySearchResult};

const WRITER_HEAP_SIZE: usize = 15_000_000; // 15MB — smaller than code index, fewer docs

// ============================================================================
// Schema
// ============================================================================

/// Field name constants for the memory schema.
mod fields {
    pub const ID: &str = "id";
    pub const BODY: &str = "body";
    pub const TAGS: &str = "tags";
    pub const SYMBOLS: &str = "symbols";
    pub const DECISION: &str = "decision";
    pub const IMPACT: &str = "impact";
    pub const BRANCH: &str = "branch";
    pub const TIMESTAMP: &str = "timestamp";
    pub const FILE_PATH: &str = "file_path";
}

/// Pre-resolved field handles for efficient document construction and retrieval.
#[derive(Clone)]
struct MemorySchemaFields {
    id: Field,
    body: Field,
    tags: Field,
    symbols: Field,
    decision: Field,
    impact: Field,
    branch: Field,
    timestamp: Field,
    file_path: Field,
}

/// Build the Tantivy schema for memory search.
///
/// Uses Tantivy's default tokenizer (English-aware, not code-aware) for TEXT
/// fields. STRING fields are exact-match only.
fn create_memory_schema() -> Schema {
    let mut builder = Schema::builder();

    // Exact-match fields (STRING = raw tokenizer, stored)
    builder.add_text_field(fields::ID, STRING | STORED);
    builder.add_text_field(fields::BRANCH, STRING | STORED);
    builder.add_text_field(fields::TIMESTAMP, STRING | STORED);
    builder.add_text_field(fields::FILE_PATH, STRING | STORED);

    // Full-text searchable fields (TEXT = default tokenizer, stored)
    builder.add_text_field(fields::BODY, TEXT | STORED);
    builder.add_text_field(fields::TAGS, TEXT | STORED);
    builder.add_text_field(fields::SYMBOLS, TEXT | STORED);
    builder.add_text_field(fields::DECISION, TEXT | STORED);
    builder.add_text_field(fields::IMPACT, TEXT | STORED);

    builder.build()
}

impl MemorySchemaFields {
    /// Resolve all field handles from a schema.
    ///
    /// # Panics
    /// Panics if the schema was not created by `create_memory_schema()`.
    fn new(schema: &Schema) -> Self {
        Self {
            id: schema.get_field(fields::ID).unwrap(),
            body: schema.get_field(fields::BODY).unwrap(),
            tags: schema.get_field(fields::TAGS).unwrap(),
            symbols: schema.get_field(fields::SYMBOLS).unwrap(),
            decision: schema.get_field(fields::DECISION).unwrap(),
            impact: schema.get_field(fields::IMPACT).unwrap(),
            branch: schema.get_field(fields::BRANCH).unwrap(),
            timestamp: schema.get_field(fields::TIMESTAMP).unwrap(),
            file_path: schema.get_field(fields::FILE_PATH).unwrap(),
        }
    }
}

// ============================================================================
// MemoryIndex
// ============================================================================

/// Tantivy-backed search index for developer memory checkpoints.
///
/// Supports indexing checkpoints and searching via BM25 across natural-language
/// fields (body, tags, symbols, decision, impact).
pub struct MemoryIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<Option<IndexWriter>>,
    fields: MemorySchemaFields,
}

impl MemoryIndex {
    /// Create a new memory index at the given directory path.
    pub fn create(path: &Path) -> Result<Self> {
        let schema = create_memory_schema();
        let fields = MemorySchemaFields::new(&schema);

        let index = Index::create_in_dir(path, schema)
            .with_context(|| format!("Failed to create memory index at {}", path.display()))?;

        let reader = index
            .reader()
            .context("Failed to create memory index reader")?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(None),
            fields,
        })
    }

    /// Open an existing memory index or create a new one if it doesn't exist.
    pub fn open_or_create(path: &Path) -> Result<Self> {
        let schema = create_memory_schema();
        let fields = MemorySchemaFields::new(&schema);

        let index = Index::builder()
            .schema(schema)
            .create_in_dir(path)
            .or_else(|_| Index::open_in_dir(path))
            .with_context(|| {
                format!(
                    "Failed to open or create memory index at {}",
                    path.display()
                )
            })?;

        let reader = index
            .reader()
            .context("Failed to create memory index reader")?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(None),
            fields,
        })
    }

    /// Get the total number of documents in the index.
    pub fn num_docs(&self) -> u64 {
        self.reader.reload().ok();
        self.reader.searcher().num_docs()
    }

    /// Add a checkpoint to the index.
    ///
    /// The `file_path` parameter is the relative path within `.memories/`
    /// (e.g., `"2026-03-07/143000_abcd.md"`). Pass `None` if not applicable.
    ///
    /// Requires an explicit `commit()` call to make searchable.
    pub fn add_checkpoint(
        &self,
        checkpoint: &Checkpoint,
        file_path: Option<&str>,
    ) -> Result<()> {
        let f = &self.fields;
        let mut doc = TantivyDocument::new();

        doc.add_text(f.id, &checkpoint.id);
        doc.add_text(f.body, &checkpoint.description);
        doc.add_text(f.timestamp, &checkpoint.timestamp);
        doc.add_text(f.file_path, file_path.unwrap_or(""));

        // Join tags with spaces for tokenized search
        let tags_str = checkpoint
            .tags
            .as_deref()
            .map(|t| t.join(" "))
            .unwrap_or_default();
        doc.add_text(f.tags, &tags_str);

        // Join symbols with spaces for tokenized search
        let symbols_str = checkpoint
            .symbols
            .as_deref()
            .map(|s| s.join(" "))
            .unwrap_or_default();
        doc.add_text(f.symbols, &symbols_str);

        doc.add_text(
            f.decision,
            checkpoint.decision.as_deref().unwrap_or(""),
        );
        doc.add_text(f.impact, checkpoint.impact.as_deref().unwrap_or(""));

        // Extract branch from git context
        let branch = checkpoint
            .git
            .as_ref()
            .and_then(|g| g.branch.as_deref())
            .unwrap_or("");
        doc.add_text(f.branch, branch);

        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.add_document(doc)?;

        Ok(())
    }

    /// Commit pending changes to make them searchable.
    pub fn commit(&self) -> Result<()> {
        let mut guard = self.writer.lock().unwrap();
        if let Some(ref mut writer) = *guard {
            writer.commit().context("Failed to commit memory index")?;
        }
        self.reader
            .reload()
            .context("Failed to reload memory index reader")?;
        Ok(())
    }

    /// Remove all documents from the index.
    pub fn clear_all(&self) -> Result<()> {
        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.delete_all_documents()?;
        drop(guard);
        self.commit()?;
        Ok(())
    }

    /// Search the memory index with a natural-language query.
    ///
    /// Returns results ranked by BM25 score across body, tags, symbols,
    /// decision, and impact fields.
    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<MemorySearchResult>> {
        let f = &self.fields;

        // Build a query parser targeting the searchable TEXT fields
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![f.body, f.tags, f.symbols, f.decision, f.impact],
        );

        let query = query_parser
            .parse_query(query_str)
            .with_context(|| format!("Failed to parse memory search query: {}", query_str))?;

        let searcher = self.reader.searcher();
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            results.push(MemorySearchResult {
                id: Self::get_text_field(&doc, f.id),
                body: Self::get_text_field(&doc, f.body),
                tags: Self::get_text_field(&doc, f.tags),
                symbols: Self::get_text_field(&doc, f.symbols),
                decision: Self::get_text_field(&doc, f.decision),
                impact: Self::get_text_field(&doc, f.impact),
                branch: Self::get_text_field(&doc, f.branch),
                timestamp: Self::get_text_field(&doc, f.timestamp),
                file_path: Self::get_text_field(&doc, f.file_path),
                score,
            });
        }

        Ok(results)
    }

    /// Rebuild the index from `.memories/` checkpoint files on disk.
    ///
    /// Clears the existing index, walks all `.memories/` date directories,
    /// parses each checkpoint file, indexes it, and commits.
    pub fn rebuild_from_files(&self, workspace_root: &Path) -> Result<()> {
        // 1. Clear existing index
        self.clear_all()?;

        // 2. Check if .memories/ exists
        let memories_dir = workspace_root.join(".memories");
        if !memories_dir.exists() {
            return Ok(());
        }

        // 3. Walk date directories
        let entries = std::fs::read_dir(&memories_dir)
            .with_context(|| {
                format!(
                    "Failed to read .memories directory: {}",
                    memories_dir.display()
                )
            })?;

        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_dir() {
                continue;
            }

            let dir_name = entry.file_name().to_string_lossy().to_string();

            // Only process YYYY-MM-DD directories
            if !is_date_dir(&dir_name) {
                continue;
            }

            let date_dir = entry.path();
            self.index_date_directory(&date_dir, &dir_name)?;
        }

        // 4. Commit
        self.commit()?;

        Ok(())
    }

    // ========================================================================
    // Private helpers
    // ========================================================================

    fn get_or_create_writer(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<IndexWriter>>> {
        let mut guard = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(
                self.index
                    .writer(WRITER_HEAP_SIZE)
                    .context("Failed to create memory index writer")?,
            );
        }
        Ok(guard)
    }

    /// Index all checkpoint files in a single date directory.
    fn index_date_directory(&self, date_dir: &Path, date_name: &str) -> Result<()> {
        let entries = std::fs::read_dir(date_dir).with_context(|| {
            format!(
                "Failed to read date directory: {}",
                date_dir.display()
            )
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Only process .md files
            match path.extension().and_then(|e| e.to_str()) {
                Some("md") => {}
                _ => continue,
            }

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        "Skipping unreadable checkpoint file {}: {}",
                        path.display(),
                        e
                    );
                    continue;
                }
            };

            match parse_checkpoint(&content) {
                Ok(checkpoint) => {
                    // Construct relative file_path: "YYYY-MM-DD/filename.md"
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown.md");
                    let rel_path = format!("{}/{}", date_name, filename);

                    if let Err(e) = self.add_checkpoint(&checkpoint, Some(&rel_path)) {
                        tracing::warn!(
                            "Failed to index checkpoint {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Skipping malformed checkpoint file {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        Ok(())
    }

    fn get_text_field(doc: &TantivyDocument, field: Field) -> String {
        doc.get_first(field)
            .and_then(|v| match v {
                OwnedValue::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }
}

/// Check if a directory name matches YYYY-MM-DD format.
fn is_date_dir(name: &str) -> bool {
    if name.len() != 10 {
        return false;
    }
    let bytes = name.as_bytes();
    // Quick structural check: YYYY-MM-DD
    bytes[4] == b'-' && bytes[7] == b'-' && bytes.iter().enumerate().all(|(i, &b)| {
        if i == 4 || i == 7 {
            true // dashes already checked
        } else {
            b.is_ascii_digit()
        }
    })
}
