//! YAML frontmatter storage layer for checkpoint files.
//!
//! Handles serialization/deserialization of checkpoint markdown files with
//! YAML frontmatter, maintaining full compatibility with Goldfish's format.
//!
//! ## File Format
//!
//! ```text
//! ---
//! id: checkpoint_7bb3fd6e
//! timestamp: "2026-03-07T17:44:14.659Z"
//! tags:
//!   - architecture
//! git:
//!   branch: main
//!   commit: 62017a0
//! type: decision
//! ---
//!
//! # Markdown body here
//! ```

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use super::{Checkpoint, CheckpointType, GitContext};

/// Generate a deterministic checkpoint ID from timestamp and description.
///
/// Format: `checkpoint_{first 8 hex chars of SHA-256(timestamp:description)}`
///
/// This matches Goldfish's `generateCheckpointId()` exactly.
pub fn generate_checkpoint_id(timestamp: &str, description: &str) -> String {
    let input = format!("{}:{}", timestamp, description);
    let hash = Sha256::digest(input.as_bytes());
    let hex = hex::encode(hash);
    format!("checkpoint_{}", &hex[..8])
}

/// Get the filename for a checkpoint file.
///
/// Format: `HHMMSS_xxxx.md` where xxxx is the first 4 chars of the hash
/// portion of the checkpoint ID.
///
/// This matches Goldfish's `getCheckpointFilename()` exactly.
pub fn get_checkpoint_filename(timestamp: &str, id: &str) -> String {
    // Extract HH:MM:SS from ISO timestamp (chars 11..19)
    let time_part = &timestamp[11..19]; // "HH:MM:SS"
    let hhmmss = time_part.replace(':', "");

    // Extract first 4 chars of the hash portion of the ID
    let hash4 = id
        .strip_prefix("checkpoint_")
        .unwrap_or(id)
        .get(..4)
        .unwrap_or("0000");

    format!("{}_{}.md", hhmmss, hash4)
}

/// Format a checkpoint as YAML frontmatter + markdown body.
///
/// Produces the standard Goldfish-compatible format:
/// ```text
/// ---
/// yaml frontmatter
/// ---
///
/// markdown body
/// ```
///
/// Only includes fields that are present (no None/empty values in output).
pub fn format_checkpoint(checkpoint: &Checkpoint) -> String {
    let mut frontmatter = serde_yaml::Mapping::new();

    // Required fields
    frontmatter.insert(
        serde_yaml::Value::String("id".to_string()),
        serde_yaml::Value::String(checkpoint.id.clone()),
    );
    frontmatter.insert(
        serde_yaml::Value::String("timestamp".to_string()),
        serde_yaml::Value::String(checkpoint.timestamp.clone()),
    );

    // Tags (omit if empty)
    if let Some(ref tags) = checkpoint.tags {
        if !tags.is_empty() {
            let tag_values: Vec<serde_yaml::Value> = tags
                .iter()
                .map(|t| serde_yaml::Value::String(t.clone()))
                .collect();
            frontmatter.insert(
                serde_yaml::Value::String("tags".to_string()),
                serde_yaml::Value::Sequence(tag_values),
            );
        }
    }

    // Git context (omit if all fields are None)
    if let Some(ref git) = checkpoint.git {
        let mut git_map = serde_yaml::Mapping::new();
        let mut has_fields = false;

        if let Some(ref branch) = git.branch {
            git_map.insert(
                serde_yaml::Value::String("branch".to_string()),
                serde_yaml::Value::String(branch.clone()),
            );
            has_fields = true;
        }
        if let Some(ref commit) = git.commit {
            git_map.insert(
                serde_yaml::Value::String("commit".to_string()),
                serde_yaml::Value::String(commit.clone()),
            );
            has_fields = true;
        }
        if let Some(ref files) = git.files {
            if !files.is_empty() {
                let file_values: Vec<serde_yaml::Value> = files
                    .iter()
                    .map(|f| serde_yaml::Value::String(f.clone()))
                    .collect();
                git_map.insert(
                    serde_yaml::Value::String("files".to_string()),
                    serde_yaml::Value::Sequence(file_values),
                );
                has_fields = true;
            }
        }

        if has_fields {
            frontmatter.insert(
                serde_yaml::Value::String("git".to_string()),
                serde_yaml::Value::Mapping(git_map),
            );
        }
    }

    // Summary
    if let Some(ref summary) = checkpoint.summary {
        frontmatter.insert(
            serde_yaml::Value::String("summary".to_string()),
            serde_yaml::Value::String(summary.clone()),
        );
    }

    // Plan ID
    if let Some(ref plan_id) = checkpoint.plan_id {
        frontmatter.insert(
            serde_yaml::Value::String("planId".to_string()),
            serde_yaml::Value::String(plan_id.clone()),
        );
    }

    // Type
    if let Some(ref checkpoint_type) = checkpoint.checkpoint_type {
        let type_str = match checkpoint_type {
            CheckpointType::Checkpoint => "checkpoint",
            CheckpointType::Decision => "decision",
            CheckpointType::Incident => "incident",
            CheckpointType::Learning => "learning",
        };
        frontmatter.insert(
            serde_yaml::Value::String("type".to_string()),
            serde_yaml::Value::String(type_str.to_string()),
        );
    }

    // Context
    if let Some(ref context) = checkpoint.context {
        frontmatter.insert(
            serde_yaml::Value::String("context".to_string()),
            serde_yaml::Value::String(context.clone()),
        );
    }

    // Decision
    if let Some(ref decision) = checkpoint.decision {
        frontmatter.insert(
            serde_yaml::Value::String("decision".to_string()),
            serde_yaml::Value::String(decision.clone()),
        );
    }

    // Alternatives
    if let Some(ref alternatives) = checkpoint.alternatives {
        if !alternatives.is_empty() {
            let values: Vec<serde_yaml::Value> = alternatives
                .iter()
                .map(|a| serde_yaml::Value::String(a.clone()))
                .collect();
            frontmatter.insert(
                serde_yaml::Value::String("alternatives".to_string()),
                serde_yaml::Value::Sequence(values),
            );
        }
    }

    // Impact
    if let Some(ref impact) = checkpoint.impact {
        frontmatter.insert(
            serde_yaml::Value::String("impact".to_string()),
            serde_yaml::Value::String(impact.clone()),
        );
    }

    // Evidence
    if let Some(ref evidence) = checkpoint.evidence {
        if !evidence.is_empty() {
            let values: Vec<serde_yaml::Value> = evidence
                .iter()
                .map(|e| serde_yaml::Value::String(e.clone()))
                .collect();
            frontmatter.insert(
                serde_yaml::Value::String("evidence".to_string()),
                serde_yaml::Value::Sequence(values),
            );
        }
    }

    // Symbols
    if let Some(ref symbols) = checkpoint.symbols {
        if !symbols.is_empty() {
            let values: Vec<serde_yaml::Value> = symbols
                .iter()
                .map(|s| serde_yaml::Value::String(s.clone()))
                .collect();
            frontmatter.insert(
                serde_yaml::Value::String("symbols".to_string()),
                serde_yaml::Value::Sequence(values),
            );
        }
    }

    // Next
    if let Some(ref next) = checkpoint.next {
        frontmatter.insert(
            serde_yaml::Value::String("next".to_string()),
            serde_yaml::Value::String(next.clone()),
        );
    }

    // Confidence
    if let Some(confidence) = checkpoint.confidence {
        frontmatter.insert(
            serde_yaml::Value::String("confidence".to_string()),
            serde_yaml::Value::Number(serde_yaml::Number::from(confidence)),
        );
    }

    // Unknowns
    if let Some(ref unknowns) = checkpoint.unknowns {
        if !unknowns.is_empty() {
            let values: Vec<serde_yaml::Value> = unknowns
                .iter()
                .map(|u| serde_yaml::Value::String(u.clone()))
                .collect();
            frontmatter.insert(
                serde_yaml::Value::String("unknowns".to_string()),
                serde_yaml::Value::Sequence(values),
            );
        }
    }

    let yaml = serde_yaml::to_string(&frontmatter).unwrap_or_default();
    // serde_yaml adds a trailing newline; trim it for clean output
    let yaml = yaml.trim();

    format!("---\n{}\n---\n\n{}\n", yaml, checkpoint.description)
}

/// Parse a checkpoint from a YAML frontmatter markdown file.
///
/// Handles:
/// - Standard Goldfish format (YAML frontmatter + markdown body)
/// - BOM prefix (Windows Notepad)
/// - CRLF line endings (Windows git checkout)
/// - Legacy `files_changed` field (normalized to `files`)
/// - Legacy Unix timestamps (normalized to ISO 8601)
pub fn parse_checkpoint(content: &str) -> Result<Checkpoint> {
    // Strip BOM and normalize CRLF -> LF
    let normalized = content
        .trim_start_matches('\u{FEFF}')
        .replace("\r\n", "\n");

    // Split on frontmatter delimiters
    let (yaml_content, body) = split_frontmatter(&normalized)
        .context("Invalid checkpoint file: no YAML frontmatter found")?;

    // Parse YAML frontmatter as a generic mapping
    let frontmatter: serde_yaml::Mapping = serde_yaml::from_str(yaml_content)
        .context("Invalid checkpoint file: YAML parsing failed")?;

    // Extract required fields
    let id = get_string(&frontmatter, "id")
        .context("Missing required field: id")?;

    let raw_timestamp = frontmatter.get("timestamp");
    let timestamp = normalize_timestamp(raw_timestamp);

    // Extract optional fields
    let checkpoint_type = get_string(&frontmatter, "type").and_then(|s| match s.as_str() {
        "checkpoint" => Some(CheckpointType::Checkpoint),
        "decision" => Some(CheckpointType::Decision),
        "incident" => Some(CheckpointType::Incident),
        "learning" => Some(CheckpointType::Learning),
        _ => None,
    });

    let git = parse_git_context(&frontmatter);
    let tags = get_string_array(&frontmatter, "tags");
    let summary = get_string(&frontmatter, "summary");
    let plan_id = get_string(&frontmatter, "planId");
    let context = get_string(&frontmatter, "context");
    let decision = get_string(&frontmatter, "decision");
    let alternatives = get_string_array(&frontmatter, "alternatives");
    let impact = get_string(&frontmatter, "impact");
    let evidence = get_string_array(&frontmatter, "evidence");
    let symbols = get_string_array(&frontmatter, "symbols");
    let next = get_string(&frontmatter, "next");
    let confidence = get_confidence(&frontmatter);
    let unknowns = get_string_array(&frontmatter, "unknowns");

    Ok(Checkpoint {
        id,
        timestamp,
        description: body.to_string(),
        checkpoint_type,
        context,
        decision,
        alternatives,
        impact,
        evidence,
        symbols,
        next,
        confidence,
        unknowns,
        tags,
        git,
        summary,
        plan_id,
    })
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Split content into (yaml_content, body) at the frontmatter delimiters.
fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    // Must start with "---\n"
    let content = content.strip_prefix("---\n")?;

    // Find closing "---\n"
    let end_pos = content.find("\n---\n")?;
    let yaml = &content[..end_pos];
    let body = &content[end_pos + 5..]; // skip "\n---\n"

    // Body typically has a leading newline after the closing ---
    let body = body.strip_prefix('\n').unwrap_or(body);
    // Trim trailing whitespace from body
    let body = body.trim_end();

    Some((yaml, body))
}

/// Extract a string value from a YAML mapping.
fn get_string(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

/// Extract a string array from a YAML mapping.
fn get_string_array(map: &serde_yaml::Mapping, key: &str) -> Option<Vec<String>> {
    map.get(key).and_then(|v| {
        if let serde_yaml::Value::Sequence(seq) = v {
            let items: Vec<String> = seq
                .iter()
                .filter_map(|item| match item {
                    serde_yaml::Value::String(s) => {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        }
                    }
                    serde_yaml::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect();
            if items.is_empty() {
                None
            } else {
                Some(items)
            }
        } else {
            None
        }
    })
}

/// Extract and validate the confidence field (1-5).
fn get_confidence(map: &serde_yaml::Mapping) -> Option<u8> {
    let value = map.get("confidence")?;
    let num = match value {
        serde_yaml::Value::Number(n) => n.as_u64()?,
        serde_yaml::Value::String(s) => s.parse::<u64>().ok()?,
        _ => return None,
    };

    if (1..=5).contains(&num) {
        Some(num as u8)
    } else {
        None
    }
}

/// Normalize a YAML timestamp value to ISO 8601 string.
///
/// Handles:
/// - String timestamps (ISO 8601 passthrough)
/// - Numeric timestamps (Unix seconds or milliseconds -> ISO 8601)
fn normalize_timestamp(value: Option<&serde_yaml::Value>) -> String {
    match value {
        Some(serde_yaml::Value::String(s)) => s.clone(),
        Some(serde_yaml::Value::Number(n)) => {
            if let Some(secs) = n.as_u64() {
                // Heuristic: values > 1e10 are milliseconds, otherwise seconds
                // (Unix seconds won't exceed 1e10 until year 2286)
                let millis = if secs > 10_000_000_000 {
                    secs as i64
                } else {
                    (secs as i64) * 1000
                };
                // Convert to ISO 8601
                chrono::DateTime::from_timestamp_millis(millis)
                    .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                    .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string())
            } else if let Some(secs_f) = n.as_f64() {
                let millis = if secs_f > 1e10 {
                    secs_f as i64
                } else {
                    (secs_f * 1000.0) as i64
                };
                chrono::DateTime::from_timestamp_millis(millis)
                    .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                    .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string())
            } else {
                chrono::Utc::now()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            }
        }
        _ => chrono::Utc::now()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    }
}

/// Parse a git context from the YAML frontmatter.
///
/// Handles both current format (`files`) and legacy format (`files_changed`).
fn parse_git_context(map: &serde_yaml::Mapping) -> Option<GitContext> {
    let git_value = map.get("git")?;
    let git_map = git_value.as_mapping()?;

    let branch = get_string(git_map, "branch");
    let commit = get_string(git_map, "commit");

    // Handle both `files` and legacy `files_changed` / `filesChanged`
    let files = get_string_array(git_map, "files")
        .or_else(|| get_string_array(git_map, "files_changed"))
        .or_else(|| get_string_array(git_map, "filesChanged"));

    // Only return Some if at least one field is present
    if branch.is_none() && commit.is_none() && files.is_none() {
        return None;
    }

    Some(GitContext {
        branch,
        commit,
        files,
    })
}
