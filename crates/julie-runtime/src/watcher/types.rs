//! Type definitions for file watcher events and statistics
//!
//! This module defines the core data structures used to represent file system
//! changes and indexing statistics.

use std::path::PathBuf;
use std::time::SystemTime;

/// Represents a file system change event
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub change_type: FileChangeType,
    pub timestamp: SystemTime,
}

/// Types of file system changes we track
#[derive(Debug, Clone)]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
    Renamed { from: PathBuf, to: PathBuf },
}

/// Statistics for incremental indexing performance
#[derive(Debug, Clone)]
pub struct IndexingStats {
    pub files_processed: u64,
    pub symbols_added: u64,
    pub symbols_updated: u64,
    pub symbols_deleted: u64,
    pub processing_time_ms: u64,
}
