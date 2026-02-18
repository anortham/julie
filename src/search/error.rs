use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("Index error: {0}")]
    IndexError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Tantivy error: {0}")]
    TantivyError(#[from] tantivy::TantivyError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Index not found at path: {0}")]
    IndexNotFound(String),

    #[error("Search index has been shut down")]
    Shutdown,
}

pub type Result<T> = std::result::Result<T, SearchError>;
