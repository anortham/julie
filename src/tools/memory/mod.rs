// Memory System - Project-level development memories
//
// Stores checkpoints, decisions, and learnings as markdown files with YAML
// frontmatter in `.memories/` organized by date. Memories are git-trackable,
// human-readable, and browsable on GitHub.
//
// Key principles:
// - **Minimal Core Schema**: Only 3 required fields (id, timestamp, type)
// - **Flexible Schema**: Type-specific fields via serde flatten
// - **Markdown + YAML Frontmatter**: Human-readable, GitHub-browsable
// - **Backward Compatible**: Reads both legacy .json and new .md files
// - **Immutable First**: Append-only semantics

// MCP Tools (Phase 1)
mod checkpoint;
mod recall;

// MCP Tools (Phase 1.5)
pub mod plan; // Public module to expose plan functions and types
mod plan_tool;

// Re-export tools for external use
pub use checkpoint::CheckpointTool;
pub use plan_tool::{PlanAction, PlanTool};
pub use recall::RecallTool;

use anyhow::{Context, Result};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{
    IndexRecordOption, OwnedValue, Schema, TextFieldIndexing, TextOptions, STORED, STRING,
};
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{Index, TantivyDocument};

use crate::search::tokenizer::CodeTokenizer;

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

/// Format a memory as markdown with YAML frontmatter
///
/// Output format:
/// ```markdown
/// ---
/// git:
///   branch: main
///   commit: abc123
///   dirty: true
///   files_changed:
///   - src/foo.rs
/// id: checkpoint_abc123_def456
/// tags:
/// - bugfix
/// - search
/// timestamp: 1770316749
/// type: checkpoint
/// ---
///
/// Description text goes here as the markdown body.
/// ```
fn format_as_markdown(memory: &Memory) -> String {
    let mut fm = String::from("---\n");

    // Git context
    if let Some(ref git) = memory.git {
        fm.push_str("git:\n");
        fm.push_str(&format!("  branch: {}\n", git.branch));
        fm.push_str(&format!("  commit: {}\n", git.commit));
        fm.push_str(&format!("  dirty: {}\n", git.dirty));
        if let Some(ref files) = git.files_changed {
            fm.push_str("  files_changed:\n");
            for f in files {
                fm.push_str(&format!("  - {}\n", f));
            }
        }
    }

    // Core fields (alphabetical to match miller convention)
    fm.push_str(&format!("id: {}\n", memory.id));

    // Tags from extra
    if let Some(tags) = memory.extra.get("tags").and_then(|v| v.as_array()) {
        if !tags.is_empty() {
            fm.push_str("tags:\n");
            for tag in tags {
                if let Some(t) = tag.as_str() {
                    fm.push_str(&format!("- {}\n", t));
                }
            }
        }
    }

    fm.push_str(&format!("timestamp: {}\n", memory.timestamp));
    fm.push_str(&format!("type: {}\n", memory.memory_type));
    fm.push_str("---\n\n");

    // Body = description
    let description = memory
        .extra
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    format!("{}{}\n", fm, description)
}

/// Save a memory to disk as markdown with YAML frontmatter
///
/// Directory structure: `.memories/YYYY-MM-DD/HHMMSS_xxxx.md`
pub fn save_memory(workspace_root: &Path, memory: &Memory) -> Result<PathBuf> {
    // Create memories base directory
    let memories_dir = workspace_root.join(".memories");
    fs::create_dir_all(&memories_dir).context("Failed to create memories directory")?;

    // Create date-based subdirectory (YYYY-MM-DD)
    let timestamp = DateTime::from_timestamp(memory.timestamp, 0).context("Invalid timestamp")?;
    let date_str = timestamp.format("%Y-%m-%d").to_string();
    let date_dir = memories_dir.join(&date_str);
    fs::create_dir_all(&date_dir)
        .context(format!("Failed to create date directory: {}", date_str))?;

    // Generate filename: HHMMSS_xxxx.md
    let time_str = timestamp.format("%H%M%S").to_string();
    let random_suffix = generate_random_hex(4);
    let filename = format!("{}_{}.md", time_str, random_suffix);
    let file_path = date_dir.join(&filename);

    // Atomic write: write to temp file, then rename
    let temp_path = date_dir.join(format!(".{}.tmp", filename));

    // Serialize to markdown with YAML frontmatter
    let content = format_as_markdown(memory);

    // Write to temp file
    fs::write(&temp_path, &content)
        .context(format!("Failed to write temp file: {:?}", temp_path))?;

    // Atomic rename
    fs::rename(&temp_path, &file_path).context(format!(
        "Failed to rename {:?} to {:?}",
        temp_path, file_path
    ))?;

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
    for date_entry in fs::read_dir(&memories_dir).context("Failed to read memories directory")? {
        let date_entry = date_entry?;
        let date_path = date_entry.path();

        // Skip non-directories
        if !date_path.is_dir() {
            continue;
        }

        // Read all memory files (.md and legacy .json)
        for file_entry in fs::read_dir(&date_path)
            .context(format!("Failed to read date directory: {:?}", date_path))?
        {
            let file_entry = file_entry?;
            let file_path = file_entry.path();

            // Accept both .md (new) and .json (legacy) files
            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if ext != "md" && ext != "json" {
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
                    eprintln!(
                        "Warning: Failed to parse memory file {:?}: {}",
                        file_path, e
                    );
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

/// Search memories by relevance to a text query using an ephemeral Tantivy index.
///
/// Loads memories via `recall_memories`, builds an in-memory Tantivy index with
/// `CodeTokenizer` for code-aware tokenization, and returns results ranked by
/// relevance score (highest first).
///
/// # Arguments
/// * `workspace_root` - Root directory of the workspace
/// * `query_str` - Text query to search for
/// * `options` - Filtering options passed to `recall_memories`
///
/// # Returns
/// Vector of (Memory, score) tuples, sorted by relevance (highest score first).
/// If query is empty, returns all memories with score 0.0.
pub fn search_memories(
    workspace_root: &Path,
    query_str: &str,
    options: RecallOptions,
) -> Result<Vec<(Memory, f32)>> {
    let memories = recall_memories(workspace_root, options)?;

    // If no memories or empty query, return all with score 0.0
    if memories.is_empty() || query_str.trim().is_empty() {
        return Ok(memories.into_iter().map(|m| (m, 0.0)).collect());
    }

    // Build schema: id (stored, exact match) + body (text, code-tokenized)
    let mut schema_builder = Schema::builder();

    let code_text_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("code")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let id_field = schema_builder.add_text_field("id", STRING | STORED);
    let body_field = schema_builder.add_text_field("body", code_text_options);
    let schema = schema_builder.build();

    // Create in-memory index
    let index = Index::create_in_ram(schema);

    // Register CodeTokenizer as "code" (matching the schema's tokenizer name)
    index
        .tokenizers()
        .register("code", TextAnalyzer::builder(CodeTokenizer::new(vec![])).build());

    // Index each memory
    let mut writer = index
        .writer(15_000_000)
        .context("Failed to create Tantivy writer")?;

    // Build a lookup map: id -> Memory
    let mut memory_map: HashMap<String, Memory> = HashMap::with_capacity(memories.len());

    for memory in memories {
        // Compose searchable body: description + tags + memory_type
        let description = memory
            .extra
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let tags = memory
            .extra
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        let body = format!("{} {} {}", description, tags, memory.memory_type);

        let mut doc = TantivyDocument::new();
        doc.add_text(id_field, &memory.id);
        doc.add_text(body_field, &body);
        writer.add_document(doc)?;

        memory_map.insert(memory.id.clone(), memory);
    }

    writer.commit().context("Failed to commit Tantivy index")?;

    // Search
    let reader = index.reader().context("Failed to open Tantivy reader")?;
    let searcher = reader.searcher();

    let query_parser = QueryParser::for_index(&index, vec![body_field]);
    let query = query_parser
        .parse_query(query_str)
        .unwrap_or_else(|_| {
            // Fallback: treat as a single term query on body field
            let term = tantivy::Term::from_field_text(body_field, query_str);
            Box::new(tantivy::query::TermQuery::new(
                term,
                IndexRecordOption::WithFreqsAndPositions,
            ))
        });

    let top_docs = searcher
        .search(&query, &TopDocs::with_limit(memory_map.len()))
        .context("Tantivy search failed")?;

    // Map results back to Memory structs
    let mut results: Vec<(Memory, f32)> = Vec::new();
    for (score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher.doc(doc_address)?;
        if let Some(OwnedValue::Str(id_str)) = doc.get_first(id_field) {
            if let Some(memory) = memory_map.remove(id_str.as_str()) {
                results.push((memory, score));
            }
        }
    }

    Ok(results)
}

/// Read and parse a single memory file (.md or legacy .json)
pub fn read_memory_file(path: &Path) -> Result<Memory> {
    let content =
        fs::read_to_string(path).context(format!("Failed to read memory file: {:?}", path))?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => parse_markdown_memory(&content)
            .context(format!("Failed to parse markdown memory: {:?}", path)),
        _ => Memory::from_json(&content)
            .context(format!("Failed to parse memory JSON: {:?}", path)),
    }
}

/// Parse a markdown memory file with YAML frontmatter
///
/// Expected format:
/// ```text
/// ---
/// git:
///   branch: main
///   commit: abc123
/// id: checkpoint_xxx
/// tags:
/// - tag1
/// timestamp: 1234567890
/// type: checkpoint
/// ---
///
/// Description body text here.
/// ```
fn parse_markdown_memory(content: &str) -> Result<Memory> {
    // Split on frontmatter delimiters
    let content = content.trim_start();
    if !content.starts_with("---") {
        anyhow::bail!("Missing frontmatter delimiter");
    }

    // Find the closing ---
    let after_first = &content[3..].trim_start_matches('\n');
    let end_idx = after_first
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("Missing closing frontmatter delimiter"))?;

    let frontmatter = &after_first[..end_idx];
    let body = after_first[end_idx + 4..].trim().to_string();

    // Parse frontmatter fields
    let mut id = String::new();
    let mut timestamp: i64 = 0;
    let mut memory_type = String::from("checkpoint");
    let mut tags: Vec<String> = Vec::new();
    let mut git_branch = String::new();
    let mut git_commit = String::new();
    let mut git_dirty = false;
    let mut git_files: Vec<String> = Vec::new();
    let mut has_git = false;

    // Track current parsing context for nested YAML
    let mut in_git = false;
    let mut in_tags = false;
    let mut in_files_changed = false;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check indentation level to determine context
        let indent = line.len() - line.trim_start().len();

        if indent == 0 && !trimmed.starts_with('-') {
            // Top-level key — reset nested context
            in_tags = false;
            in_files_changed = false;
            in_git = false;

            if let Some(val) = trimmed.strip_prefix("id: ") {
                id = val.to_string();
            } else if let Some(val) = trimmed.strip_prefix("timestamp: ") {
                timestamp = val.parse().unwrap_or(0);
            } else if let Some(val) = trimmed.strip_prefix("type: ") {
                memory_type = val.to_string();
            } else if trimmed == "tags:" {
                in_tags = true;
            } else if trimmed == "git:" {
                in_git = true;
                has_git = true;
            }
        } else if in_tags && trimmed.starts_with('-') {
            // Tag list item
            let tag = trimmed.trim_start_matches('-').trim();
            if !tag.is_empty() {
                tags.push(tag.to_string());
            }
        } else if in_git && indent >= 2 {
            // Git nested fields
            let field = trimmed;
            if let Some(val) = field.strip_prefix("branch: ") {
                git_branch = val.to_string();
            } else if let Some(val) = field.strip_prefix("commit: ") {
                git_commit = val.to_string();
            } else if let Some(val) = field.strip_prefix("dirty: ") {
                git_dirty = val == "true";
            } else if field == "files_changed:" {
                in_files_changed = true;
            } else if in_files_changed && field.starts_with('-') {
                let file = field.trim_start_matches('-').trim();
                if !file.is_empty() {
                    git_files.push(file.to_string());
                }
            }
        }
    }

    if id.is_empty() {
        anyhow::bail!("Missing required 'id' field in frontmatter");
    }

    // Build git context
    let git = if has_git {
        Some(GitContext {
            branch: git_branch,
            commit: git_commit,
            dirty: git_dirty,
            files_changed: if git_files.is_empty() {
                None
            } else {
                Some(git_files)
            },
        })
    } else {
        None
    };

    // Build extra fields (description + tags)
    let mut extra_map = serde_json::Map::new();
    if !body.is_empty() {
        extra_map.insert("description".to_string(), serde_json::json!(body));
    }
    if !tags.is_empty() {
        extra_map.insert("tags".to_string(), serde_json::json!(tags));
    }

    Ok(Memory {
        id,
        timestamp,
        memory_type,
        git,
        extra: Value::Object(extra_map),
    })
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
            )
            .with_extra(json!({
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
        assert_eq!(
            recalled[0].id, "memory_2",
            "First memory should be memory_2 (oldest of newest 3), but got {}",
            recalled[0].id
        );
        assert_eq!(
            recalled[1].id, "memory_3",
            "Second memory should be memory_3, but got {}",
            recalled[1].id
        );
        assert_eq!(
            recalled[2].id, "memory_4",
            "Third memory should be memory_4 (newest overall), but got {}",
            recalled[2].id
        );
    }

    #[test]
    fn test_format_as_markdown() {
        let memory = Memory::new(
            "checkpoint_abc123_def456".to_string(),
            1770316749,
            "checkpoint".to_string(),
        )
        .with_git(GitContext {
            branch: "main".to_string(),
            commit: "1a33684".to_string(),
            dirty: true,
            files_changed: Some(vec![
                "src/search/index.rs".to_string(),
                "src/tools/workspace/discovery.rs".to_string(),
            ]),
        })
        .with_extra(json!({
            "description": "Fixed content search quality by removing .memories from discovery.",
            "tags": ["bugfix", "search-quality"]
        }));

        let md = format_as_markdown(&memory);

        // Verify frontmatter structure
        assert!(md.starts_with("---\n"), "Should start with frontmatter delimiter");
        assert!(md.contains("\n---\n"), "Should have closing frontmatter delimiter");
        assert!(md.contains("id: checkpoint_abc123_def456"));
        assert!(md.contains("timestamp: 1770316749"));
        assert!(md.contains("type: checkpoint"));

        // Verify git context
        assert!(md.contains("git:\n"));
        assert!(md.contains("  branch: main"));
        assert!(md.contains("  commit: 1a33684"));
        assert!(md.contains("  dirty: true"));
        assert!(md.contains("  - src/search/index.rs"));

        // Verify tags
        assert!(md.contains("tags:\n"));
        assert!(md.contains("- bugfix"));
        assert!(md.contains("- search-quality"));

        // Verify body
        assert!(md.contains("Fixed content search quality"));
    }

    #[test]
    fn test_parse_markdown_memory() {
        let md = "\
---
git:
  branch: main
  commit: abc123
  dirty: true
  files_changed:
  - src/foo.rs
  - src/bar.rs
id: checkpoint_test_123456
tags:
- bugfix
- auth
timestamp: 1770316749
type: decision
---

This is the description body.
It can span multiple lines.";

        let memory = parse_markdown_memory(md).expect("Should parse successfully");

        assert_eq!(memory.id, "checkpoint_test_123456");
        assert_eq!(memory.timestamp, 1770316749);
        assert_eq!(memory.memory_type, "decision");

        // Check git
        let git = memory.git.expect("Should have git context");
        assert_eq!(git.branch, "main");
        assert_eq!(git.commit, "abc123");
        assert!(git.dirty);
        assert_eq!(
            git.files_changed.as_ref().unwrap(),
            &vec!["src/foo.rs".to_string(), "src/bar.rs".to_string()]
        );

        // Check extra fields
        let desc = memory.extra.get("description").unwrap().as_str().unwrap();
        assert!(desc.contains("This is the description body."));
        assert!(desc.contains("It can span multiple lines."));

        let tags: Vec<&str> = memory
            .extra
            .get("tags")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(tags, vec!["bugfix", "auth"]);
    }

    #[test]
    fn test_roundtrip_markdown() {
        // Write a memory, then parse it back — should be equivalent
        let memory = Memory::new(
            "checkpoint_round_trip".to_string(),
            1234567890,
            "learning".to_string(),
        )
        .with_git(GitContext {
            branch: "feature".to_string(),
            commit: "deadbeef".to_string(),
            dirty: false,
            files_changed: None,
        })
        .with_extra(json!({
            "description": "Learned something important about testing.",
            "tags": ["testing", "tdd"]
        }));

        let md = format_as_markdown(&memory);
        let parsed = parse_markdown_memory(&md).expect("Should roundtrip cleanly");

        assert_eq!(parsed.id, memory.id);
        assert_eq!(parsed.timestamp, memory.timestamp);
        assert_eq!(parsed.memory_type, memory.memory_type);
        assert_eq!(parsed.git.as_ref().unwrap().branch, "feature");
        assert_eq!(parsed.git.as_ref().unwrap().commit, "deadbeef");
        assert!(!parsed.git.as_ref().unwrap().dirty);

        let desc = parsed.extra.get("description").unwrap().as_str().unwrap();
        assert_eq!(desc, "Learned something important about testing.");
    }

    #[test]
    fn test_save_memory_writes_markdown_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let memory = Memory::new(
            "checkpoint_save_test".to_string(),
            1770316749,
            "checkpoint".to_string(),
        )
        .with_extra(json!({
            "description": "Test save creates .md file",
            "tags": ["test"]
        }));

        let path = save_memory(temp_dir.path(), &memory).expect("Should save");

        // Verify .md extension
        assert_eq!(
            path.extension().unwrap().to_str().unwrap(),
            "md",
            "Should write .md file, not .json"
        );

        // Verify content is valid markdown with frontmatter
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("id: checkpoint_save_test"));
        assert!(content.contains("Test save creates .md file"));
    }

    #[test]
    fn test_recall_reads_mixed_formats() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let memories_dir = temp_dir.path().join(".memories").join("2025-11-14");
        std::fs::create_dir_all(&memories_dir).unwrap();

        // Write a legacy JSON memory
        let json_memory = Memory::new("json_mem".to_string(), 1000000, "checkpoint".to_string())
            .with_extra(json!({"description": "JSON format", "tags": []}));
        let json_content = serde_json::to_string_pretty(&json_memory).unwrap();
        std::fs::write(memories_dir.join("100000_aaaa.json"), json_content).unwrap();

        // Write a new markdown memory
        let md_memory = Memory::new("md_mem".to_string(), 1000100, "checkpoint".to_string())
            .with_extra(json!({"description": "Markdown format", "tags": ["new"]}));
        let md_content = format_as_markdown(&md_memory);
        std::fs::write(memories_dir.join("100100_bbbb.md"), md_content).unwrap();

        // Recall should find both
        let recalled = recall_memories(
            temp_dir.path(),
            RecallOptions {
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(recalled.len(), 2, "Should read both .json and .md files");

        let ids: Vec<&str> = recalled.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"json_mem"), "Should include JSON memory");
        assert!(ids.contains(&"md_mem"), "Should include markdown memory");
    }
}
