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
    /// Full file content for Tantivy search
    pub content: Option<String>,
}

/// Database statistics for health monitoring
#[derive(Debug, Default)]
pub struct DatabaseStats {
    pub total_symbols: i64,
    pub total_relationships: i64,
    pub total_files: i64,
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
