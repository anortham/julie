// Database type definitions

use serde::{Deserialize, Serialize};

/// File tracking information with Blake3 hashing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub language: String,
    pub hash: String, // Blake3 hash
    pub size: i64,
    pub last_modified: i64, // Unix timestamp
    pub last_indexed: i64,  // Unix timestamp
    pub symbol_count: i32,
    /// CASCADE: Full file content for FTS5 search
    pub content: Option<String>,
}

/// Embedding metadata linking symbols to vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingInfo {
    pub symbol_id: String,
    pub vector_id: String,
    pub model_name: String,
    pub embedding_hash: String,
    pub created_at: i64,
}

/// File search result from FTS5 queries
#[derive(Debug, Clone)]
pub struct FileSearchResult {
    pub path: String,
    pub snippet: String,
    pub rank: f32,
}

/// Database statistics for health monitoring
#[derive(Debug, Default)]
pub struct DatabaseStats {
    pub total_symbols: i64,
    pub total_relationships: i64,
    pub total_files: i64,
    pub total_embeddings: i64,
    pub languages: Vec<String>,
    pub db_size_mb: f64,
}

/// Statistics returned after workspace cleanup
#[derive(Debug, Clone)]
pub struct WorkspaceCleanupStats {
    pub symbols_deleted: i64,
    pub relationships_deleted: i64,
    pub files_deleted: i64,
}

/// Usage statistics for a workspace (for LRU eviction)
#[derive(Debug, Clone)]
pub struct WorkspaceUsageStats {
    pub workspace_id: String,
    pub symbol_count: i64,
    pub file_count: i64,
    pub total_size_bytes: i64,
}
