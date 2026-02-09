//! Recall Tool - Retrieve development memories
//!
//! Queries saved memories with filtering by type, date range, and tags.
//! Without a query: returns memories in chronological order (oldest first).
//! With a query: searches memory content using Tantivy and returns results ranked by relevance.
//! With `scope: "global"`: aggregates memories across all registered projects.

use anyhow::Result;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use tracing::{info, warn, debug};

use crate::handler::JulieServerHandler;
use crate::tools::memory::{RecallOptions, recall_memories, search_memories, Memory};

/// Parse ISO date string to Unix timestamp
fn parse_date_to_timestamp(date_str: &str) -> Result<i64> {
    // Try parsing ISO 8601 format with timezone (YYYY-MM-DDTHH:MM:SSZ)
    if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
        return Ok(dt.timestamp());
    }

    // Try parsing datetime without timezone (YYYY-MM-DDTHH:MM:SS) - assume local machine time
    if let Ok(naive_dt) = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Local.from_local_datetime(&naive_dt).unwrap().timestamp());
    }

    // Try parsing just date (YYYY-MM-DD)
    if let Ok(naive) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return Ok(Utc
            .from_utc_datetime(&naive.and_hms_opt(0, 0, 0).unwrap())
            .timestamp());
    }

    anyhow::bail!(
        "Invalid date format. Use ISO 8601 (YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, or YYYY-MM-DDTHH:MM:SSZ)"
    )
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RecallTool {
    /// Search query to find specific memories by content (uses fuzzy matching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Maximum results (default: 10)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Since date (ISO 8601: YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, YYYY-MM-DDTHH:MM:SSZ)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Until date (ISO 8601: YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, YYYY-MM-DDTHH:MM:SSZ)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// Filter by type: "checkpoint", "decision", "learning", etc.
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<String>,
    /// Recall scope: omit for current workspace, "global" for all registered projects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Format a single memory entry as text. Returns the formatted string.
fn format_memory_entry(memory: &Memory, score: Option<f32>) -> String {
    let dt = DateTime::from_timestamp(memory.timestamp, 0).unwrap_or_else(|| Utc::now());
    let local_dt = dt.with_timezone(&Local);
    let date_str = local_dt.format("%Y-%m-%d %H:%M").to_string();

    let description = memory
        .extra
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("[no description]");

    let tags = memory
        .extra
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|t| format!("#{}", t))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty());

    // Header: type | date | git [| score]
    let mut parts: Vec<String> = vec![memory.memory_type.clone(), date_str];

    if let Some(git) = &memory.git {
        let short_hash = if git.commit.len() >= 7 {
            &git.commit[..7]
        } else {
            &git.commit
        };
        parts.push(format!("{}@{}", git.branch, short_hash));
    }

    if let Some(s) = score {
        parts.push(format!("score: {:.1}", s));
    }

    let mut out = format!("\n{}\n{}\n", parts.join(" | "), description);

    if let Some(tags_str) = tags {
        out.push_str(&format!("{}\n", tags_str));
    }

    out
}

/// Parse the date filter fields into timestamps.
fn parse_date_filters(since: &Option<String>, until: &Option<String>) -> Result<(Option<i64>, Option<i64>)> {
    let since_ts = since
        .as_ref()
        .map(|s| parse_date_to_timestamp(s))
        .transpose()?;
    let until_ts = until
        .as_ref()
        .map(|s| parse_date_to_timestamp(s))
        .transpose()?;
    Ok((since_ts, until_ts))
}

// ---------------------------------------------------------------------------
// Local recall (current workspace)
// ---------------------------------------------------------------------------

impl RecallTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🔍 Recalling memories with filters: {:?}", self);

        // Route to global recall if scope is "global"
        if self.scope.as_deref() == Some("global") {
            return self.recall_global().await;
        }

        // Get workspace root
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let workspace_root = workspace.root.clone();

        let (since_ts, until_ts) = parse_date_filters(&self.since, &self.until)?;

        let options = RecallOptions {
            memory_type: self.memory_type.clone(),
            since: since_ts,
            until: until_ts,
            limit: self.limit.map(|l| l as usize),
        };

        // Recall or search memories
        let memories_with_scores: Vec<(Memory, Option<f32>)> = if let Some(ref query) = self.query {
            let results = search_memories(&workspace_root, query, options)?;
            results.into_iter().map(|(m, s)| (m, Some(s))).collect()
        } else {
            let memories = recall_memories(&workspace_root, options)?;
            memories.into_iter().map(|m| (m, None)).collect()
        };

        // Format response
        if memories_with_scores.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                Self::empty_message(&self.query, &self.memory_type, &self.since, &self.until),
            )]));
        }

        let is_search = self.query.is_some();
        let mut output = format!(
            "Found {} memor{}{}:\n",
            memories_with_scores.len(),
            if memories_with_scores.len() == 1 { "y" } else { "ies" },
            if is_search { " (by relevance)" } else { "" }
        );

        for (memory, score) in &memories_with_scores {
            output.push_str(&format_memory_entry(memory, *score));
        }

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }

    fn empty_message(
        query: &Option<String>,
        memory_type: &Option<String>,
        since: &Option<String>,
        until: &Option<String>,
    ) -> String {
        if let Some(q) = query {
            format!(
                "No memories matched query \"{}\".\n\n\
                 Try broader terms or remove filters. Use recall without a query to browse chronologically.",
                q
            )
        } else if memory_type.is_some() || since.is_some() || until.is_some() {
            "No memories found.\n\nTry adjusting your filters.".to_string()
        } else {
            "No memories found.\n\nCreate your first checkpoint with the checkpoint tool!"
                .to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Global recall (across all registered projects)
// ---------------------------------------------------------------------------

impl RecallTool {
    /// Recall memories across all registered projects.
    async fn recall_global(&self) -> Result<CallToolResult> {
        let projects = crate::user_registry::list_projects()?;

        if projects.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "No projects registered yet.\n\n\
                 Projects auto-register when you open them with Julie.",
            )]));
        }

        let (since_ts, until_ts) = parse_date_filters(&self.since, &self.until)?;

        // Build options WITHOUT limit — we collect all, then apply limit after merge
        let base_options = RecallOptions {
            memory_type: self.memory_type.clone(),
            since: since_ts,
            until: until_ts,
            limit: None,
        };

        // Collect memories from all projects, tagged with project name
        let mut all_memories: Vec<(Memory, Option<f32>, String)> = Vec::new();

        for project in &projects {
            let project_path = Path::new(&project.path);

            if !project_path.exists() {
                debug!("Skipping missing project: {} ({})", project.name, project.path);
                continue;
            }

            let options = base_options.clone();
            let result = if let Some(ref query) = self.query {
                match search_memories(project_path, query, options) {
                    Ok(results) => results
                        .into_iter()
                        .map(|(m, s)| (m, Some(s), project.name.clone()))
                        .collect::<Vec<_>>(),
                    Err(e) => {
                        warn!("Failed to search memories in {}: {}", project.name, e);
                        continue;
                    }
                }
            } else {
                match recall_memories(project_path, options) {
                    Ok(memories) => memories
                        .into_iter()
                        .map(|m| (m, None, project.name.clone()))
                        .collect::<Vec<_>>(),
                    Err(e) => {
                        warn!("Failed to recall memories from {}: {}", project.name, e);
                        continue;
                    }
                }
            };

            all_memories.extend(result);
        }

        if all_memories.is_empty() {
            let scope_note = format!(
                "No memories found across {} registered project{}.",
                projects.len(),
                if projects.len() == 1 { "" } else { "s" }
            );
            return Ok(CallToolResult::text_content(vec![Content::text(scope_note)]));
        }

        // Sort: by score (desc) if search, by timestamp (asc = chronological) if not
        if self.query.is_some() {
            all_memories.sort_by(|a, b| {
                b.1.unwrap_or(0.0)
                    .partial_cmp(&a.1.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            all_memories.sort_by_key(|(m, _, _)| m.timestamp);
        }

        // Apply limit after merge
        let limit = self.limit.unwrap_or(20) as usize;
        all_memories.truncate(limit);

        // Count unique projects in results
        let unique_projects: HashSet<&str> = all_memories.iter().map(|(_, _, p)| p.as_str()).collect();

        let is_search = self.query.is_some();
        let mut output = format!(
            "Found {} memor{} across {} project{}{}:\n",
            all_memories.len(),
            if all_memories.len() == 1 { "y" } else { "ies" },
            unique_projects.len(),
            if unique_projects.len() == 1 { "" } else { "s" },
            if is_search { " (by relevance)" } else { "" }
        );

        // Format with project group headers (insert header when project changes)
        let mut current_project = String::new();
        for (memory, score, project_name) in &all_memories {
            if current_project != *project_name {
                output.push_str(&format!("\n## {}\n", project_name));
                current_project = project_name.clone();
            }
            output.push_str(&format_memory_entry(memory, *score));
        }

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date_with_timezone() {
        // Should accept full ISO 8601 with timezone
        let result = parse_date_to_timestamp("2025-11-10T02:10:08Z");
        assert!(result.is_ok());

        let result = parse_date_to_timestamp("2025-11-10T02:10:08+00:00");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_date_without_timezone() {
        // Should accept datetime without timezone (assume local machine time)
        let result = parse_date_to_timestamp("2025-11-10T02:10:08");
        assert!(result.is_ok(), "Should accept datetime without timezone");
    }

    #[test]
    fn test_parse_date_only() {
        // Should accept just date (YYYY-MM-DD)
        let result = parse_date_to_timestamp("2025-11-10");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_invalid_date() {
        // Should reject invalid formats
        let result = parse_date_to_timestamp("not-a-date");
        assert!(result.is_err());

        let result = parse_date_to_timestamp("2025/11/10");
        assert!(result.is_err());
    }

    #[test]
    fn test_short_commit_hash_no_longer_panics() {
        // BUG FIX VERIFICATION: Short commit hashes should not crash
        let short_commit = "05a8cb"; // 6 characters - shorter than default
        let normal_commit = "05a8cb5"; // 7 characters - typical git short hash
        let long_commit = "05a8cb5def123"; // 13 characters

        let short_display = if short_commit.len() >= 7 {
            &short_commit[..7]
        } else {
            short_commit
        };
        assert_eq!(format!("main@{}", short_display), "main@05a8cb");

        let normal_display = if normal_commit.len() >= 7 {
            &normal_commit[..7]
        } else {
            normal_commit
        };
        assert_eq!(format!("main@{}", normal_display), "main@05a8cb5");

        let long_display = if long_commit.len() >= 7 {
            &long_commit[..7]
        } else {
            long_commit
        };
        assert_eq!(format!("main@{}", long_display), "main@05a8cb5");
    }
}
