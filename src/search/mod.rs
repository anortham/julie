//! Tantivy-based code search engine.
//!
//! Code-aware full-text search using custom tokenization that understands
//! CamelCase, snake_case, and language-specific operators.

mod error;
pub mod index;
pub mod language_config;
pub mod query;
pub mod schema;
pub mod scoring;
pub mod tokenizer;

pub use error::{Result, SearchError};
pub use index::{
    ContentSearchResult, ContentSearchResults, FileDocument, SearchFilter, SearchIndex,
    SymbolDocument, SymbolSearchResult, SymbolSearchResults,
};
pub use language_config::LanguageConfigs;
pub use schema::{create_schema, SchemaFields};
pub use tokenizer::CodeTokenizer;
