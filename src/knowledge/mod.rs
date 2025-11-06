/// Knowledge module - RAG documentation embeddings and semantic search
///
/// This module implements the knowledge base layer for Julie's RAG capabilities,
/// enabling semantic search across documentation, code, tests, and ADRs.

pub mod doc_indexer;

pub use doc_indexer::DocumentationIndexer;
