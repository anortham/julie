use super::ManageWorkspaceTool;
use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use tracing::{info, warn};

impl ManageWorkspaceTool {
    /// Handle refresh command - re-index workspace
    pub(crate) async fn handle_refresh_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("Refreshing workspace: {}", workspace_id);

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "No primary workspace found.";
                return Ok(CallToolResult::text_content(vec![Content::text(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());

        // Get workspace info
        match registry_service.get_workspace(workspace_id).await? {
            Some(workspace_entry) => {
                // Update last accessed time
                registry_service.update_last_accessed(workspace_id).await?;

                // Actually re-index the workspace content
                let workspace_path = std::path::PathBuf::from(&workspace_entry.original_path);

                info!(
                    "Starting re-indexing of workspace: {}",
                    workspace_entry.display_name
                );

                let force = self.force.unwrap_or(false);
                match self
                    .index_workspace_files(handler, &workspace_path, force)
                    .await
                {
                    Ok(result) => {
                        // Update workspace statistics in registry
                        if let Ok(Some(workspace)) = handler.get_workspace().await {
                            // Use per-workspace index path
                            let index_path = workspace.workspace_index_path(workspace_id);

                            // Calculate directory size asynchronously to avoid blocking
                            let index_size = if index_path.metadata().is_ok() {
                                let path = index_path.clone();
                                match tokio::task::spawn_blocking(move || {
                                    crate::tools::workspace::calculate_dir_size(&path)
                                })
                                .await
                                {
                                    Ok(Ok(size)) => size,
                                    Ok(Err(e)) => {
                                        warn!(
                                            "Failed to calculate index directory size for {}: {}",
                                            workspace_id, e
                                        );
                                        0
                                    }
                                    Err(e) => {
                                        warn!(
                                            "spawn_blocking task failed for directory size calculation: {}",
                                            e
                                        );
                                        0
                                    }
                                }
                            } else {
                                0
                            };

                            if let Err(e) = registry_service
                                .update_workspace_statistics(
                                    workspace_id,
                                    result.symbols_total,
                                    result.files_total,
                                    index_size,
                                )
                                .await
                            {
                                warn!("Failed to update workspace statistics: {}", e);
                            } else {
                                info!(
                                    "Updated workspace statistics for {}: {} files, {} symbols, {} bytes index",
                                    workspace_id, result.files_total, result.symbols_total, index_size
                                );
                            }
                        }

                        let status = if result.files_processed == 0 {
                            "Already up-to-date.".to_string()
                        } else if force {
                            format!("Full re-index: {} files processed.", result.files_processed)
                        } else {
                            format!("{} changed files re-indexed.", result.files_processed)
                        };

                        let mut message = format!(
                            "Workspace Refresh: {}\n\
                            {}\n\
                            Path: {}\n\
                            Totals: {} files, {} symbols, {} relationships",
                            workspace_entry.display_name,
                            status,
                            workspace_entry.original_path,
                            result.files_total,
                            result.symbols_total,
                            result.relationships_total
                        );
                        if workspace_entry.workspace_type == WorkspaceType::Reference {
                            let embed_count = crate::tools::workspace::indexing::embeddings::spawn_reference_embedding(
                                handler,
                                workspace_id.to_string(),
                            ).await;
                            if embed_count > 0 {
                                message.push_str(&format!("\nEmbedding {} symbols in background...", embed_count));
                            }
                        }
                        Ok(CallToolResult::text_content(vec![Content::text(
                            message,
                        )]))
                    }
                    Err(e) => {
                        let message = format!(
                            "Workspace Refresh Failed\n\
                            Workspace: {}\n\
                            Path: {}\n\
                            Error: {}\n\
                            Check that the path exists and contains readable files",
                            workspace_entry.display_name, workspace_entry.original_path, e
                        );
                        Ok(CallToolResult::text_content(vec![Content::text(
                            message,
                        )]))
                    }
                }
            }
            None => {
                let message = format!("Workspace not found: {}", workspace_id);
                Ok(CallToolResult::text_content(vec![Content::text(
                    message,
                )]))
            }
        }
    }

    /// Handle stats command - show workspace statistics
    pub(crate) async fn handle_stats_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: Option<String>,
    ) -> Result<CallToolResult> {
        info!("Showing workspace statistics");

        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "No primary workspace found.";
                return Ok(CallToolResult::text_content(vec![Content::text(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());

        // Default to current workspace if no workspace_id specified
        let workspace_id = match workspace_id {
            Some(id) => Some(id),
            None => {
                // Get primary workspace ID from registry
                let registry = registry_service.load_registry().await?;
                registry.primary_workspace.as_ref().map(|pw| pw.id.clone())
            }
        };

        match workspace_id {
            Some(id) => {
                // Show stats for specific workspace
                match registry_service.get_workspace(&id).await? {
                    Some(workspace) => {
                        // Get embedding count from the workspace's DB
                        let db_path = primary_workspace.workspace_db_path(&id);
                        let embed_count = if db_path.exists() {
                            match tokio::task::spawn_blocking(move || {
                                SymbolDatabase::new(&db_path)
                                    .and_then(|db| db.embedding_count())
                                    .unwrap_or(0)
                            })
                            .await
                            {
                                Ok(count) => count,
                                Err(_) => 0,
                            }
                        } else {
                            0
                        };

                        let message = format!(
                            "Workspace Statistics: {}\n\n\
                            {} ({})\n\
                            Path: {}\n\
                            Type: {:?}\n\
                            Files: {} | Symbols: {}\n\
                            Embeddings: {}/{}\n\
                            Index Size: {:.2} MB\n\
                            Created: {} (unix)\n\
                            Last Accessed: {} (unix)\n\
                            Expires: {}",
                            workspace.display_name,
                            workspace.display_name,
                            workspace.id,
                            workspace.original_path,
                            workspace.workspace_type,
                            workspace.file_count,
                            workspace.symbol_count,
                            embed_count,
                            workspace.symbol_count,
                            workspace.index_size_bytes as f64 / (1024.0 * 1024.0),
                            workspace.created_at,
                            workspace.last_accessed,
                            workspace
                                .expires_at
                                .map(|t| t.to_string())
                                .unwrap_or("never".to_string())
                        );
                        Ok(CallToolResult::text_content(vec![Content::text(
                            message,
                        )]))
                    }
                    None => {
                        let message = format!("Workspace not found: {}", id);
                        Ok(CallToolResult::text_content(vec![Content::text(
                            message,
                        )]))
                    }
                }
            }
            None => {
                // Show overall statistics
                let registry = registry_service.load_registry().await?;

                let message = format!(
                    "Overall Workspace Statistics\n\n\
                    Registry Status\n\
                    Total Workspaces: {}\n\
                    Primary Workspace: {}\n\
                    Reference Workspaces: {}\n\
                    Orphaned Indexes: {}\n\n\
                    Storage Usage\n\
                    Total Files: {}\n\
                    Total Symbols: {}\n\
                    Total Index Size: {:.2} MB\n\
                    Last Updated: {} (unix)\n\n\
                    Configuration\n\
                    Default TTL: {} days\n\
                    Max Size Limit: {} MB\n\
                    Auto Cleanup: {}",
                    registry.statistics.total_workspaces,
                    if registry.primary_workspace.is_some() {
                        "Yes"
                    } else {
                        "No"
                    },
                    registry.reference_workspaces.len(),
                    registry.statistics.total_orphans,
                    registry.statistics.total_files,
                    registry.statistics.total_symbols,
                    registry.statistics.total_index_size_bytes as f64 / (1024.0 * 1024.0),
                    registry.last_updated,
                    registry.config.default_ttl_seconds / (24 * 60 * 60), // Convert to days
                    registry.config.max_total_size_bytes / (1024 * 1024), // Convert to MB
                    if registry.config.auto_cleanup_enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                );
                Ok(CallToolResult::text_content(vec![Content::text(
                    message,
                )]))
            }
        }
    }
}
