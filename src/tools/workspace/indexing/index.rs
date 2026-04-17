//! Main workspace indexing orchestration
//! Coordinates file discovery, processing, and Tantivy search indexing

use super::pipeline::run_indexing_pipeline;
use super::route::IndexRoute;
use super::state::IndexingOperation;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Result of workspace indexing — distinguishes files processed from DB totals.
pub(crate) struct IndexResult {
    /// Files actually processed in this indexing run (may be 0 if nothing changed)
    pub files_processed: usize,
    /// Orphaned files cleaned from DB (deleted from disk since last index)
    pub orphans_cleaned: usize,
    /// Latest canonical SQLite revision after this indexing run
    pub canonical_revision: Option<i64>,
    /// Total files in the workspace DB after indexing
    pub files_total: usize,
    /// Total symbols in the workspace DB after indexing
    pub symbols_total: usize,
    /// Total relationships in the workspace DB after indexing
    pub relationships_total: usize,
    /// Total indexing duration in milliseconds
    pub duration_ms: u64,
}

impl ManageWorkspaceTool {
    /// Index a workspace by discovering, parsing, and storing file symbols
    ///
    /// This is the main entry point for workspace indexing. It coordinates:
    /// 1. File discovery and filtering
    /// 2. Symbol extraction with optimized parser reuse
    /// 3. Bulk database storage
    /// 4. Search index updates (Tantivy full-text search)
    pub(crate) async fn index_workspace_files(
        &self,
        handler: &JulieServerHandler,
        workspace_path: &Path,
        force_reindex: bool,
    ) -> Result<IndexResult> {
        let index_start = std::time::Instant::now();
        info!("🔍 Scanning workspace: {}", workspace_path.display());

        let route = IndexRoute::for_workspace_path(handler, workspace_path)
            .await
            .map_err(anyhow::Error::new)?;
        debug!(
            workspace_id = %route.workspace_id,
            workspace_root = %route.workspace_root.display(),
            db_path = %route.db_path.display(),
            tantivy_path = %route.tantivy_path.display(),
            is_primary = route.is_primary,
            "Resolved indexing route"
        );

        // Only clear existing data for primary workspace reindex to preserve workspace isolation
        if force_reindex && route.is_primary {
            debug!("Clearing primary workspace for force reindex");
            // Database will be cleared during workspace initialization
        } else if force_reindex {
            debug!("Force reindexing reference workspace");
        }

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
        let (files_to_index, orphans_cleaned) = if force_reindex {
            debug!(
                "Force reindex mode - processing all {} files",
                all_discovered_files.len()
            );
            debug!("🐛 [INDEX TRACE E1] Using all files (force_reindex=true)");
            (all_discovered_files, 0)
        } else {
            debug!("🐛 [INDEX TRACE E2] Calling filter_changed_files");
            let (files, orphans) = self
                .filter_changed_files(handler, all_discovered_files, &route)
                .await?;
            debug!(
                "🐛 [INDEX TRACE E3] filter_changed_files returned {} files, {} orphans cleaned",
                files.len(),
                orphans
            );
            (files, orphans)
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

        // ═══════════════════════════════════════════════════════════════════
        // TANTIVY: Force re-index clears index; normal startup backfills
        // ═══════════════════════════════════════════════════════════════════
        if force_reindex {
            if let Some(search_index) = route.search_index_for_write().await? {
                tokio::task::spawn_blocking(move || {
                    if let Ok(idx) = search_index.lock() {
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
            let database = route.database_for_read(handler).await?;
            let search_index = route.search_index_for_write().await?;
            self.backfill_tantivy_if_needed(
                handler,
                &route.workspace_id,
                database.as_ref(),
                search_index.as_ref(),
            )
            .await?;
        }

        // Proceeding with indexing (parser pool groups files by language for 10-50x speedup)
        debug!("🐛 [INDEX TRACE S] About to call run_indexing_pipeline");
        let indexing_operation = route
            .indexing_runtime
            .as_ref()
            .and_then(|runtime| {
                let snapshot = runtime
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .snapshot();
                if snapshot.catchup_active {
                    Some(IndexingOperation::CatchUp)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                if force_reindex {
                    IndexingOperation::Full
                } else {
                    IndexingOperation::Incremental
                }
            });
        let pipeline_result =
            run_indexing_pipeline(self, handler, files_to_index, &route, indexing_operation)
                .await?;
        let total_files = pipeline_result.files_processed;
        if pipeline_result.state.repair_needed() {
            warn!(
                workspace_id = %route.workspace_id,
                canonical_revision = pipeline_result.canonical_revision,
                repair_files = pipeline_result.state.repair_file_count(),
                "Indexing finished with repair-needed files"
            );
        }
        debug!("🐛 [INDEX TRACE T] run_indexing_pipeline completed");

        // 🚀 NEW ARCHITECTURE: Get final counts from DATABASE, not memory!
        // 🔴 CRITICAL FIX: Query the CORRECT database for reference vs primary workspaces!
        // Reference workspaces have their own separate databases at indexes/{workspace_id}/db/symbols.db
        let (total_symbols, total_files_in_db, total_relationships) = {
            let db_to_query = route.database_for_read(handler).await?;

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
                let stats = db.get_stats().unwrap_or_default();
                (
                    stats.total_symbols as usize,
                    stats.total_files as usize,
                    stats.total_relationships as usize,
                )
            } else {
                (0, 0, 0)
            }
        };

        info!(
            "✅ Indexing complete: {} symbols, {} files, {} relationships stored in SQLite",
            total_symbols, total_files_in_db, total_relationships
        );

        let duration_ms = index_start.elapsed().as_millis() as u64;

        Ok(IndexResult {
            files_processed: total_files,
            orphans_cleaned,
            canonical_revision: pipeline_result.canonical_revision,
            files_total: total_files_in_db,
            symbols_total: total_symbols,
            relationships_total: total_relationships,
            duration_ms,
        })
    }

    /// Ensure the Tantivy projection matches canonical SQLite state.
    ///
    /// This handles empty indexes, stale projection metadata, and revision lag
    /// without requiring a daemon restart or a full tree-sitter re-extract.
    async fn backfill_tantivy_if_needed(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
        database: Option<&Arc<std::sync::Mutex<crate::database::SymbolDatabase>>>,
        search_index: Option<&Arc<std::sync::Mutex<crate::search::SearchIndex>>>,
    ) -> Result<()> {
        let search_index = match search_index {
            Some(idx) => Arc::clone(idx),
            None => return Ok(()),
        };
        let db = match database {
            Some(db) => Arc::clone(db),
            None => return Ok(()),
        };

        let workspace_id = workspace_id.to_string();
        let indexing_status = Arc::clone(&handler.indexing_status);
        tokio::task::spawn_blocking(move || {
            let mut db_lock = db.lock().unwrap_or_else(|p| p.into_inner());
            let idx = search_index.lock().unwrap_or_else(|p| p.into_inner());
            let projection = crate::search::SearchProjection::tantivy(workspace_id);
            projection.ensure_current_with_gate(&mut db_lock, &idx, &indexing_status.search_ready)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Tantivy projection sync task panicked: {}", e))??;

        Ok(())
    }
}
