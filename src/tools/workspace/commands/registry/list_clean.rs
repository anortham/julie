use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::utils::progressive_reduction::ProgressiveReducer;
use crate::utils::token_estimation::TokenEstimator;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use tracing::info;

impl ManageWorkspaceTool {
    /// Handle list command - show all workspaces
    pub(crate) async fn handle_list_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        info!("Listing all workspaces");

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "No primary workspace found. Use 'index' command to create one.";
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());

        match registry_service.get_all_workspaces().await {
            Ok(workspaces) => {
                if workspaces.is_empty() {
                    let message = "No workspaces registered.";
                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        message,
                    )]));
                }

                // Apply token optimization using ProgressiveReducer
                let token_estimator = TokenEstimator::new();
                let reducer = ProgressiveReducer::new();

                // Target 10000 tokens for workspace listings
                let target_tokens = 10000;

                // Create a token estimation function that formats a workspace entry
                let estimate_workspaces =
                    |ws_subset: &[crate::workspace::registry::WorkspaceEntry]| {
                        let mut test_output = String::from("Registered Workspaces:\n\n");
                        for workspace in ws_subset {
                            let status = if workspace.is_expired() {
                                "EXPIRED"
                            } else if !workspace.path_exists() {
                                "MISSING"
                            } else {
                                "ACTIVE"
                            };

                            let expires = match workspace.expires_at {
                                Some(exp_time) => {
                                    let now = crate::workspace::registry::current_timestamp();
                                    if exp_time > now {
                                        let days_left = (exp_time - now) / (24 * 60 * 60);
                                        format!("in {} days", days_left)
                                    } else {
                                        "expired".to_string()
                                    }
                                }
                                None => "never".to_string(),
                            };

                            test_output.push_str(&format!(
                                "{} ({})\n\
                            Path: {}\n\
                            Type: {:?}\n\
                            Files: {} | Symbols: {} | Size: {:.1} KB\n\
                            Expires: {}\n\
                            Status: {}\n\n",
                                workspace.display_name,
                                workspace.id,
                                workspace.original_path,
                                workspace.workspace_type,
                                workspace.file_count,
                                workspace.symbol_count,
                                workspace.index_size_bytes as f64 / 1024.0,
                                expires,
                                status
                            ));
                        }
                        token_estimator.estimate_string(&test_output)
                    };

                // Reduce workspaces if needed to fit token limit
                let total_count = workspaces.len();
                let optimized_workspaces =
                    reducer.reduce(&workspaces, target_tokens, estimate_workspaces);
                let shown_count = optimized_workspaces.len();

                let mut output = String::from("Registered Workspaces:\n\n");

                for workspace in &optimized_workspaces {
                    let status = if workspace.is_expired() {
                        "EXPIRED"
                    } else if !workspace.path_exists() {
                        "MISSING"
                    } else {
                        "ACTIVE"
                    };

                    let expires = match workspace.expires_at {
                        Some(exp_time) => {
                            let now = crate::workspace::registry::current_timestamp();
                            if exp_time > now {
                                let days_left = (exp_time - now) / (24 * 60 * 60);
                                format!("in {} days", days_left)
                            } else {
                                "expired".to_string()
                            }
                        }
                        None => "never".to_string(),
                    };

                    output.push_str(&format!(
                        "{} ({})\n\
                        Path: {}\n\
                        Type: {:?}\n\
                        Files: {} | Symbols: {} | Size: {:.1} KB\n\
                        Expires: {}\n\
                        Status: {}\n\n",
                        workspace.display_name,
                        workspace.id,
                        workspace.original_path,
                        workspace.workspace_type,
                        workspace.file_count,
                        workspace.symbol_count,
                        workspace.index_size_bytes as f64 / 1024.0,
                        expires,
                        status
                    ));
                }

                // Add truncation notice if results were reduced
                if shown_count < total_count {
                    output.push_str(&format!(
                        "Showing {} of {} total workspaces (token limit applied)\n\
                        Use workspace stats to see details for specific workspaces\n",
                        shown_count, total_count
                    ));
                }

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    output,
                )]))
            }
            Err(e) => {
                let message = format!("Failed to list workspaces: {}", e);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }

    /// Handle clean command - clean expired/orphaned workspaces
    pub(crate) async fn handle_clean_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        info!("Cleaning workspaces (comprehensive cleanup: TTL + Size Limits + Orphans)");

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "No primary workspace found.";
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());

        // Always do comprehensive cleanup (TTL + Size Limits + Orphans)
        match registry_service
            .comprehensive_cleanup(primary_workspace.db.as_ref())
            .await
        {
            Ok(report) => {
                let ttl_count = report.ttl_cleanup.workspaces_removed.len();
                let size_count = report.size_cleanup.workspaces_removed.len();
                let orphan_count = report.orphaned_cleaned.len();
                let total_symbols = report.ttl_cleanup.total_symbols_deleted
                    + report.size_cleanup.total_symbols_deleted;
                let total_files = report.ttl_cleanup.total_files_deleted
                    + report.size_cleanup.total_files_deleted;

                let mut message_parts = Vec::new();

                if ttl_count > 0 {
                    message_parts.push(format!("TTL Cleanup: {} expired workspaces", ttl_count));
                }

                if size_count > 0 {
                    message_parts.push(format!(
                        "Size Cleanup: {} workspaces (LRU eviction)",
                        size_count
                    ));
                }

                if orphan_count > 0 {
                    message_parts.push(format!(
                        "Orphan Cleanup: {} abandoned indexes",
                        orphan_count
                    ));
                }

                let message = if message_parts.is_empty() {
                    "No cleanup needed. All workspaces are healthy!".to_string()
                } else {
                    format!(
                        "Comprehensive Cleanup Complete\n\n{}\n\n\
                        Database Impact:\n\
                        • {} symbols deleted\n\
                        • {} files deleted\n\
                        • {} relationships deleted\n\n\
                        Cleanup helps maintain optimal performance and storage usage.",
                        message_parts.join("\n"),
                        total_symbols,
                        total_files,
                        report.ttl_cleanup.total_relationships_deleted
                            + report.size_cleanup.total_relationships_deleted
                    )
                };

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
            Err(e) => {
                let message = format!("Failed to perform comprehensive cleanup: {}", e);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }
}
