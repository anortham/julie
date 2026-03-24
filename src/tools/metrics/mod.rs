//! Metrics query tool — surface risk, coverage, and centrality data for symbols.
//!
//! Provides a structured interface for querying pre-computed analysis metrics
//! (security risk, change risk, test coverage, centrality) stored in symbol
//! metadata by the analysis pipeline.

pub(crate) mod operational;
pub(crate) mod query;
pub mod session;
pub(crate) mod trend;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::tools::navigation::resolution::{WorkspaceTarget, resolve_workspace_filter};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

fn default_category() -> String {
    "code_health".to_string()
}

fn default_sort_by() -> String {
    "security_risk".to_string()
}

fn default_order() -> String {
    "desc".to_string()
}

fn default_limit() -> u32 {
    20
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

/// Query pre-computed code health metrics (risk scores, test coverage, centrality).
///
/// Returns symbols ranked by the chosen metric, with filters for risk level,
/// test status, symbol kind, file pattern, and language.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct QueryMetricsTool {
    /// Metrics category: "code_health" (default), "session", "history", or "trend"
    #[serde(default = "default_category")]
    pub category: String,
    /// Sort field: "security_risk", "change_risk", "centrality", "test_coverage" (default: "security_risk")
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    /// Sort order: "asc" or "desc" (default: "desc")
    #[serde(default = "default_order")]
    pub order: String,
    /// Minimum risk level filter: "LOW", "MEDIUM", or "HIGH" (applies to security_risk or change_risk)
    #[serde(default)]
    pub min_risk: Option<String>,
    /// Filter by test status: true = only tested symbols, false = only untested symbols
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_bool_lenient"
    )]
    pub has_tests: Option<bool>,
    /// Filter by symbol kind: "function", "class", "method", etc.
    #[serde(default)]
    pub kind: Option<String>,
    /// Glob pattern to filter file paths (e.g., "src/core/**")
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Language filter: "rust", "typescript", "python", etc.
    #[serde(default)]
    pub language: Option<String>,
    /// Exclude test files from results (default: true)
    #[serde(
        default = "default_exclude_tests",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub exclude_tests: bool,
    /// Maximum results to return (default: 20)
    #[serde(
        default = "default_limit",
        deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient"
    )]
    pub limit: u32,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

fn default_exclude_tests() -> bool {
    true
}

impl QueryMetricsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!(
            "Metrics query: category={}, sort_by={}, order={}, limit={}",
            self.category, self.sort_by, self.order, self.limit
        );

        match self.category.as_str() {
            "session" => {
                let output = operational::format_session_from_metrics(&handler.session_metrics);
                Ok(CallToolResult::text_content(vec![Content::text(output)]))
            }
            "trend" => {
                let daemon_db = match &handler.daemon_db {
                    Some(db) => std::sync::Arc::clone(db),
                    None => {
                        return Ok(CallToolResult::text_content(vec![Content::text(
                            "Trend history is only available in daemon mode.".to_string(),
                        )]));
                    }
                };
                let workspace_id = match &handler.workspace_id {
                    Some(id) => id.clone(),
                    None => {
                        return Ok(CallToolResult::text_content(vec![Content::text(
                            "No workspace ID available for trend query.".to_string(),
                        )]));
                    }
                };

                let output = tokio::task::spawn_blocking(move || {
                    let snapshots = daemon_db.get_snapshot_history(&workspace_id, 10)?;
                    if snapshots.is_empty() {
                        return Ok::<String, anyhow::Error>(
                            "No codehealth history yet. Run an indexing pass first.".to_string(),
                        );
                    }
                    let table = trend::format_trend_table(&snapshots);
                    if snapshots.len() < 2 {
                        return Ok(table);
                    }
                    // Build a CodehealthSnapshot from the latest row for comparison
                    let latest = &snapshots[0];
                    let current = crate::daemon::database::CodehealthSnapshot {
                        total_symbols: latest.total_symbols,
                        total_files: latest.total_files,
                        security_high: latest.security_high,
                        security_medium: latest.security_medium,
                        security_low: latest.security_low,
                        change_high: latest.change_high,
                        change_medium: latest.change_medium,
                        change_low: latest.change_low,
                        symbols_tested: latest.symbols_tested,
                        symbols_untested: latest.symbols_untested,
                        avg_centrality: latest.avg_centrality,
                        max_centrality: latest.max_centrality,
                    };
                    let comparison = trend::format_comparison(&current, &snapshots[1]);
                    Ok(format!("{comparison}\n\n{table}"))
                })
                .await??;

                Ok(CallToolResult::text_content(vec![Content::text(output)]))
            }
            "history" => {
                let workspace_target =
                    resolve_workspace_filter(self.workspace.as_deref(), handler).await?;
                let db_arc = match workspace_target {
                    WorkspaceTarget::Primary => {
                        let workspace = handler
                            .get_workspace()
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;
                        workspace
                            .db
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("No database available"))?
                    }
                    WorkspaceTarget::Reference(ref id) => {
                        handler.get_database_for_workspace(id).await?
                    }
                };

                let output = tokio::task::spawn_blocking(move || {
                    let db = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            tracing::warn!("Database mutex poisoned in history query, recovering");
                            poisoned.into_inner()
                        }
                    };
                    let history = db.query_history_summary(7)?;
                    Ok::<String, anyhow::Error>(operational::format_history_output(&history))
                })
                .await??;

                Ok(CallToolResult::text_content(vec![Content::text(output)]))
            }
            "code_health" => {
                let workspace_target =
                    resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

                let db_arc = match workspace_target {
                    WorkspaceTarget::Primary => {
                        let workspace = handler
                            .get_workspace()
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;
                        workspace
                            .db
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("No database available"))?
                    }
                    WorkspaceTarget::Reference(ref id) => {
                        handler.get_database_for_workspace(id).await?
                    }
                };

                let sort_by = self.sort_by.clone();
                let order = self.order.clone();
                let min_risk = self.min_risk.clone();
                let has_tests = self.has_tests;
                let kind = self.kind.clone();
                let file_pattern = self.file_pattern.clone();
                let language = self.language.clone();
                let exclude_tests = self.exclude_tests;
                let limit = self.limit;

                let output = tokio::task::spawn_blocking(move || {
                    let db = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            tracing::warn!("Database mutex poisoned in metrics query, recovering");
                            poisoned.into_inner()
                        }
                    };
                    let results = query::query_by_metrics(
                        &db,
                        &sort_by,
                        &order,
                        min_risk.as_deref(),
                        has_tests,
                        kind.as_deref(),
                        file_pattern.as_deref(),
                        language.as_deref(),
                        exclude_tests,
                        limit,
                    )?;
                    Ok::<String, anyhow::Error>(query::format_metrics_output(
                        &results, &sort_by, &order,
                    ))
                })
                .await??;

                Ok(CallToolResult::text_content(vec![Content::text(output)]))
            }
            other => {
                let msg = format!(
                    "Unknown category '{}'. Valid categories: code_health, session, history, trend.",
                    other
                );
                Ok(CallToolResult::text_content(vec![Content::text(msg)]))
            }
        }
    }
}
