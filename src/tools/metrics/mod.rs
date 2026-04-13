//! Metrics query tool — surface session, history, and code quality data.
//!
//! Provides a structured interface for querying operational metrics
//! (session stats, tool call history) and code quality metrics
//! (doc coverage, dead code detection) stored by the operational pipeline.

pub(crate) mod code_quality;
pub(crate) mod operational;
pub mod session;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::tools::navigation::resolution::{WorkspaceTarget, resolve_workspace_filter};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

fn default_category() -> String {
    "session".to_string()
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

/// Query metrics: operational (session, history) or code quality (doc_coverage, dead_code).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct QueryMetricsTool {
    /// Metrics category: "session" (default), "history", "doc_coverage", or "dead_code"
    #[serde(default = "default_category")]
    pub category: String,
    /// Sort order: "asc" or "desc" (default: "desc")
    #[serde(default = "default_order")]
    pub order: String,
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

impl QueryMetricsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!(
            "Metrics query: category={}, order={}, limit={}",
            self.category, self.order, self.limit
        );

        match self.category.as_str() {
            "session" => {
                let output = operational::format_session_from_metrics(&handler.session_metrics);
                Ok(CallToolResult::text_content(vec![Content::text(output)]))
            }
            "history" => {
                let workspace_target =
                    resolve_workspace_filter(self.workspace.as_deref(), handler).await?;
                let db_arc = match workspace_target {
                    WorkspaceTarget::Primary => handler.primary_database().await?,
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
            "doc_coverage" => {
                let workspace_target =
                    resolve_workspace_filter(self.workspace.as_deref(), handler).await?;
                let db_arc = match workspace_target {
                    WorkspaceTarget::Primary => handler.primary_database().await?,
                    WorkspaceTarget::Reference(ref id) => {
                        handler.get_database_for_workspace(id).await?
                    }
                };

                let limit = self.limit;
                let output = tokio::task::spawn_blocking(move || {
                    let db = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            tracing::warn!(
                                "Database mutex poisoned in doc_coverage query, recovering"
                            );
                            poisoned.into_inner()
                        }
                    };
                    let stats = db.get_doc_coverage()?;
                    let undocumented = db.get_undocumented_symbols(limit as usize)?;
                    Ok::<String, anyhow::Error>(code_quality::format_doc_coverage(
                        &stats,
                        &undocumented,
                    ))
                })
                .await??;

                Ok(CallToolResult::text_content(vec![Content::text(output)]))
            }
            "dead_code" => {
                let workspace_target =
                    resolve_workspace_filter(self.workspace.as_deref(), handler).await?;
                let db_arc = match workspace_target {
                    WorkspaceTarget::Primary => handler.primary_database().await?,
                    WorkspaceTarget::Reference(ref id) => {
                        handler.get_database_for_workspace(id).await?
                    }
                };

                let limit = self.limit;
                let output = tokio::task::spawn_blocking(move || {
                    let db = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            tracing::warn!(
                                "Database mutex poisoned in dead_code query, recovering"
                            );
                            poisoned.into_inner()
                        }
                    };
                    let total_dead = db.count_dead_code_candidates()?;
                    let candidates = db.get_dead_code_candidates(limit as usize)?;
                    let stats = db.get_doc_coverage()?;
                    Ok::<String, anyhow::Error>(code_quality::format_dead_code(
                        &candidates,
                        total_dead,
                        stats.total_public,
                    ))
                })
                .await??;

                Ok(CallToolResult::text_content(vec![Content::text(output)]))
            }
            other => {
                let msg = format!(
                    "Unknown category '{}'. Valid categories: session, history, doc_coverage, dead_code.",
                    other
                );
                Ok(CallToolResult::text_content(vec![Content::text(msg)]))
            }
        }
    }
}
