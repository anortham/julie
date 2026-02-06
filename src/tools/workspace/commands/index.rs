use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, error, info, warn};


fn indexing_lock_cache() -> &'static StdMutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>> {
    static LOCKS: OnceLock<StdMutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| StdMutex::new(HashMap::new()))
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

        // Get original path for reference workspace check BEFORE resolution
        // Priority: explicit path > initialized workspace root > JULIE_WORKSPACE env > current_dir
        let original_path = match path {
            Some(ref p) => {
                let expanded = shellexpand::tilde(p).to_string();
                PathBuf::from(expanded)
            }
            None => {
                // Try to use already-initialized workspace root first
                if let Ok(Some(ws)) = handler.get_workspace().await {
                    ws.root.clone()
                } else {
                    // Fall back to JULIE_WORKSPACE env var (same as main.rs get_workspace_root())
                    if let Ok(path_str) = env::var("JULIE_WORKSPACE") {
                        let expanded = shellexpand::tilde(&path_str).to_string();
                        let path = PathBuf::from(expanded);
                        if path.exists() {
                            path.canonicalize().unwrap_or(path)
                        } else {
                            env::current_dir()?
                        }
                    } else {
                        env::current_dir()?
                    }
                }
            }
        };

        // 🔥 CRITICAL FIX: Check if this is a reference workspace FIRST before calling find_workspace_root
        // Reference workspaces don't have .julie/ directories, so find_workspace_root will walk up
        // to the primary workspace and return the wrong path!
        let is_reference_check = if let Ok(Some(primary_ws)) = handler.get_workspace().await {
            let registry_service = WorkspaceRegistryService::new(primary_ws.root.clone());
            registry_service
                .get_workspace_by_path(original_path.to_string_lossy().as_ref())
                .await
                .ok()
                .flatten()
                .map(|entry| entry.workspace_type == WorkspaceType::Reference)
                .unwrap_or(false)
        } else {
            false
        };

        // For reference workspaces, use the original path directly (no workspace root resolution)
        // For primary workspaces, resolve to workspace root using markers
        let workspace_path = if is_reference_check {
            debug!("Reference workspace detected - using original path directly");
            original_path.clone()
        } else {
            self.resolve_workspace_path(path)?
        };

        let canonical_path = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.clone());

        let index_lock = {
            let mut locks = match indexing_lock_cache().lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!(
                        "Indexing lock cache mutex poisoned, recovering: {}",
                        poisoned
                    );
                    poisoned.into_inner()
                }
            };
            locks
                .entry(canonical_path.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone()
        };

        let _index_guard = index_lock.lock().await;
        let force_reindex = force;

        info!("🎯 Resolved workspace path: {}", canonical_path.display());

        // Clear existing state if force reindexing
        if force_reindex {
            info!("🔄 Force reindex requested - clearing existing state");
            *handler.is_indexed.write().await = false;
            // Database will be cleared by initialize_workspace_with_force
        }

        // 🔥 CRITICAL FIX: Only initialize workspace if it's the PRIMARY workspace being indexed
        // Reference workspaces should NEVER reinitialize the handler's workspace!
        // They are indexed into the primary workspace's indexes/{workspace_id}/ directory
        let workspace_already_loaded = handler.get_workspace().await?.is_some();

        // Check if this path is a reference workspace (check ORIGINAL path, not resolved path!)
        let is_reference_workspace = if let Ok(Some(primary_ws)) = handler.get_workspace().await {
            let registry_service = WorkspaceRegistryService::new(primary_ws.root.clone());
            match registry_service
                .get_workspace_by_path(original_path.to_string_lossy().as_ref())
                .await
            {
                Ok(Some(entry)) => {
                    let is_ref = entry.workspace_type == WorkspaceType::Reference;
                    debug!(
                        "Found in registry - workspace_type: {:?}, is_reference: {}",
                        entry.workspace_type, is_ref
                    );
                    is_ref
                }
                Ok(None) => {
                    debug!("Path not found in registry");
                    false
                }
                Err(e) => {
                    debug!("Error checking registry: {}", e);
                    false
                }
            }
        } else {
            debug!("No primary workspace loaded");
            false
        };

        // Only initialize if:
        // 1. Workspace not loaded yet, OR
        // 2. Forcing reindex AND this is NOT a reference workspace
        if !workspace_already_loaded || (force_reindex && !is_reference_workspace) {
            handler
                .initialize_workspace_with_force(
                    Some(canonical_path.to_string_lossy().to_string()),
                    force_reindex,
                )
                .await?;
        } else if is_reference_workspace {
            info!("🔒 Reference workspace detected - keeping handler workspace unchanged");
        }

        // Check if already indexed and not forcing reindex
        if !force_reindex {
            let is_indexed = *handler.is_indexed.read().await;
            if is_indexed {
                // Get symbol count from database using efficient COUNT(*) query
                let symbol_count = if let Ok(Some(workspace)) = handler.get_workspace().await {
                    if let Some(db) = workspace.db.as_ref() {
                        // Use registry service to get primary workspace ID
                        let registry_service =
                            WorkspaceRegistryService::new(workspace.root.clone());
                        match registry_service.get_primary_workspace_id().await {
                            Ok(Some(_workspace_id)) => {
                                let db_lock = match db.lock() {
                                    Ok(guard) => guard,
                                    Err(poisoned) => {
                                        warn!(
                                            "Database mutex poisoned during symbol count, recovering: {}",
                                            poisoned
                                        );
                                        poisoned.into_inner()
                                    }
                                };
                                // OPTIMIZED: Use SQL COUNT(*) instead of loading all symbols
                                db_lock.count_symbols_for_workspace().unwrap_or(0)
                            }
                            _ => {
                                // Fallback: if no workspace ID, count all symbols
                                let db_lock = match db.lock() {
                                    Ok(guard) => guard,
                                    Err(poisoned) => {
                                        warn!(
                                            "Database mutex poisoned during symbol count, recovering: {}",
                                            poisoned
                                        );
                                        poisoned.into_inner()
                                    }
                                };
                                db_lock.get_all_symbols().unwrap_or_default().len()
                            }
                        }
                    } else {
                        0
                    }
                } else {
                    0
                };

                // 🔥 CRITICAL FIX: If database is empty, clear the flag and proceed with indexing
                // This prevents the nonsensical "Workspace already indexed: 0 symbols" message
                if symbol_count == 0 {
                    warn!(
                        "is_indexed flag was true but database has 0 symbols - clearing flag and proceeding with indexing"
                    );
                    *handler.is_indexed.write().await = false;
                    // Fall through to indexing logic below
                } else {
                    let message = format!(
                        "Workspace already indexed: {} symbols\nUse force: true to re-index",
                        symbol_count
                    );
                    return Ok(CallToolResult::text_content(vec![Content::text(
                        message,
                    )]));
                }
            }
        }

        // Perform indexing
        match self
            .index_workspace_files(handler, &canonical_path, force_reindex)
            .await
        {
            Ok((symbol_count, file_count, relationship_count)) => {
                // Mark as indexed
                *handler.is_indexed.write().await = true;

                // Register as primary workspace and update statistics
                if let Some(workspace) = handler.get_workspace().await? {
                    let registry_service = WorkspaceRegistryService::new(workspace.root.clone());

                    // Determine canonical path for lookup/registration
                    let canonical_path_str = canonical_path.to_string_lossy().to_string();

                    // Prefer existing registry entry to avoid redundant registration
                    let workspace_id = if let Some(entry) = registry_service
                        .get_workspace_by_path(&canonical_path_str)
                        .await?
                    {
                        entry.id
                    } else {
                        // Register only if missing (handles reference workspaces)
                        match registry_service
                            .register_workspace(canonical_path_str.clone(), WorkspaceType::Primary)
                            .await
                        {
                            Ok(entry) => {
                                info!("✅ Registered primary workspace: {}", entry.id);
                                entry.id
                            }
                            Err(_) => match registry_service.get_primary_workspace_id().await? {
                                Some(id) => id,
                                None => {
                                    warn!("Failed to get primary workspace ID after registration");
                                    return Ok(CallToolResult::text_content(vec![
                                        Content::text(
                                            "⚠️ Indexing completed but could not update workspace statistics",
                                        ),
                                    ]));
                                }
                            },
                        }
                    };

                    // ALWAYS update statistics after indexing (regardless of registration status)
                    // Move blocking dir size calculation into background task
                    let index_path = workspace.workspace_index_path(&workspace_id);
                    let registry_service_clone = registry_service.clone();
                    let workspace_id_for_stats = workspace_id.clone();
                    tokio::spawn(async move {
                        // 🚨 CRITICAL: Calculate directory size using spawn_blocking
                        // std::fs operations are synchronous blocking I/O
                        let index_path_clone = index_path.clone();
                        let index_size = match tokio::task::spawn_blocking(move || {
                            crate::tools::workspace::calculate_dir_size(&index_path_clone)
                        })
                        .await
                        {
                            Ok(Ok(size)) => size,
                            Ok(Err(e)) => {
                                warn!("Failed to calculate index size: {}", e);
                                0
                            }
                            Err(e) => {
                                warn!("Index size calculation task failed: {}", e);
                                0
                            }
                        };

                        if let Err(e) = registry_service_clone
                            .update_workspace_statistics(
                                &workspace_id_for_stats,
                                symbol_count,
                                file_count,
                                index_size,
                            )
                            .await
                        {
                            warn!("Failed to update workspace statistics: {}", e);
                        } else {
                            info!(
                                "✅ Updated workspace statistics: {} files, {} symbols, {} bytes index",
                                file_count, symbol_count, index_size
                            );
                        }
                    });
                }

                let message = format!(
                    "Workspace indexing complete: {} files, {} symbols, {} relationships\nReady for search and navigation",
                    file_count, symbol_count, relationship_count
                );
                Ok(CallToolResult::text_content(vec![Content::text(
                    message,
                )]))
            }
            Err(e) => {
                error!("Failed to index workspace: {}", e);
                let message = format!(
                    "Workspace indexing failed: {}\nCheck that the path exists and contains source files",
                    e
                );
                Ok(CallToolResult::text_content(vec![Content::text(
                    message,
                )]))
            }
        }
    }
}
