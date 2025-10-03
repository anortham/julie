mod indexing;
mod queries;
mod result;
#[cfg(test)]
mod tests;
mod utils;
mod writer;

pub use result::SearchResult;
pub use writer::SearchIndexWriter;

use anyhow::Result;
use std::path::Path;
use tantivy::directory::MmapDirectory;
use tantivy::tokenizer::{LowerCaser, TextAnalyzer};
use tantivy::{Index, IndexReader};

use super::schema::{CodeSearchSchema, LanguageBoosting, QueryProcessor};
use super::tokenizers::CodeTokenizer;

fn register_code_tokenizers(index: &Index) -> Result<()> {
    let tokenizer_manager = index.tokenizers();

    tokenizer_manager.register(
        "code_aware",
        TextAnalyzer::builder(CodeTokenizer::default())
            .filter(LowerCaser)
            .build(),
    );

    Ok(())
}

/// Main search engine implementing the Search Accelerator pillar
///
/// This struct is READ-ONLY and only handles search operations.
/// All write operations (indexing, commits, deletions) are handled by SearchIndexWriter.
/// This separation eliminates RwLock contention - searches can proceed during background indexing.
pub struct SearchEngine {
    index: Index,
    schema: CodeSearchSchema,
    reader: IndexReader,
    query_processor: QueryProcessor,
    language_boosting: LanguageBoosting,
}

impl SearchEngine {
    /// Create a new search engine with the given index path
    ///
    /// This creates a READ-ONLY search engine. For write operations, use SearchIndexWriter.
    pub fn new<P: AsRef<Path>>(index_path: P) -> Result<Self> {
        let schema = CodeSearchSchema::new()?;
        let directory = MmapDirectory::open(index_path.as_ref())?;
        let index = Index::open_or_create(directory, schema.schema().clone())?;

        register_code_tokenizers(&index)?;

        let reader = index.reader()?;
        // CRITICAL: Reload reader to see existing index segments
        // Without this, get_indexed_document_count() returns 0 even for existing indexes
        // Note: reload() is called on &IndexReader, not &mut, so no mut needed
        reader.reload()?;

        let query_processor = QueryProcessor::new()?;
        let language_boosting = LanguageBoosting::new();

        Ok(Self {
            index,
            schema,
            reader,
            query_processor,
            language_boosting,
        })
    }

    /// Create a search engine in RAM for testing
    ///
    /// This creates a READ-ONLY search engine. For write operations, use SearchIndexWriter.
    pub fn in_memory() -> Result<Self> {
        let schema = CodeSearchSchema::new()?;
        let index = Index::create_in_ram(schema.schema().clone());

        register_code_tokenizers(&index)?;

        let reader = index.reader()?;
        let query_processor = QueryProcessor::new()?;
        let language_boosting = LanguageBoosting::new();

        Ok(Self {
            index,
            schema,
            reader,
            query_processor,
            language_boosting,
        })
    }

    /// Get the total number of documents in the Tantivy index
    /// Used to check if the search index has been populated
    pub fn get_indexed_document_count(&self) -> Result<u64> {
        let searcher = self.reader.searcher();
        let segment_readers = searcher.segment_readers();

        let total_docs: u64 = segment_readers
            .iter()
            .map(|reader| reader.num_docs() as u64)
            .sum();

        Ok(total_docs)
    }

    /// Reload the reader to see changes committed by SearchIndexWriter
    ///
    /// This MUST be called after SearchIndexWriter.commit() to make new changes visible to searches.
    /// This is a fast operation - Tantivy uses MVCC snapshots internally.
    pub fn reload_reader(&mut self) -> Result<()> {
        self.reader.reload()?;
        Ok(())
    }

    /// Get the underlying Index (needed for creating SearchIndexWriter)
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Get the schema (needed for creating SearchIndexWriter)
    pub fn schema(&self) -> &CodeSearchSchema {
        &self.schema
    }
}
