//! Tantivy-based code search engine.
//!
//! Code-aware full-text search using custom tokenization that understands
//! CamelCase, snake_case, and language-specific operators.

pub mod debug;
mod error;
pub mod expansion;
pub mod hybrid;
pub mod index;
pub mod language_config;
pub mod query;
pub mod schema;
pub mod scoring;
pub mod tokenizer;
pub mod weights;

pub use error::{Result, SearchError};
pub use index::{
    ContentSearchResult, ContentSearchResults, FileDocument, SearchFilter, SearchIndex,
    SymbolDocument, SymbolSearchResult, SymbolSearchResults,
};
pub use language_config::LanguageConfigs;
pub use schema::{SchemaFields, create_schema};
pub use tokenizer::CodeTokenizer;
