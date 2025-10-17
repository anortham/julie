//! Main workspace indexing orchestration
//! Coordinates file discovery, processing, and embedding generation

use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::path::Path;
use tracing::{debug, error, info};

use super::embeddings::generate_embeddings_from_sqlite;

impl ManageWorkspaceTool {
    /// Index a workspace by discovering, parsing, and storing file symbols
    ///
    /// This is the main entry point for workspace indexing. It coordinates:
    /// 1. File discovery and filtering
    /// 2. Symbol extraction with optimized parser reuse
    /// 3. Bulk database storage
    /// 4. Background embedding generation (asynchronous)
    ///
    /// Returns: (total_symbols, total_files, total_relationships)
    pub(crate) async fn index_workspace_files(
        &self,
        handler: &JulieServerHandler,
        workspace_path: &Path,
        force_reindex: bool,
    ) -> Result<(usize, usize, usize)> {
        info!("🔍 Scanning workspace: {}", workspace_path.display());

        // Check if this is the primary workspace (current directory)
        debug!("🐛 [INDEX TRACE A] About to get current_dir");
        let current_dir = std::env::current_dir().unwrap_or_default();
        let is_primary_workspace = workspace_path == current_dir;
        debug!(
            "🐛 [INDEX TRACE B] Got current_dir, is_primary={}",
            is_primary_workspace
        );

        // Log workspace path comparison for debugging
        debug!(
            "Workspace comparison: path={:?}, current_dir={:?}, is_primary={}",
            workspace_path, current_dir, is_primary_workspace
        );

        // Only clear existing data for primary workspace reindex to preserve workspace isolation
        if force_reindex && is_primary_workspace {
            debug!("Clearing primary workspace for force reindex");
            // Database will be cleared during workspace initialization
        } else if force_reindex {
            debug!("Force reindexing reference workspace");
        }

        let mut total_files = 0;

        // Use blacklist-based file discovery
        // 🚨 CRITICAL: File discovery uses std::fs blocking I/O - must run on blocking thread pool
        debug!("🐛 [INDEX TRACE C] About to call discover_indexable_files");
        let workspace_path_clone = workspace_path.to_path_buf();
        let tool_clone = self.clone();
        let all_discovered_files = tokio::task::spawn_blocking(move || {
            tool_clone.discover_indexable_files(&workspace_path_clone)
        })
        .await
        .map_err(|e| anyhow::anyhow!("File discovery task failed: {}", e))??;
        debug!(
            "🐛 [INDEX TRACE D] discover_indexable_files returned {} files",
            all_discovered_files.len()
        );

        info!(
            "📊 Discovered {} files total after filtering",
            all_discovered_files.len()
        );

        // 🚀 INCREMENTAL UPDATE: Filter files that need re-indexing based on hash changes
        debug!(
            "🐛 [INDEX TRACE E] About to filter files, force_reindex={}",
            force_reindex
        );
        let files_to_index = if force_reindex {
            debug!(
                "Force reindex mode - processing all {} files",
                all_discovered_files.len()
            );
            debug!("🐛 [INDEX TRACE E1] Using all files (force_reindex=true)");
            all_discovered_files
        } else {
            debug!("🐛 [INDEX TRACE E2] Calling filter_changed_files");
            let result = self
                .filter_changed_files(handler, all_discovered_files, workspace_path)
                .await?;
            debug!(
                "🐛 [INDEX TRACE E3] filter_changed_files returned {} files",
                result.len()
            );
            result
        };
        debug!(
            "🐛 [INDEX TRACE F] Files filtered, {} files to index",
            files_to_index.len()
        );

        info!(
            "⚡ Need to process {} files (incremental filtering applied)",
            files_to_index.len()
        );

        debug!(
            "🐛 [INDEX TRACE 1] Starting index_workspace_files for path: {:?}",
            workspace_path
        );

        // 🔥 CRITICAL DEADLOCK FIX: Call get_workspace() ONCE and reuse throughout function
        // Calling get_workspace() multiple times causes lock contention and deadlocks
        debug!("🐛 [INDEX TRACE G] About to get workspace for ID generation (ONCE)");
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available for indexing"))?;
        debug!("🐛 [INDEX TRACE H] Got workspace successfully (reusing throughout function)");

        // Get workspace ID early for use throughout the function
        // CRITICAL DEADLOCK FIX: Generate workspace ID directly to avoid registry lock contention
        // CRITICAL FIX: Use the workspace_path parameter to determine canonical path
        // This ensures we get the correct workspace_id for BOTH primary and reference workspaces
        debug!("🐛 [INDEX TRACE I] Canonicalizing path");
        let canonical_path = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf())
            .to_string_lossy()
            .to_string();

        // DEADLOCK FIX: Generate workspace ID directly from path (no registry access)
        // Same pattern as search_workspace_tantivy and filter_changed_files
        debug!(
            "🐛 [INDEX TRACE J] Generating workspace ID directly from: {}",
            canonical_path
        );
        let workspace_id = match crate::workspace::registry::generate_workspace_id(&canonical_path)
        {
            Ok(id) => {
                debug!("🐛 [INDEX TRACE K] Generated workspace ID: {}", id);
                id
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to generate workspace ID for path {}: {}",
                    canonical_path,
                    e
                ));
            }
        };
        debug!("🐛 [INDEX TRACE L] workspace_id obtained: {}", workspace_id);

        // Tantivy removed - proceeding with SQLite-only indexing
        debug!("🐛 [INDEX TRACE S] About to call process_files_optimized");
        // PERFORMANCE OPTIMIZATION: Group files by language and use parser pool for 10-50x speedup
        self.process_files_optimized(
            handler,
            files_to_index,
            is_primary_workspace,
            &mut total_files,
            workspace_id.clone(), // Pass workspace_id to avoid re-lookup
        )
        .await?;
        debug!("🐛 [INDEX TRACE T] process_files_optimized completed");

        // 🚀 NEW ARCHITECTURE: Get final counts from DATABASE, not memory!
        // 🔴 CRITICAL FIX: Query the CORRECT database for reference vs primary workspaces!
        // Reference workspaces have their own separate databases at indexes/{workspace_id}/db/symbols.db
        let (total_symbols, total_relationships) = {
            // Determine which database to query based on workspace type
            let db_to_query = if is_primary_workspace {
                // Primary workspace - use handler's database connection
                workspace.db.clone()
            } else {
                // Reference workspace - must have been created in process_files_optimized
                // Get the reference workspace database we just indexed
                let ref_db_path = workspace.workspace_db_path(&workspace_id);
                if ref_db_path.exists() {
                    // Open the reference workspace database for reading final counts
                    match tokio::task::spawn_blocking(move || {
                        crate::database::SymbolDatabase::new(ref_db_path)
                    })
                    .await
                    {
                        Ok(Ok(db)) => Some(std::sync::Arc::new(std::sync::Mutex::new(db))),
                        Ok(Err(e)) => {
                            tracing::warn!("Failed to open reference workspace DB for final count: {}", e);
                            None
                        }
                        Err(e) => {
                            tracing::warn!("Reference workspace DB open task failed: {}", e);
                            None
                        }
                    }
                } else {
                    tracing::warn!("Reference workspace database not found at expected path");
                    None
                }
            };

            // Query the correct database
            if let Some(db_arc) = db_to_query {
                let db = db_arc.lock().unwrap();
                let symbols_count = db
                    .get_symbol_count_for_workspace()
                    .unwrap_or(0);
                let stats = db.get_stats().unwrap_or_default();
                (symbols_count as usize, stats.total_relationships as usize)
            } else {
                (0, 0)
            }
        };

        info!(
            "✅ Indexing complete: {} symbols, {} relationships stored in SQLite",
            total_symbols, total_relationships
        );

        // 🔥 STALENESS CHECK: Only generate embeddings for symbols that don't have them yet
        // This fixes the bug where embeddings were regenerated on EVERY startup
        let symbols_needing_embeddings = if let Some(db_arc) = if is_primary_workspace {
            workspace.db.clone()
        } else {
            let ref_db_path = workspace.workspace_db_path(&workspace_id);
            if ref_db_path.exists() {
                match tokio::task::spawn_blocking(move || {
                    crate::database::SymbolDatabase::new(ref_db_path)
                })
                .await
                {
                    Ok(Ok(db)) => Some(std::sync::Arc::new(std::sync::Mutex::new(db))),
                    _ => None,
                }
            } else {
                None
            }
        } {
            let db_lock = db_arc.lock().unwrap();
            db_lock
                .get_symbols_without_embeddings()
                .unwrap_or_default()
                .len()
        } else {
            0
        };

        // 🔥 BACKGROUND TASK: Generate embeddings from SQLite (optional, compute-intensive)
        // Now runs for ALL workspaces (primary and reference) - BUT ONLY IF NEEDED!
        if symbols_needing_embeddings > 0 {
            let workspace_type = if is_primary_workspace {
                "primary"
            } else {
                "reference"
            };
            info!(
                "🚀 Starting background embedding generation for {} new symbols in {} workspace: {}",
                symbols_needing_embeddings, workspace_type, workspace_id
            );

            // Clone necessary references for background task
            // Use the workspace variable we already fetched (DEADLOCK FIX: no re-lock)
            let embedding_engine = handler.embedding_engine.clone();
            let embedding_engine_last_used = handler.embedding_engine_last_used.clone();

            // 🔴 CRITICAL FIX: Pass correct database for reference vs primary workspaces!
            // Reference workspaces need their own database, not the primary's
            let workspace_db = if is_primary_workspace {
                // Primary workspace - use handler's database
                workspace.db.clone()
            } else {
                // Reference workspace - open its separate database for embedding generation
                let ref_db_path = workspace.workspace_db_path(&workspace_id);
                if ref_db_path.exists() {
                    match tokio::task::spawn_blocking(move || {
                        crate::database::SymbolDatabase::new(ref_db_path)
                    })
                    .await
                    {
                        Ok(Ok(db)) => Some(std::sync::Arc::new(std::sync::Mutex::new(db))),
                        Ok(Err(e)) => {
                            tracing::warn!("Failed to open reference workspace DB for embeddings: {}", e);
                            None
                        }
                        Err(e) => {
                            tracing::warn!("Reference workspace DB open task failed for embeddings: {}", e);
                            None
                        }
                    }
                } else {
                    tracing::warn!("Reference workspace database not found for embeddings");
                    None
                }
            };

            let workspace_root = Some(workspace.root.clone());
            let workspace_id_clone = workspace_id.clone();
            let indexing_status_clone = handler.indexing_status.clone();

            tokio::spawn(async move {
                info!(
                    "🐛 Background embedding task started for workspace: {}",
                    workspace_id_clone
                );
                let task_start = std::time::Instant::now();

                // 🔥 CRITICAL: Add 5-minute timeout to prevent infinite loops
                // Background embedding should complete in <2min for 10k symbols
                // If it takes >5min, something is seriously wrong
                match tokio::time::timeout(
                    std::time::Duration::from_secs(300), // 5 minute timeout
                    generate_embeddings_from_sqlite(
                        embedding_engine,
                        embedding_engine_last_used,
                        workspace_db,
                        workspace_root,
                        workspace_id_clone.clone(),
                        indexing_status_clone,
                    ),
                )
                .await
                {
                    Ok(Ok(_)) => {
                        info!("✅ Embeddings generated from SQLite in {:.2}s for workspace {} - semantic search available!",
                              task_start.elapsed().as_secs_f64(), workspace_id_clone);
                    }
                    Ok(Err(e)) => {
                        error!(
                            "❌ Background embedding generation failed for workspace {}: {}",
                            workspace_id_clone, e
                        );
                    }
                    Err(_) => {
                        error!(
                            "⏰ Background embedding generation TIMED OUT after 5min for workspace {}! \
                             This indicates a serious bug (infinite loop or deadlock). \
                             Semantic search will not be available. Check logs for details.",
                            workspace_id_clone
                        );
                    }
                }
                info!(
                    "🐛 Background embedding task completed for workspace: {}",
                    workspace_id_clone
                );
            });
        }

        Ok((total_symbols, total_files, total_relationships))
    }
}
