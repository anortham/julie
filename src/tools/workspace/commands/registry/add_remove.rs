use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use tracing::{debug, info, warn};

impl ManageWorkspaceTool {
    /// Handle add command - add reference workspace
    pub(crate) async fn handle_add_command(
        &self,
        handler: &JulieServerHandler,
        path: &str,
        name: Option<String>,
    ) -> Result<CallToolResult> {
        info!("Adding reference workspace: {}", path);

        // Get primary workspace for registry service
        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "No primary workspace found. Please run 'index' command first.";
                return Ok(CallToolResult::text_content(vec![Content::text(
                    message,
                )]));
            }
        };

        let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());

        // Register the reference workspace
        debug!("TRACE: About to call register_workspace for path: {}", path);
        match registry_service
            .register_workspace(path.to_string(), WorkspaceType::Reference)
            .await
        {
            Ok(entry) => {
                debug!(
                    "TRACE: register_workspace completed successfully for {}",
                    entry.id
                );
                let display_name = name.unwrap_or_else(|| entry.display_name.clone());

                // Index the reference workspace immediately
                let workspace_path = std::path::PathBuf::from(&entry.original_path);

                info!("Starting indexing of reference workspace: {}", display_name);

                debug!("About to call index_workspace_files for reference workspace");
                match self
                    .index_workspace_files(handler, &workspace_path, false)
                    .await
                {
                    Ok((symbol_count, file_count, relationship_count)) => {
                        debug!("index_workspace_files completed successfully");

                        // Update workspace statistics in registry
                        // Use per-workspace index path
                        let index_path = primary_workspace.workspace_index_path(&entry.id);

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
                                        entry.id, e
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
                                &entry.id,
                                symbol_count,
                                file_count,
                                index_size,
                            )
                            .await
                        {
                            warn!("Failed to update workspace statistics: {}", e);
                        } else {
                            info!(
                                "Updated workspace statistics for {}: {} files, {} symbols, {} bytes index",
                                entry.id, file_count, symbol_count, index_size
                            );
                        }

                        let message = format!(
                            "Reference workspace added and indexed!\n\
                             Workspace ID: {}\n\
                             Display Name: {}\n\
                             Path: {}\n\
                             {} files, {} symbols, {} relationships",
                            entry.id,
                            display_name,
                            entry.original_path,
                            file_count,
                            symbol_count,
                            relationship_count
                        );
                        Ok(CallToolResult::text_content(vec![Content::text(
                            message,
                        )]))
                    }
                    Err(e) => {
                        warn!("Failed to index reference workspace: {}", e);
                        let message = format!(
                            "Reference workspace added but indexing failed!\n\
                             Workspace ID: {}\n\
                             Display Name: {}\n\
                             Path: {}\n\
                             Error: {}",
                            entry.id, display_name, entry.original_path, e
                        );
                        Ok(CallToolResult::text_content(vec![Content::text(
                            message,
                        )]))
                    }
                }
            }
            Err(e) => {
                // Registration failed
                let message = format!("Failed to add reference workspace: {}", e);
                Ok(CallToolResult::text_content(vec![Content::text(
                    message,
                )]))
            }
        }
    }

    /// Handle remove command - remove workspace by ID
    pub(crate) async fn handle_remove_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("Removing workspace: {}", workspace_id);

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

        // First check if workspace exists and clean up workspace directory
        if let Ok(Some(_workspace_entry)) = registry_service.get_workspace(workspace_id).await {
            // Delete entire workspace directory: .julie/indexes/{workspace_id}/
            // This removes the separate database and all index data for this workspace
            let workspace_index_path = primary_workspace
                .root
                .join(".julie")
                .join("indexes")
                .join(workspace_id);

            if workspace_index_path.exists() {
                match tokio::fs::remove_dir_all(&workspace_index_path).await {
                    Ok(()) => {
                        info!(
                            "Deleted workspace directory for {}: {:?}",
                            workspace_id, workspace_index_path
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete workspace directory {}: {}",
                            workspace_id, e
                        );
                    }
                }
            }

            // Remove from registry
            match registry_service.unregister_workspace(workspace_id).await {
                Ok(true) => {
                    let message = format!(
                        "Workspace Removed Successfully\n\
                        Workspace: {}\n\
                        Database data cleaned up\n\
                        All associated symbols, files, and relationships have been removed.",
                        workspace_id
                    );
                    Ok(CallToolResult::text_content(vec![Content::text(
                        message,
                    )]))
                }
                Ok(false) => {
                    let message = format!("Workspace not found in registry: {}", workspace_id);
                    Ok(CallToolResult::text_content(vec![Content::text(
                        message,
                    )]))
                }
                Err(e) => {
                    let message = format!("Failed to remove workspace from registry: {}", e);
                    Ok(CallToolResult::text_content(vec![Content::text(
                        message,
                    )]))
                }
            }
        } else {
            let message = format!("Workspace not found: {}", workspace_id);
            Ok(CallToolResult::text_content(vec![Content::text(
                message,
            )]))
        }
    }
}
