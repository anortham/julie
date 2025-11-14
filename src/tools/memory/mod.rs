// Memory System - Project-level development memories
//
// Stores checkpoints, decisions, and learnings as pretty-printed JSON files
// in `.memories/` organized by date. Memories are automatically indexed
// by Julie's existing tree-sitter pipeline for searchability.
//
// Key principles:
// - **Minimal Core Schema**: Only 3 required fields (id, timestamp, type)
// - **Flexible Schema**: Type-specific fields via serde flatten
// - **Git-Trackable**: Individual JSON files, human-readable
// - **Immutable First**: Phase 1 focuses on append-only semantics

// MCP Tools (Phase 1)
mod checkpoint;
mod recall;

// MCP Tools (Phase 1.5)
pub mod plan; // Public module to expose plan functions and types
mod plan_tool;

// Re-export tools for external use
pub use checkpoint::CheckpointTool;
pub use recall::RecallTool;
pub use plan_tool::{PlanAction, PlanTool};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use chrono::DateTime;

/// Git context captured at memory creation time
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitContext {
    /// Current git branch
    pub branch: String,

    /// Current commit hash (short or full)
    pub commit: String,

    /// Whether working directory is dirty
    pub dirty: bool,

    /// List of changed files (optional, can be expensive to compute)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_changed: Option<Vec<String>>,
}

/// Core memory structure with flexible schema
///
/// Required fields:
/// - `id`: Unique identifier (format: "{type}_{timestamp}_{random}")
/// - `timestamp`: Unix timestamp (used for chronological ordering)
/// - `type`: Memory type (checkpoint, decision, learning, etc.)
///
/// Optional common fields:
/// - `git`: Git context (branch, commit, dirty status)
///
/// Type-specific fields:
/// - Everything else is stored in `extra` via serde flatten
/// - Examples:
///   - checkpoint: description, tags
///   - decision: question, chosen, alternatives, rationale
///   - learning: insight, context
///
/// # Examples
///
/// ```json
/// {
///   "id": "mem_1234567890_abc",
///   "timestamp": 1234567890,
///   "type": "checkpoint",
///   "description": "Fixed auth bug",
///   "tags": ["bug", "auth"],
///   "git": {
///     "branch": "main",
///     "commit": "abc123",
///     "dirty": false
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique memory identifier
    pub id: String,

    /// Unix timestamp (seconds since epoch)
    pub timestamp: i64,

    /// Memory type (checkpoint, decision, learning, etc.)
    #[serde(rename = "type")]
    pub memory_type: String,

    /// Optional git context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitContext>,

    /// All other fields flattened at top level
    /// This enables flexible, type-specific schemas
    #[serde(flatten)]
    pub extra: Value,
}

impl Memory {
    /// Create a new memory with minimal required fields
    pub fn new(id: String, timestamp: i64, memory_type: String) -> Self {
        Self {
            id,
            timestamp,
            memory_type,
            git: None,
            extra: Value::Object(serde_json::Map::new()),
        }
    }

    /// Create a memory with git context
    pub fn with_git(mut self, git: GitContext) -> Self {
        self.git = Some(git);
        self
    }

    /// Add extra fields (type-specific data)
    pub fn with_extra(mut self, extra: Value) -> Self {
        self.extra = extra;
        self
    }

    /// Serialize to pretty-printed JSON string
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Save a memory to disk in the appropriate directory structure
///
/// Directory structure: `.memories/YYYY-MM-DD/HHMMSS_xxxx.json`
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `memory` - Memory to save
///
/// # Returns
/// Path to the saved memory file
///
/// # Example
/// ```no_run
/// # use std::path::PathBuf;
/// # use crate::tools::memory::*;
/// let memory = Memory::new("mem_123".into(), 1234567890, "checkpoint".into());
/// let path = save_memory(&PathBuf::from("."), &memory)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn save_memory(workspace_root: &Path, memory: &Memory) -> Result<PathBuf> {
    // Create memories base directory
    let memories_dir = workspace_root.join(".memories");
    fs::create_dir_all(&memories_dir)
        .context("Failed to create memories directory")?;

    // Create date-based subdirectory (YYYY-MM-DD)
    let timestamp = DateTime::from_timestamp(memory.timestamp, 0)
        .context("Invalid timestamp")?;
    let date_str = timestamp.format("%Y-%m-%d").to_string();
    let date_dir = memories_dir.join(&date_str);
    fs::create_dir_all(&date_dir)
        .context(format!("Failed to create date directory: {}", date_str))?;

    // Generate filename: HHMMSS_xxxx.json
    let time_str = timestamp.format("%H%M%S").to_string();
    let random_suffix = generate_random_hex(4);
    let filename = format!("{}_{}.json", time_str, random_suffix);
    let file_path = date_dir.join(&filename);

    // Atomic write: write to temp file, then rename
    let temp_path = date_dir.join(format!(".{}.tmp", filename));

    // Serialize to pretty-printed JSON
    let json = memory.to_pretty_json()
        .context("Failed to serialize memory to JSON")?;

    // Write to temp file
    fs::write(&temp_path, json)
        .context(format!("Failed to write temp file: {:?}", temp_path))?;

    // Atomic rename
    fs::rename(&temp_path, &file_path)
        .context(format!("Failed to rename {:?} to {:?}", temp_path, file_path))?;

    Ok(file_path)
}

/// Generate a random hexadecimal string of specified length
///
/// Uses first N characters of a UUID v4 for randomness
fn generate_random_hex(length: usize) -> String {
    let uuid = uuid::Uuid::new_v4();
    let hex = uuid.simple().to_string();
    hex[..length.min(hex.len())].to_string()
}

/// Options for filtering memories during recall
#[derive(Debug, Clone, Default)]
pub struct RecallOptions {
    /// Filter by memory type (checkpoint, decision, learning, etc.)
    pub memory_type: Option<String>,

    /// Return memories since this timestamp (inclusive)
    pub since: Option<i64>,

    /// Return memories until this timestamp (inclusive)
    pub until: Option<i64>,

    /// Maximum number of results to return
    pub limit: Option<usize>,
}

/// Recall memories from disk with optional filtering
///
/// Reads memory files from `.memories/`, parses them, and returns
/// them in chronological order (oldest first).
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `options` - Filtering options (type, date range, limit)
///
/// # Returns
/// Vector of memories, sorted by timestamp (oldest first)
///
/// # Example
/// ```no_run
/// # use std::path::PathBuf;
/// # use crate::tools::memory::*;
/// let options = RecallOptions {
///     memory_type: Some("checkpoint".into()),
///     limit: Some(10),
///     ..Default::default()
/// };
/// let memories = recall_memories(&PathBuf::from("."), options)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn recall_memories(workspace_root: &Path, options: RecallOptions) -> Result<Vec<Memory>> {
    let memories_dir = workspace_root.join(".memories");

    // If memories directory doesn't exist, return empty list
    if !memories_dir.exists() {
        return Ok(Vec::new());
    }

    let mut memories = Vec::new();

    // Walk through all date directories
    for date_entry in fs::read_dir(&memories_dir)
        .context("Failed to read memories directory")?
    {
        let date_entry = date_entry?;
        let date_path = date_entry.path();

        // Skip non-directories
        if !date_path.is_dir() {
            continue;
        }

        // Read all JSON files in this date directory
        for file_entry in fs::read_dir(&date_path)
            .context(format!("Failed to read date directory: {:?}", date_path))?
        {
            let file_entry = file_entry?;
            let file_path = file_entry.path();

            // Skip non-JSON files
            if !file_path.extension().map_or(false, |ext| ext == "json") {
                continue;
            }

            // Try to read and parse the memory
            match read_memory_file(&file_path) {
                Ok(memory) => {
                    // Apply filters
                    if let Some(ref type_filter) = options.memory_type {
                        if &memory.memory_type != type_filter {
                            continue;
                        }
                    }

                    if let Some(since) = options.since {
                        if memory.timestamp < since {
                            continue;
                        }
                    }

                    if let Some(until) = options.until {
                        if memory.timestamp > until {
                            continue;
                        }
                    }

                    memories.push(memory);
                }
                Err(e) => {
                    // Log warning but continue processing other files
                    eprintln!("Warning: Failed to parse memory file {:?}: {}", file_path, e);
                    continue;
                }
            }
        }
    }

    // Sort by timestamp (chronological order)
    memories.sort_by_key(|m| m.timestamp);

    // Reverse to get newest first, THEN apply limit to keep the N most recent
    // (Note: caller in recall.rs will reverse again for display)
    memories.reverse();

    // Apply limit if specified - now keeps the N most recent
    if let Some(limit) = options.limit {
        memories.truncate(limit);
    }

    // Reverse back to chronological order for caller
    memories.reverse();

    Ok(memories)
}

/// Read and parse a single memory file
fn read_memory_file(path: &Path) -> Result<Memory> {
    let content = fs::read_to_string(path)
        .context(format!("Failed to read memory file: {:?}", path))?;

    Memory::from_json(&content)
        .context(format!("Failed to parse memory JSON: {:?}", path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_memory_builder() {
        let memory = Memory::new(
            "mem_test_123".to_string(),
            1234567890,
            "checkpoint".to_string(),
        )
        .with_git(GitContext {
            branch: "main".to_string(),
            commit: "abc123".to_string(),
            dirty: false,
            files_changed: None,
        })
        .with_extra(json!({
            "description": "Test checkpoint",
            "tags": ["test"]
        }));

        assert_eq!(memory.id, "mem_test_123");
        assert_eq!(memory.timestamp, 1234567890);
        assert_eq!(memory.memory_type, "checkpoint");
        assert!(memory.git.is_some());
    }

    #[test]
    fn test_generate_random_hex() {
        let hex = generate_random_hex(4);
        assert_eq!(hex.len(), 4);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_recall_memories_returns_most_recent_with_limit() {
        use std::fs;
        use tempfile::TempDir;

        // Create temp workspace with .memories directory
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();
        let memories_dir = workspace_root.join(".memories").join("2025-11-14");
        fs::create_dir_all(&memories_dir).unwrap();

        // Create 5 memories with incrementing timestamps
        // We'll verify that recall(limit=3) returns the 3 NEWEST, not 3 oldest
        let base_ts = 1000000;

        for i in 0..5 {
            let memory = Memory::new(
                format!("memory_{}", i),
                base_ts + (i as i64 * 100), // 1000000, 1000100, 1000200, 1000300, 1000400
                "checkpoint".to_string(),
            ).with_extra(json!({
                "description": format!("Memory number {}", i),
                "tags": []
            }));

            let file_path = memories_dir.join(format!("mem_{}.json", i));
            let json = serde_json::to_string_pretty(&memory).unwrap();
            fs::write(&file_path, json).unwrap();
        }

        // Recall with limit=3 should return the 3 MOST RECENT (indices 2, 3, 4)
        let options = RecallOptions {
            memory_type: None,
            since: None,
            until: None,
            limit: Some(3),
        };

        let recalled = recall_memories(workspace_root, options).unwrap();

        // Should return exactly 3 memories
        assert_eq!(recalled.len(), 3, "Should return exactly 3 memories");

        // recall_memories returns in CHRONOLOGICAL order (oldest first)
        // With limit=3, it should return the 3 NEWEST in chronological order
        // So: [memory_2 (oldest of newest 3), memory_3, memory_4 (newest)]
        assert_eq!(recalled[0].id, "memory_2",
            "First memory should be memory_2 (oldest of newest 3), but got {}", recalled[0].id);
        assert_eq!(recalled[1].id, "memory_3",
            "Second memory should be memory_3, but got {}", recalled[1].id);
        assert_eq!(recalled[2].id, "memory_4",
            "Third memory should be memory_4 (newest overall), but got {}", recalled[2].id);
    }
}
