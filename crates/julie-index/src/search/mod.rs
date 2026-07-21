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
pub mod projection;
pub mod query;
pub mod query_parse;
pub mod reranker;
pub mod schema;
pub mod scoring;
pub mod similarity;
pub mod tokenizer;
pub mod weights;

pub use error::{Result, SearchError};
#[cfg(test)]
pub use index::{ContentSearchResult, ContentSearchResults};
pub use index::{
    SearchDocument, SearchFilter, SearchIndex, SearchIndexHandle, SymbolSearchResult,
    SymbolSearchResults,
};
pub use language_config::LanguageConfigs;
pub use projection::SearchProjection;
pub use schema::{SchemaFields, create_schema};
pub use tokenizer::{CodeTokenizer, SimpleCodeTokenizer, pretokenize_code};
