//! Recall Tool - Retrieve development memories
//!
//! Queries saved memories with filtering by type, date range, and tags.
//! Results are returned in reverse chronological order (most recent first).
//!
//! For semantic search across memories, use fast_search with:
//! `file_pattern=".memories/**/*.json"`

use anyhow::Result;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::handler::JulieServerHandler;
use crate::tools::memory::{recall_memories, RecallOptions};

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
        return Ok(
            Utc.from_utc_datetime(&naive.and_hms_opt(0, 0, 0).unwrap())
                .timestamp(),
        );
    }

    anyhow::bail!("Invalid date format. Use ISO 8601 (YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, or YYYY-MM-DDTHH:MM:SSZ)")
}

#[mcp_tool(
    name = "recall",
    description = concat!(
        "Retrieve development memory checkpoints with optional filtering.\n\n",
        "⚡ USE THIS PROACTIVELY to:\n",
        "• Remember how you solved similar problems before\n",
        "• Understand past architectural decisions and their rationale\n",
        "• Avoid repeating mistakes from previous debugging sessions\n",
        "• Build on insights and learnings from earlier work\n\n",
        "Returns memories in reverse chronological order (most recent first). ",
        "Use filters to narrow results by type, date range, or tags.\n\n",
        "FILTERING:\n",
        "• type: Filter by memory type (checkpoint, decision, learning, etc.)\n",
        "• since: Return memories since this date (ISO 8601: YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, or YYYY-MM-DDTHH:MM:SSZ)\n",
        "• until: Return memories until this date (ISO 8601: YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, or YYYY-MM-DDTHH:MM:SSZ)\n",
        "• limit: Maximum number of results (default: 10)\n\n",
        "EXAMPLES:\n",
        "• Recent checkpoints: {\"limit\": 10}\n",
        "• Decisions only: {\"type\": \"decision\", \"limit\": 5}\n",
        "• Since date: {\"since\": \"2025-01-01\", \"limit\": 20}\n",
        "• All learnings: {\"type\": \"learning\"}\n\n",
        "TIP: For semantic search across memories, use fast_search with:\n",
        "file_pattern=\".memories/**/*.json\"\n\n",
        "Performance: <5ms for chronological queries"
    ),
    title = "Recall Development Memories",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "memory", "phase": "1"}"#
)]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RecallTool {
    /// Maximum number of results to return (default: 10)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,

    /// Return memories since this date (ISO 8601: YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, or YYYY-MM-DDTHH:MM:SSZ)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,

    /// Return memories until this date (ISO 8601: YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, or YYYY-MM-DDTHH:MM:SSZ)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,

    /// Filter by memory type (checkpoint, decision, learning, etc.)
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<String>,
}

impl RecallTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🔍 Recalling memories with filters: {:?}", self);

        // Get workspace root
        let workspace = handler.get_workspace().await?
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

        // Recall memories
        let mut memories = recall_memories(&workspace_root, options)?;

        // Return in reverse chronological order (most recent first)
        memories.reverse();

        // Format response
        if memories.is_empty() {
            let filter_info = if self.memory_type.is_some()
                || self.since.is_some()
                || self.until.is_some()
            {
                "\n\nTry adjusting your filters or use fast_search for semantic queries."
            } else {
                "\n\nCreate your first checkpoint with the checkpoint tool!"
            };

            return Ok(CallToolResult::text_content(vec![TextContent::from(
                format!("No memories found.{}", filter_info),
            )]));
        }

        // Build formatted output
        let mut output = format!("Found {} memor{}:\n\n", memories.len(), if memories.len() == 1 { "y" } else { "ies" });

        for memory in &memories {
            // Format timestamp in local timezone
            let dt = DateTime::from_timestamp(memory.timestamp, 0)
                .unwrap_or_else(|| Utc::now());
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
                format!(" [{}@{}]", git.branch, &git.commit[..8])
            });

            // Build entry
            output.push_str(&format!(
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
                 📅 {} | {} | {}\n",
                date_str,
                memory.memory_type,
                &memory.id[..20] // Show first 20 chars of ID
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

        // Add footer with search tip
        output.push_str(&format!(
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
             Showing {} of total memories. Use limit parameter to see more.\n\n\
             💡 TIP: Use fast_search with file_pattern=\".memories/**/*.json\" for semantic queries.",
            memories.len()
        ));

        Ok(CallToolResult::text_content(vec![TextContent::from(
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
}
