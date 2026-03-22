use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use tracing::{debug, info, warn};

impl ManageWorkspaceTool {
    /// Handle add command — register a reference workspace and index it.
    pub(crate) async fn handle_add_command(
        &self,
        handler: &JulieServerHandler,
        path: &str,
        name: Option<String>,
    ) -> Result<CallToolResult> {
        info!("Adding reference workspace: {}", path);

        // Daemon mode: use DaemonDatabase for registry operations
        if let Some(ref db) = handler.daemon_db {
            let primary_workspace_id = handler.workspace_id.as_deref().unwrap_or("primary");
            let workspace_path = std::path::PathBuf::from(path);
            let path_str = workspace_path
                .canonicalize()
                .unwrap_or_else(|_| workspace_path.clone())
                .to_string_lossy()
                .to_string();

            let ref_workspace_id = match generate_workspace_id(&path_str) {
                Ok(id) => id,
                Err(e) => {
                    let message = format!("Failed to generate workspace ID for {}: {}", path, e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            };

            let dir_name = workspace_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&ref_workspace_id);
            let display_name = name.unwrap_or_else(|| dir_name.to_string());

            // Instant attach: if already indexed, just record the reference relationship
            if let Ok(Some(existing)) = db.get_workspace(&ref_workspace_id) {
                if existing.status == "ready" {
                    debug!(
                        "Reference workspace {} already indexed, instant attach",
                        ref_workspace_id
                    );
                    if let Err(e) = db.add_reference(primary_workspace_id, &ref_workspace_id) {
                        warn!("Failed to record reference relationship: {}", e);
                    }
                    let message = format!(
                        "Reference workspace attached (already indexed)!\n\
                         Workspace ID: {}\n\
                         Display Name: {}\n\
                         Path: {}\n\
                         Files: {} | Symbols: {}",
                        ref_workspace_id,
                        display_name,
                        existing.path,
                        existing.file_count.unwrap_or(0),
                        existing.symbol_count.unwrap_or(0),
                    );
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }

            // Register as indexing before we start
            if let Err(e) = db.upsert_workspace(&ref_workspace_id, &path_str, "indexing") {
                warn!("Failed to register reference workspace in daemon.db: {}", e);
            }

            info!("Starting indexing of reference workspace: {}", display_name);

            match self
                .index_workspace_files(handler, &workspace_path, false)
                .await
            {
                Ok(result) => {
                    // Mark as ready and record stats
                    if let Err(e) = db.update_workspace_status(&ref_workspace_id, "ready") {
                        warn!("Failed to update reference workspace status: {}", e);
                    }
                    if let Err(e) = db.update_workspace_stats(
                        &ref_workspace_id,
                        result.symbols_total as i64,
                        result.files_total as i64,
                        None,
                        None,
                    ) {
                        warn!("Failed to update reference workspace stats: {}", e);
                    }
                    if let Err(e) = db.add_reference(primary_workspace_id, &ref_workspace_id) {
                        warn!("Failed to record reference relationship: {}", e);
                    }

                    let embed_count =
                        crate::tools::workspace::indexing::embeddings::spawn_reference_embedding(
                            handler,
                            ref_workspace_id.clone(),
                        )
                        .await;

                    let mut message = format!(
                        "Reference workspace added and indexed!\n\
                         Workspace ID: {}\n\
                         Display Name: {}\n\
                         Path: {}\n\
                         {} files, {} symbols, {} relationships",
                        ref_workspace_id,
                        display_name,
                        path_str,
                        result.files_total,
                        result.symbols_total,
                        result.relationships_total,
                    );
                    if embed_count > 0 {
                        message.push_str(&format!(
                            "\nEmbedding {} symbols in background...",
                            embed_count
                        ));
                    }
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
                Err(e) => {
                    if let Err(ue) = db.update_workspace_status(&ref_workspace_id, "error") {
                        warn!("Failed to update workspace status to error: {}", ue);
                    }
                    let message = format!(
                        "Reference workspace registered but indexing failed!\n\
                         Workspace ID: {}\n\
                         Display Name: {}\n\
                         Path: {}\n\
                         Error: {}",
                        ref_workspace_id, display_name, path_str, e,
                    );
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }
        }

        // Stdio mode fallback: use WorkspaceRegistryService
        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "No primary workspace found. Please run 'index' command first.";
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
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
                    Ok(result) => {
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
                                entry.id, result.files_total, result.symbols_total, index_size
                            );
                        }

                        let embed_count = crate::tools::workspace::indexing::embeddings::spawn_reference_embedding(
                            handler,
                            entry.id.clone(),
                        ).await;

                        let mut message = format!(
                            "Reference workspace added and indexed!\n\
                             Workspace ID: {}\n\
                             Display Name: {}\n\
                             Path: {}\n\
                             {} files, {} symbols, {} relationships",
                            entry.id,
                            display_name,
                            entry.original_path,
                            result.files_total,
                            result.symbols_total,
                            result.relationships_total
                        );
                        if embed_count > 0 {
                            message.push_str(&format!(
                                "\nEmbedding {} symbols in background...",
                                embed_count
                            ));
                        }
                        Ok(CallToolResult::text_content(vec![Content::text(message)]))
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
                        Ok(CallToolResult::text_content(vec![Content::text(message)]))
                    }
                }
            }
            Err(e) => {
                // Registration failed
                let message = format!("Failed to add reference workspace: {}", e);
                Ok(CallToolResult::text_content(vec![Content::text(message)]))
            }
        }
    }

    /// Handle remove command - remove workspace by ID and clean up index data.
    pub(crate) async fn handle_remove_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("Removing workspace: {}", workspace_id);

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            let primary_workspace_id = handler.workspace_id.as_deref().unwrap_or("primary");

            match db.get_workspace(workspace_id) {
                Ok(Some(ws_row)) => {
                    // Delete index directory. In daemon mode, ref workspace indexes live
                    // under the primary workspace's index root (workspace_index_path).
                    if let Ok(Some(primary_ws)) = handler.get_workspace().await {
                        // indexes_root_path() in daemon mode = ~/.julie/indexes/{primary_id}
                        // Ref workspace index = indexes_root_path()/{ref_id}/db
                        let workspace_index_path =
                            primary_ws.indexes_root_path().join(workspace_id).join("db");
                        let index_dir = workspace_index_path
                            .parent()
                            .unwrap_or(&workspace_index_path);
                        if index_dir.exists() {
                            match tokio::fs::remove_dir_all(index_dir).await {
                                Ok(()) => {
                                    info!("Deleted workspace index for {}", workspace_id);
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to delete workspace directory for {}: {}",
                                        workspace_id, e
                                    );
                                }
                            }
                        }
                    }

                    // Remove reference relationship
                    if let Err(e) = db.remove_reference(primary_workspace_id, workspace_id) {
                        warn!("Failed to remove reference relationship: {}", e);
                    }

                    // Remove from daemon.db
                    if let Err(e) = db.delete_workspace(workspace_id) {
                        let message =
                            format!("Failed to remove workspace from daemon.db: {}", e);
                        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                    }

                    let message = format!(
                        "Workspace Removed Successfully\n\
                        Workspace: {}\n\
                        Path: {}\n\
                        All associated index data removed.",
                        workspace_id, ws_row.path,
                    );
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
                Ok(None) => {
                    let message = format!("Workspace not found: {}", workspace_id);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
                Err(e) => {
                    let message = format!("Failed to look up workspace: {}", e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }
        }

        // Stdio mode fallback: use WorkspaceRegistryService
        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => {
                let message = "No primary workspace found.";
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
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
                    Ok(CallToolResult::text_content(vec![Content::text(message)]))
                }
                Ok(false) => {
                    let message = format!("Workspace not found in registry: {}", workspace_id);
                    Ok(CallToolResult::text_content(vec![Content::text(message)]))
                }
                Err(e) => {
                    let message = format!("Failed to remove workspace from registry: {}", e);
                    Ok(CallToolResult::text_content(vec![Content::text(message)]))
                }
            }
        } else {
            let message = format!("Workspace not found: {}", workspace_id);
            Ok(CallToolResult::text_content(vec![Content::text(message)]))
        }
    }
}
