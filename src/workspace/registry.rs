// src/workspace/registry.rs
//! Workspace Registry System for Julie
//!
//! This module provides centralized workspace metadata management for Julie's
//! multi-workspace indexing capability. Inspired by COA CodeSearch's approach
//! but adapted for Julie's Rust architecture.
//!
//! Key features:
//! - JSON-based registry with atomic operations
//! - Workspace ID generation using SHA256 hashing
//! - TTL-based expiration for reference workspaces
//! - Orphan detection and cleanup
//! - Memory caching for performance

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use sha2::{Digest, Sha256};

/// Central registry for all workspace metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRegistry {
    /// Registry format version for future migrations
    pub version: String,

    /// When this registry was last updated
    pub last_updated: u64, // Unix timestamp

    /// The primary workspace where Julie was started
    pub primary_workspace: Option<WorkspaceEntry>,

    /// All reference workspaces indexed for cross-project search
    pub reference_workspaces: HashMap<String, WorkspaceEntry>,

    /// Orphaned indexes that need cleanup
    pub orphaned_indexes: HashMap<String, OrphanedIndex>,

    /// Registry-wide configuration
    pub config: RegistryConfig,

    /// Statistics about the registry
    pub statistics: RegistryStatistics,
}

/// Configuration for workspace management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Default TTL for reference workspaces (in seconds)
    pub default_ttl_seconds: u64,

    /// Maximum total index size across all workspaces (in bytes)
    pub max_total_size_bytes: u64,

    /// Whether to automatically clean expired workspaces
    pub auto_cleanup_enabled: bool,

    /// Minimum time between cleanup operations (in seconds)
    pub cleanup_interval_seconds: u64,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            default_ttl_seconds: 7 * 24 * 60 * 60, // 7 days
            max_total_size_bytes: 500 * 1024 * 1024, // 500MB
            auto_cleanup_enabled: true,
            cleanup_interval_seconds: 60 * 60, // 1 hour
        }
    }
}

/// Statistics about the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStatistics {
    /// Total number of workspaces (primary + reference)
    pub total_workspaces: usize,

    /// Total number of orphaned indexes
    pub total_orphans: usize,

    /// Total size of all indexes in bytes
    pub total_index_size_bytes: u64,

    /// Total number of documents across all workspaces
    pub total_documents: usize,

    /// Last cleanup time
    pub last_cleanup: u64, // Unix timestamp
}

impl Default for RegistryStatistics {
    fn default() -> Self {
        Self {
            total_workspaces: 0,
            total_orphans: 0,
            total_index_size_bytes: 0,
            total_documents: 0,
            last_cleanup: current_timestamp(),
        }
    }
}

/// Entry for a registered workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    /// Computed hash ID (workspacename_hash8)
    pub id: String,

    /// Original workspace path
    pub original_path: String,

    /// Sanitized directory name for file system
    pub directory_name: String,

    /// Display name for UI purposes
    pub display_name: String,

    /// Type of workspace
    pub workspace_type: WorkspaceType,

    /// When this workspace was first indexed
    pub created_at: u64, // Unix timestamp

    /// Last time this workspace was accessed
    pub last_accessed: u64, // Unix timestamp

    /// When this workspace expires (None = never expires)
    pub expires_at: Option<u64>, // Unix timestamp

    /// Number of documents in the index
    pub document_count: usize,

    /// Size of index in bytes
    pub index_size_bytes: u64,

    /// Current status of this workspace
    pub status: WorkspaceStatus,
}

/// Type of workspace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkspaceType {
    /// Primary workspace where Julie was started (file watching enabled, never expires)
    Primary,

    /// Reference workspace for cross-project search (no file watching, can expire)
    Reference,

    /// Session-only workspace (cleared on restart)
    Session,
}

/// Status of a workspace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkspaceStatus {
    /// Workspace is active and being used
    Active,

    /// Workspace path no longer exists
    Missing,

    /// Workspace has errors
    Error,

    /// Workspace is archived/inactive
    Archived,

    /// Workspace is scheduled for deletion
    Expired,
}

/// Information about an orphaned index that needs cleanup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanedIndex {
    /// Directory name in indexes folder
    pub directory_name: String,

    /// When this orphan was first discovered
    pub discovered_at: u64, // Unix timestamp

    /// Last modified date of the index directory
    pub last_modified: u64, // Unix timestamp

    /// Reason why this index is considered orphaned
    pub reason: OrphanReason,

    /// When this index is scheduled for automatic deletion
    pub scheduled_for_deletion: u64, // Unix timestamp

    /// Size of the orphaned index in bytes
    pub size_bytes: u64,

    /// Original path that was attempted to be resolved (if known)
    pub attempted_path: Option<String>,
}

/// Reason why an index is considered orphaned
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrphanReason {
    /// No metadata found in registry
    NoRegistryEntry,

    /// Original path in registry no longer exists
    PathNotFound,

    /// Cannot resolve path from directory name
    UnresolvablePath,

    /// Manually marked as orphaned
    ManuallyMarked,

    /// Index directory is corrupted
    CorruptedIndex,
}

impl Default for WorkspaceRegistry {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            last_updated: current_timestamp(),
            primary_workspace: None,
            reference_workspaces: HashMap::new(),
            orphaned_indexes: HashMap::new(),
            config: RegistryConfig::default(),
            statistics: RegistryStatistics::default(),
        }
    }
}

impl WorkspaceEntry {
    /// Create a new workspace entry
    pub fn new(
        original_path: String,
        workspace_type: WorkspaceType,
        config: &RegistryConfig,
    ) -> Result<Self> {
        let id = generate_workspace_id(&original_path)?;
        let directory_name = id.clone(); // Use ID as directory name
        let display_name = extract_workspace_name(&original_path);

        let expires_at = match workspace_type {
            WorkspaceType::Primary => None, // Never expires
            WorkspaceType::Reference => {
                Some(current_timestamp() + config.default_ttl_seconds)
            },
            WorkspaceType::Session => {
                Some(current_timestamp() + 24 * 60 * 60) // 24 hours
            },
        };

        Ok(Self {
            id,
            original_path,
            directory_name,
            display_name,
            workspace_type,
            created_at: current_timestamp(),
            last_accessed: current_timestamp(),
            expires_at,
            document_count: 0,
            index_size_bytes: 0,
            status: WorkspaceStatus::Active,
        })
    }

    /// Check if this workspace has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            current_timestamp() > expires_at
        } else {
            false
        }
    }

    /// Check if the workspace path still exists
    pub fn path_exists(&self) -> bool {
        Path::new(&self.original_path).exists()
    }

    /// Update the last accessed time
    pub fn update_last_accessed(&mut self) {
        self.last_accessed = current_timestamp();
    }

    /// Extend the expiration time
    pub fn extend_expiration(&mut self, ttl_seconds: u64) {
        if self.workspace_type != WorkspaceType::Primary {
            self.expires_at = Some(current_timestamp() + ttl_seconds);
        }
    }
}

/// Generate a unique workspace ID from a path
/// Format: workspacename_hash8
pub fn generate_workspace_id(workspace_path: &str) -> Result<String> {
    // Normalize the path for consistent hashing
    let normalized_path = normalize_path(workspace_path)?;

    // Generate SHA256 hash
    let mut hasher = Sha256::new();
    hasher.update(normalized_path.as_bytes());
    let hash = hasher.finalize();

    // Take first 8 characters of hex hash
    let hash_str = format!("{:x}", hash);
    let hash_short = &hash_str[..8];

    // Extract workspace name and sanitize
    let workspace_name = extract_workspace_name(workspace_path);
    let safe_name = sanitize_name(&workspace_name);

    // Combine: workspacename_hash8
    Ok(format!("{}_{}", safe_name, hash_short))
}

/// Normalize a path for consistent hashing
fn normalize_path(path: &str) -> Result<String> {
    let path_buf = PathBuf::from(path);
    let canonical = path_buf.canonicalize()
        .or_else(|_| {
            // If canonicalize fails, try to get absolute path
            std::env::current_dir()
                .map(|current| current.join(&path_buf))
                .and_then(|abs| abs.canonicalize())
        })
        .unwrap_or(path_buf);

    let normalized = canonical
        .to_string_lossy()
        .to_lowercase()
        .replace('\\', "/") // Normalize separators
        .trim_end_matches('/')
        .to_string();

    Ok(normalized)
}

/// Extract workspace name from path
fn extract_workspace_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace")
        .to_string()
}

/// Sanitize name for use in file system
fn sanitize_name(name: &str) -> String {
    let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', ' ', '.'];
    let mut sanitized = name.to_lowercase();

    for ch in invalid_chars {
        sanitized = sanitized.replace(ch, "_");
    }

    // Truncate if too long (leave room for hash and underscore)
    if sanitized.len() > 50 {
        sanitized.truncate(50);
    }

    // Ensure it starts with alphanumeric
    if !sanitized.chars().next().unwrap_or('_').is_alphanumeric() {
        sanitized = format!("ws_{}", sanitized);
    }

    sanitized
}

/// Get current Unix timestamp
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_workspace_id() {
        let path = "/Users/test/project-a";
        let id = generate_workspace_id(path).unwrap();

        // Should be format: name_hash8
        assert!(id.contains('_'));
        let parts: Vec<&str> = id.split('_').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].len(), 8); // Hash should be 8 chars
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("project-a"), "project-a");
        assert_eq!(sanitize_name("Project A"), "project_a");
        assert_eq!(sanitize_name("my:project"), "my_project");
        assert_eq!(sanitize_name(""), "");
    }

    #[test]
    fn test_workspace_entry_expiration() {
        let config = RegistryConfig::default();

        // Primary workspace should never expire
        let primary = WorkspaceEntry::new(
            "/test/primary".to_string(),
            WorkspaceType::Primary,
            &config,
        ).unwrap();
        assert!(!primary.is_expired());
        assert!(primary.expires_at.is_none());

        // Reference workspace should have expiration
        let reference = WorkspaceEntry::new(
            "/test/reference".to_string(),
            WorkspaceType::Reference,
            &config,
        ).unwrap();
        assert!(!reference.is_expired()); // Should not be expired immediately
        assert!(reference.expires_at.is_some());
    }
}