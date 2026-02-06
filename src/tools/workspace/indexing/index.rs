//! Main workspace indexing orchestration
//! Coordinates file discovery, processing, and Tantivy search indexing

use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};


impl ManageWorkspaceTool {
    /// Index a workspace by discovering, parsing, and storing file symbols
    ///
    /// This is the main entry point for workspace indexing. It coordinates:
    /// 1. File discovery and filtering
    /// 2. Symbol extraction with optimized parser reuse
    /// 3. Bulk database storage
    /// 4. Search index updates (Tantivy full-text search)
    ///
    /// Returns: (total_symbols, total_files, total_relationships)
    pub(crate) async fn index_workspace_files(
        &self,
        handler: &JulieServerHandler,
        workspace_path: &Path,
        force_reindex: bool,
    ) -> Result<(usize, usize, usize)> {
        info!("🔍 Scanning workspace: {}", workspace_path.display());

        // 🔥 CRITICAL DEADLOCK FIX: Call get_workspace() ONCE and reuse throughout function
        // Moved up from later in function so we can use workspace.root for primary detection.
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available for indexing"))?;

        // Check if this is the primary workspace by comparing against the handler's workspace root.
        // Previously compared against std::env::current_dir() which is WRONG — in tests and any
        // scenario where CWD != workspace root, this incorrectly treated the primary workspace
        // as a reference workspace, creating a disconnected SearchIndex whose commits silently failed.
        let workspace_canonical = workspace_path.canonicalize().unwrap_or_else(|_| workspace_path.to_path_buf());
        let root_canonical = workspace.root.canonicalize().unwrap_or_else(|_| workspace.root.clone());
        let is_primary_workspace = workspace_canonical == root_canonical;
        debug!(
            "Workspace comparison: path={:?}, root={:?}, is_primary={}",
            workspace_canonical, root_canonical, is_primary_workspace
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

        // workspace was already acquired at top of function (reusing throughout)

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
        // Same pattern as other indexing operations to avoid lock contention
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

        // ═══════════════════════════════════════════════════════════════════
        // TANTIVY: Force re-index clears index; normal startup backfills
        // ═══════════════════════════════════════════════════════════════════
        if is_primary_workspace {
            if force_reindex {
                // Clear Tantivy so stale entries (e.g. previously-indexed .memories files)
                // don't persist. process_files_optimized will rebuild from discovered files.
                if let Some(ref search_index) = workspace.search_index {
                    let si = search_index.clone();
                    tokio::task::spawn_blocking(move || {
                        if let Ok(idx) = si.lock() {
                            if let Err(e) = idx.clear_all() {
                                tracing::warn!("Failed to clear Tantivy index: {}", e);
                            } else {
                                info!("🗑️  Cleared Tantivy index for force re-index");
                            }
                        }
                    })
                    .await?;
                }
            } else {
                // Normal startup: backfill Tantivy from SQLite if empty (v1→v2 upgrade)
                self.backfill_tantivy_if_needed(&workspace).await?;
            }
        }

        // Proceeding with indexing (parser pool groups files by language for 10-50x speedup)
        debug!("🐛 [INDEX TRACE S] About to call process_files_optimized");
        self.process_files_optimized(
            handler,
            files_to_index,
            is_primary_workspace,
            &mut total_files,
            workspace_id.clone(), // Pass workspace_id to avoid re-lookup
            workspace_path,       // Pass workspace path for correct relative path conversion
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
                            tracing::warn!(
                                "Failed to open reference workspace DB for final count: {}",
                                e
                            );
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
                let db = match db_arc.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!(
                            "Database mutex poisoned during final count query, recovering: {}",
                            poisoned
                        );
                        poisoned.into_inner()
                    }
                };
                let symbols_count = db.get_symbol_count_for_workspace().unwrap_or(0);
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

        Ok((total_symbols, total_files, total_relationships))
    }

    /// Backfill Tantivy search index from SQLite data when Tantivy is empty.
    ///
    /// This handles the v1.x → v2.0 upgrade scenario where SQLite already has
    /// all symbols and file content, but Tantivy was just created empty.
    /// Instead of re-parsing every file with tree-sitter, we read directly
    /// from SQLite — skipping parsing entirely for a ~10x speedup.
    async fn backfill_tantivy_if_needed(
        &self,
        workspace: &crate::workspace::JulieWorkspace,
    ) -> Result<()> {
        let search_index = match &workspace.search_index {
            Some(idx) => Arc::clone(idx),
            None => return Ok(()),
        };
        let db = match &workspace.db {
            Some(db) => Arc::clone(db),
            None => return Ok(()),
        };

        // Quick check: does Tantivy already have data?
        let tantivy_docs = {
            let idx = search_index.lock().unwrap_or_else(|p| p.into_inner());
            idx.num_docs()
        };

        if tantivy_docs > 0 {
            debug!(
                "Tantivy already has {} docs, no backfill needed",
                tantivy_docs
            );
            return Ok(());
        }

        // Tantivy is empty — check if SQLite has data (upgrade scenario)
        let sqlite_symbol_count = {
            let db_lock = db.lock().unwrap_or_else(|p| p.into_inner());
            db_lock.get_symbol_count_for_workspace().unwrap_or(0)
        };

        if sqlite_symbol_count == 0 {
            debug!("Both Tantivy and SQLite empty — first run, no backfill needed");
            return Ok(());
        }

        info!(
            "🔄 Tantivy backfill: index empty but SQLite has {} symbols — populating from database",
            sqlite_symbol_count
        );

        // Read all symbols from SQLite
        let symbols = {
            let db_lock = db.lock().unwrap_or_else(|p| p.into_inner());
            db_lock.get_all_symbols().unwrap_or_default()
        };

        // Read all file contents with language from SQLite
        let file_contents = {
            let db_lock = db.lock().unwrap_or_else(|p| p.into_inner());
            db_lock
                .get_all_file_contents_with_language()
                .unwrap_or_default()
        };

        let symbol_count = symbols.len();
        let file_count = file_contents.len();

        // Populate Tantivy in a blocking task (Tantivy I/O is blocking)
        let backfill_result = tokio::task::spawn_blocking(move || {
            let idx = search_index.lock().unwrap_or_else(|p| p.into_inner());

            // Index all symbols
            let mut symbol_errors = 0;
            for symbol in &symbols {
                let doc = crate::search::SymbolDocument::from_symbol(symbol);
                if let Err(e) = idx.add_symbol(&doc) {
                    symbol_errors += 1;
                    if symbol_errors <= 3 {
                        warn!("Tantivy backfill: failed to add symbol {}: {}", symbol.name, e);
                    }
                }
            }

            // Index all file contents
            let mut file_errors = 0;
            for (path, language, content) in &file_contents {
                let doc = crate::search::FileDocument {
                    file_path: path.clone(),
                    content: content.clone(),
                    language: language.clone(),
                };
                if let Err(e) = idx.add_file_content(&doc) {
                    file_errors += 1;
                    if file_errors <= 3 {
                        warn!("Tantivy backfill: failed to add file {}: {}", path, e);
                    }
                }
            }

            if let Err(e) = idx.commit() {
                warn!("Tantivy backfill: commit failed: {}", e);
                return Err(anyhow::anyhow!("Tantivy backfill commit failed: {}", e));
            }

            if symbol_errors > 0 || file_errors > 0 {
                warn!(
                    "Tantivy backfill completed with errors: {} symbol errors, {} file errors",
                    symbol_errors, file_errors
                );
            }

            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("Tantivy backfill task panicked: {}", e))?;

        backfill_result?;

        info!(
            "✅ Tantivy backfill complete: {} symbols, {} files indexed from SQLite",
            symbol_count, file_count
        );

        Ok(())
    }
}
