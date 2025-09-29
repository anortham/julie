mod indexing;
mod queries;
mod result;
#[cfg(test)]
mod tests;
mod utils;

pub use result::SearchResult;

use anyhow::Result;
use std::path::Path;
use tantivy::directory::MmapDirectory;
use tantivy::tokenizer::{LowerCaser, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter};

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
pub struct SearchEngine {
    index: Index,
    schema: CodeSearchSchema,
    reader: IndexReader,
    writer: IndexWriter,
    query_processor: QueryProcessor,
    language_boosting: LanguageBoosting,
}

impl SearchEngine {
    /// Create a new search engine with the given index path
    pub fn new<P: AsRef<Path>>(index_path: P) -> Result<Self> {
        let schema = CodeSearchSchema::new()?;
        let directory = MmapDirectory::open(index_path.as_ref())?;
        let index = Index::open_or_create(directory, schema.schema().clone())?;

        register_code_tokenizers(&index)?;

        let reader = index.reader()?;
        let writer = index.writer(50_000_000)?; // 50MB heap
        let query_processor = QueryProcessor::new()?;
        let language_boosting = LanguageBoosting::new();

        Ok(Self {
            index,
            schema,
            reader,
            writer,
            query_processor,
            language_boosting,
        })
    }

    /// Create a search engine in RAM for testing
    pub fn in_memory() -> Result<Self> {
        let schema = CodeSearchSchema::new()?;
        let index = Index::create_in_ram(schema.schema().clone());

        register_code_tokenizers(&index)?;

        let reader = index.reader()?;
        let writer = index.writer(15_000_000)?; // 15MB heap minimum for testing
        let query_processor = QueryProcessor::new()?;
        let language_boosting = LanguageBoosting::new();

        Ok(Self {
            index,
            schema,
            reader,
            writer,
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
}
