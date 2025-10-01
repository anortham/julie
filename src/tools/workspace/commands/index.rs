use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use std::path::Path;
use tracing::{debug, error, info, warn};

/// Calculate the total size of a directory recursively
fn calculate_dir_size(path: &Path) -> u64 {
    let mut total_size = 0u64;

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    total_size += metadata.len();
                } else if metadata.is_dir() {
                    total_size += calculate_dir_size(&entry.path());
                }
            }
        }
    }

    total_size
}

impl ManageWorkspaceTool {
    /// Handle index command - index primary workspace
    pub(crate) async fn handle_index_command(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        force: bool,
    ) -> Result<CallToolResult> {
        info!("📚 Starting workspace indexing...");

        let workspace_path = self.resolve_workspace_path(path)?;
        let force_reindex = force;

        info!("🎯 Resolved workspace path: {}", workspace_path.display());

        // Clear existing state if force reindexing
        if force_reindex {
            info!("🔄 Force reindex requested - clearing existing state");
            *handler.is_indexed.write().await = false;
            // Database will be cleared by initialize_workspace_with_force
        }

        // Only initialize workspace if not already loaded or if forcing reindex
        // This prevents Tantivy lock failures from duplicate initialization
        let workspace_already_loaded = handler.get_workspace().await?.is_some();

        if !workspace_already_loaded || force_reindex {
            handler
                .initialize_workspace_with_force(
                    Some(workspace_path.to_string_lossy().to_string()),
                    force_reindex,
                )
                .await?;
        } else {
            debug!("Workspace already loaded, skipping re-initialization");
        }

        // Check if already indexed and not forcing reindex
        if !force_reindex {
            let is_indexed = *handler.is_indexed.read().await;
            if is_indexed {
                // Get symbol count from database using efficient COUNT(*) query
                let symbol_count = if let Ok(Some(workspace)) = handler.get_workspace().await {
                    if let Some(db) = workspace.db.as_ref() {
                        // Use registry service to get primary workspace ID
                        let registry_service = WorkspaceRegistryService::new(workspace.root.clone());
                        match registry_service.get_primary_workspace_id().await {
                            Ok(Some(workspace_id)) => {
                                let db_lock = db.lock().await;
                                // OPTIMIZED: Use SQL COUNT(*) instead of loading all symbols
                                db_lock.count_symbols_for_workspace(&workspace_id).unwrap_or(0)
                            }
                            _ => {
                                // Fallback: if no workspace ID, count all symbols
                                let db_lock = db.lock().await;
                                db_lock.get_all_symbols().unwrap_or_default().len()
                            }
                        }
                    } else {
                        0
                    }
                } else {
                    0
                };
                let message = format!(
                    "✅ Workspace already indexed!\n\
                    📊 Found {} symbols\n\
                    💡 Use force: true to re-index",
                    symbol_count
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        }

        // Perform indexing
        match self
            .index_workspace_files(handler, &workspace_path, force_reindex)
            .await
        {
            Ok((symbol_count, file_count, relationship_count)) => {
                // Mark as indexed
                *handler.is_indexed.write().await = true;

                // Register as primary workspace and update statistics
                if let Some(workspace) = handler.get_workspace().await? {
                    let registry_service = WorkspaceRegistryService::new(workspace.root.clone());
                    let workspace_path_str = workspace.root.to_string_lossy().to_string();

                    // Try to register (may fail if already registered - that's OK)
                    let workspace_id = match registry_service
                        .register_workspace(workspace_path_str, WorkspaceType::Primary)
                        .await
                    {
                        Ok(entry) => {
                            info!("✅ Registered primary workspace: {}", entry.id);
                            entry.id
                        }
                        Err(_) => {
                            // Already registered - get the existing ID
                            match registry_service.get_primary_workspace_id().await? {
                                Some(id) => id,
                                None => {
                                    warn!("Failed to get primary workspace ID after registration");
                                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                                        "⚠️ Indexing completed but could not update workspace statistics",
                                    )]));
                                }
                            }
                        }
                    };

                    // ALWAYS update statistics after indexing (regardless of registration status)
                    // Calculate actual Tantivy index size
                    let index_size = workspace.julie_dir
                        .join("index/tantivy")
                        .metadata()
                        .map(|_m| calculate_dir_size(&workspace.julie_dir.join("index/tantivy")))
                        .unwrap_or(0);

                    if let Err(e) = registry_service
                        .update_workspace_statistics(&workspace_id, symbol_count, file_count, index_size)
                        .await
                    {
                        warn!("Failed to update workspace statistics: {}", e);
                    } else {
                        info!("✅ Updated workspace statistics: {} files, {} symbols, {} bytes index",
                              file_count, symbol_count, index_size);
                    }
                }

                let message = format!(
                    "🎉 Workspace indexing complete!\n\
                    📁 Indexed {} files\n\
                    🔍 Extracted {} symbols\n\
                    🔗 Found {} relationships\n\
                    ⚡ Ready for search and navigation!",
                    file_count, symbol_count, relationship_count
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
            Err(e) => {
                error!("Failed to index workspace: {}", e);
                let message = format!(
                    "❌ Workspace indexing failed: {}\n\
                    💡 Check that the path exists and contains source files",
                    e
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }
}
