//! Checkpoint save logic — writes checkpoint files to `.memories/{date}/`.
//!
//! This module is the "write" half of the memory system. It takes a
//! `CheckpointInput`, captures git context, reads the active plan, and
//! writes a Goldfish-compatible markdown file with YAML frontmatter.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;

use super::git::get_git_context;
use super::index::MemoryIndex;
use super::storage::{format_checkpoint, generate_checkpoint_id, get_checkpoint_filename};
use super::{Checkpoint, CheckpointInput};

/// Memory index location relative to workspace root.
const MEMORY_INDEX_REL: &str = ".julie/indexes/memories/tantivy";

/// Save a checkpoint to `.memories/{YYYY-MM-DD}/{HHMMSS}_{hash}.md`.
///
/// 1. Generates a UTC timestamp
/// 2. Captures git context (branch, commit, changed files)
/// 3. Reads `.memories/.active-plan` for the current plan ID
/// 4. Extracts a summary from the description
/// 5. Builds the `Checkpoint`, formats it, writes the file
/// 6. Returns the saved `Checkpoint`
///
/// The checkpoint ID is deterministic: `checkpoint_{SHA256(timestamp:description)[..8]}`.
pub async fn save_checkpoint(workspace_root: &Path, input: CheckpointInput) -> Result<Checkpoint> {
    // 1. Get current timestamp (UTC, ISO 8601 with milliseconds)
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // 2. Generate deterministic checkpoint ID
    let id = generate_checkpoint_id(&timestamp, &input.description);

    // 3. Capture git context (gracefully returns None if not a git repo)
    let git = get_git_context(workspace_root).await;

    // 4. Read .memories/.active-plan if it exists
    let plan_id = read_active_plan(workspace_root);

    // 5. Extract summary from description
    let summary = extract_summary(&input.description);

    // 6. Build the Checkpoint struct
    let checkpoint = Checkpoint {
        id,
        timestamp,
        description: input.description,
        checkpoint_type: input.checkpoint_type,
        context: input.context,
        decision: input.decision,
        alternatives: input.alternatives,
        impact: input.impact,
        evidence: input.evidence,
        symbols: input.symbols,
        next: input.next,
        confidence: input.confidence,
        unknowns: input.unknowns,
        tags: input.tags,
        git,
        summary,
        plan_id,
    };

    // 7. Format as YAML frontmatter + markdown
    let content = format_checkpoint(&checkpoint);

    // 8. Create date directory .memories/{YYYY-MM-DD}/
    let date = &checkpoint.timestamp[..10]; // "YYYY-MM-DD"
    let date_dir = workspace_root.join(".memories").join(date);
    std::fs::create_dir_all(&date_dir)
        .with_context(|| format!("Failed to create memories directory: {}", date_dir.display()))?;

    // 9. Get filename and write
    let filename = get_checkpoint_filename(&checkpoint.timestamp, &checkpoint.id);
    let file_path = date_dir.join(&filename);
    std::fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write checkpoint file: {}", file_path.display()))?;

    // 10. Index in Tantivy (best-effort — file is already safely written)
    let rel_path = format!("{}/{}", date, filename);
    if let Err(e) = index_checkpoint_in_tantivy(workspace_root, &checkpoint, &rel_path) {
        tracing::warn!("Failed to index checkpoint in Tantivy (file is saved): {}", e);
    }

    Ok(checkpoint)
}

/// Read the active plan ID from `.memories/.active-plan`.
///
/// Returns `None` if the file doesn't exist, is empty, or cannot be read.
pub(crate) fn read_active_plan(workspace_root: &Path) -> Option<String> {
    let path = workspace_root.join(".memories").join(".active-plan");
    let content = std::fs::read_to_string(&path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Extract a summary from the checkpoint description.
///
/// Looks for the first `## ` heading. If none found, uses the first non-empty
/// line. Returns `None` if the description is entirely empty/whitespace.
pub(crate) fn extract_summary(description: &str) -> Option<String> {
    // First, look for a ## heading
    for line in description.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("## ") {
            let heading = heading.trim();
            if !heading.is_empty() {
                return Some(heading.to_string());
            }
        }
    }

    // Fall back to first non-empty line
    for line in description.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Index a checkpoint into the Tantivy memory index.
///
/// Opens or creates the index, adds the document, and commits.
/// This is called after the checkpoint file is already written to disk,
/// so failures here are non-fatal (logged as warnings by the caller).
fn index_checkpoint_in_tantivy(
    workspace_root: &Path,
    checkpoint: &Checkpoint,
    rel_path: &str,
) -> Result<()> {
    let index_path = workspace_root.join(MEMORY_INDEX_REL);
    std::fs::create_dir_all(&index_path)?;

    let index = MemoryIndex::open_or_create(&index_path)?;
    index.add_checkpoint(checkpoint, Some(rel_path))?;
    index.commit()?;

    Ok(())
}
