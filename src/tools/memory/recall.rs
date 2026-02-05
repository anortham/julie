//! Recall Tool - Retrieve development memories
//!
//! Queries saved memories with filtering by type, date range, and tags.
//! Without a query: returns memories in reverse chronological order (most recent first).
//! With a query: searches memory content using Tantivy and returns results ranked by relevance.

use anyhow::Result;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use tracing::info;

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
}

impl RecallTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🔍 Recalling memories with filters: {:?}", self);

        // Get workspace root
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let workspace_root = workspace.root.clone();

        // Parse date filters if provided
        let since_ts = self
            .since
            .as_ref()
            .map(|s| parse_date_to_timestamp(s))
            .transpose()?;

        let until_ts = self
            .until
            .as_ref()
            .map(|s| parse_date_to_timestamp(s))
            .transpose()?;

        // Build recall options
        let options = RecallOptions {
            memory_type: self.memory_type.clone(),
            since: since_ts,
            until: until_ts,
            limit: self.limit.map(|l| l as usize),
        };

        // Recall or search memories
        let memories_with_scores: Vec<(Memory, Option<f32>)> = if let Some(ref query) = self.query {
            // Query provided — use search (returns ranked by relevance)
            let results = search_memories(&workspace_root, query, options)?;
            results.into_iter().map(|(m, s)| (m, Some(s))).collect()
        } else {
            // No query — chronological recall
            let mut memories = recall_memories(&workspace_root, options)?;
            memories.reverse(); // Most recent first
            memories.into_iter().map(|m| (m, None)).collect()
        };

        // Format response
        if memories_with_scores.is_empty() {
            let message = if let Some(ref query) = self.query {
                format!(
                    "No memories matched query \"{}\".\n\n\
                     Try broader terms or remove filters. Use recall without a query to browse chronologically.",
                    query
                )
            } else if self.memory_type.is_some() || self.since.is_some() || self.until.is_some() {
                "No memories found.\n\nTry adjusting your filters.".to_string()
            } else {
                "No memories found.\n\nCreate your first checkpoint with the checkpoint tool!".to_string()
            };

            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        }

        // Build formatted output
        let mut output = format!(
            "Found {} memor{}:\n\n",
            memories_with_scores.len(),
            if memories_with_scores.len() == 1 { "y" } else { "ies" }
        );

        for (memory, score) in &memories_with_scores {
            // Format timestamp in local timezone
            let dt = DateTime::from_timestamp(memory.timestamp, 0).unwrap_or_else(|| Utc::now());
            let local_dt = dt.with_timezone(&Local);
            let date_str = local_dt.format("%Y-%m-%d %H:%M:%S").to_string();

            // Extract description (if present in extra fields)
            let description = memory
                .extra
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("[no description]");

            // Extract tags (if present)
            let tags = memory
                .extra
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|s| !s.is_empty());

            // Format git info (if present)
            let git_info = memory.git.as_ref().map(|git| {
                // Take up to 8 chars, or full hash if shorter (handles 7-char git short hashes)
                let commit_display = if git.commit.len() >= 8 {
                    &git.commit[..8]
                } else {
                    &git.commit
                };
                format!(" [{}@{}]", git.branch, commit_display)
            });

            // Build entry
            let score_display = match score {
                Some(s) => format!(" | ⚡ {:.2}", s),
                None => String::new(),
            };
            output.push_str(&format!(
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 📅 {} | {} | {}{}\n",
                date_str,
                memory.memory_type,
                &memory.id[..20], // Show first 20 chars of ID
                score_display
            ));

            if let Some(git) = git_info {
                output.push_str(&format!("📍 Git: {}\n", git));
            }

            output.push_str(&format!("📝 {}\n", description));

            if let Some(tags_str) = tags {
                output.push_str(&format!("🏷️  {}\n", tags_str));
            }

            output.push('\n');
        }

        // Add footer
        output.push_str(&format!(
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
             Showing {} of total memories. Use limit parameter to see more.",
            memories_with_scores.len()
        ));

        // Only show search tip in chronological mode (no query)
        if self.query.is_none() {
            output.push_str(
                "\n\n💡 TIP: Use `query` parameter to search memory content (e.g., recall(query=\"tantivy scoring\"))"
            );
        }

        Ok(CallToolResult::text_content(vec![Content::text(
            output,
        )]))
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
        // This test will FAIL initially - that's the bug we're fixing
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
        // Git short hashes are typically 7 characters (e.g., "05a8cb5")
        // We now handle this gracefully by taking min(len, 8)

        let short_commit = "05a8cb5"; // 7 characters - typical git short hash
        let long_commit = "05a8cb5def123"; // 13 characters

        // Test the same logic as line 179-183
        let short_display = if short_commit.len() >= 8 {
            &short_commit[..8]
        } else {
            short_commit
        };
        let formatted_short = format!(" [main@{}]", short_display);
        assert_eq!(formatted_short, " [main@05a8cb5]");

        let long_display = if long_commit.len() >= 8 {
            &long_commit[..8]
        } else {
            long_commit
        };
        let formatted_long = format!(" [main@{}]", long_display);
        assert_eq!(formatted_long, " [main@05a8cb5d]");
    }
}
