//! Main workspace indexing orchestration
//! Coordinates file discovery, processing, and Tantivy search indexing

use super::engine_version::{SEMANTIC_INDEX_ENGINE_COMPONENT, SEMANTIC_INDEX_ENGINE_VERSION};
use super::pipeline::run_indexing_pipeline;
use super::route::{IndexRoute, IndexRouteRepairReason};
use super::state::{IndexingOperation, IndexingRepairReason};
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::{debug, info, warn};

/// Result of workspace indexing, distinguishing files processed from DB totals.
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
    pub(crate) async fn semantic_index_engine_refresh_needed_for_path(
        &self,
        handler: &JulieServerHandler,
        workspace_path: &Path,
    ) -> Result<bool> {
        let route = match IndexRoute::for_workspace_path(handler, workspace_path).await {
            Ok(route) => route,
            Err(err)
                if matches!(
                    err.reason,
                    IndexRouteRepairReason::PrimaryBindingUnavailable
                        | IndexRouteRepairReason::StorageAnchorUnavailable
                ) =>
            {
                debug!(
                    workspace_path = %workspace_path.display(),
                    "Skipping semantic engine preflight because no readable index route exists yet"
                );
                return Ok(false);
            }
            Err(err) => return Err(anyhow::Error::new(err)),
        };
        semantic_index_engine_refresh_needed(handler, &route).await
    }

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
            debug!("Force reindexing target workspace");
        }

        // Use blacklist-based file discovery
        // 🚨 CRITICAL: File discovery uses std::fs blocking I/O - must run on blocking thread pool
        debug!("🐛 [INDEX TRACE C] About to call discover_indexable_files");
        let workspace_path_clone = workspace_path.to_path_buf();
        let tool_clone = self.clone();
        let write_julieignore = !handler
            .suppress_workspace_file_writes
            .load(Ordering::Relaxed);
        let all_discovered_files = tokio::task::spawn_blocking(move || {
            if write_julieignore {
                tool_clone.discover_indexable_files(&workspace_path_clone)
            } else {
                tool_clone.discover_indexable_files_with_options(&workspace_path_clone, false)
            }
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

        let semantic_engine_refresh_needed =
            semantic_index_engine_refresh_needed(handler, &route).await?;
        if semantic_engine_refresh_needed {
            info!(
                workspace_id = %route.workspace_id,
                component = SEMANTIC_INDEX_ENGINE_COMPONENT,
                expected_version = SEMANTIC_INDEX_ENGINE_VERSION,
                "Index semantic version changed or missing; forcing full re-index"
            );
        }
        let effective_force_reindex = force_reindex || semantic_engine_refresh_needed;

        // 🚀 INCREMENTAL UPDATE: Filter files that need re-indexing based on hash changes
        debug!(
            "🐛 [INDEX TRACE E] About to filter files, force_reindex={}",
            effective_force_reindex
        );
        let (files_to_index, orphans_cleaned) = if effective_force_reindex {
            debug!(
                "Force reindex mode - processing all {} files",
                all_discovered_files.len()
            );
            debug!("🐛 [INDEX TRACE E1] Using all files (effective_force_reindex=true)");
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
        if effective_force_reindex {
            if let Some(search_index) = route.search_index_for_write().await? {
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = search_index.clear_all() {
                        tracing::warn!("Failed to clear Tantivy index: {}", e);
                    } else {
                        info!("🗑️  Cleared Tantivy index for force re-index");
                    }
                })
                .await?;
            }
        } else {
            let database = route.database_for_read(handler).await?;
            let search_index = route
                .search_index_for_write()
                .await
                .context("opening Tantivy for startup projection backfill")?;
            let backfill_result = self
                .backfill_tantivy_if_needed(
                    handler,
                    &route.workspace_id,
                    database.as_ref(),
                    search_index.as_ref(),
                )
                .await;
            let release_result = if let Some(search_index) = &search_index {
                search_index.release_writer()
            } else {
                Ok(())
            };
            if let Err(err) = backfill_result {
                if let Err(release_err) = release_result {
                    warn!(
                        "Failed to release Tantivy writer after startup projection backfill error: {}",
                        release_err
                    );
                }
                return Err(err);
            }
            release_result.context("releasing Tantivy writer after startup projection backfill")?;
        }

        if !effective_force_reindex && files_to_index.is_empty() && orphans_cleaned == 0 {
            let (total_symbols, total_files_in_db, total_relationships, canonical_revision) =
                current_index_totals(handler, &route).await?;
            handler
                .indexing_status
                .search_ready
                .store(true, Ordering::Release);
            info!(
                "✅ Indexing skipped: no changed files; {} symbols, {} files, {} relationships already stored in SQLite",
                total_symbols, total_files_in_db, total_relationships
            );

            return Ok(IndexResult {
                files_processed: 0,
                orphans_cleaned,
                canonical_revision,
                files_total: total_files_in_db,
                symbols_total: total_symbols,
                relationships_total: total_relationships,
                duration_ms: index_start.elapsed().as_millis() as u64,
            });
        }

        // Proceeding with indexing (parser pool groups files by language for 10-50x speedup)
        debug!("🐛 [INDEX TRACE S] About to call run_indexing_pipeline");
        let indexing_operation = route
            .indexing_runtime
            .as_ref()
            .and_then(|runtime| {
                if effective_force_reindex {
                    return None;
                }
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
                if effective_force_reindex {
                    IndexingOperation::Full
                } else {
                    IndexingOperation::Incremental
                }
            });
        let pipeline_result =
            run_indexing_pipeline(self, handler, files_to_index, &route, indexing_operation)
                .await
                .context("running indexing pipeline after projection backfill")?;
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

        record_current_index_engine_version(handler, &route).await?;

        // 🚀 NEW ARCHITECTURE: Get final counts from DATABASE, not memory!
        // 🔴 CRITICAL FIX: Query the correct database for target vs primary workspaces.
        // Target workspaces have their own separate databases at indexes/{workspace_id}/db/symbols.db
        let (total_symbols, total_files_in_db, total_relationships, _) =
            current_index_totals(handler, &route).await?;

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
        search_index: Option<&Arc<crate::search::SearchIndex>>,
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
            let idx = search_index;
            let projection = crate::search::SearchProjection::tantivy(workspace_id);
            projection.ensure_current_with_gate(&mut db_lock, &idx, &indexing_status.search_ready)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Tantivy projection sync task panicked: {}", e))??;

        Ok(())
    }
}

async fn semantic_index_engine_refresh_needed(
    handler: &JulieServerHandler,
    route: &IndexRoute,
) -> Result<bool> {
    let db_to_query = route.database_for_read(handler).await?;

    let Some(db_arc) = db_to_query else {
        return Ok(false);
    };

    let db = match db_arc.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(
                "Database mutex poisoned during semantic engine version check, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };
    let stats = db.get_stats()?;
    let has_persisted_index_state =
        stats.total_files > 0 || stats.total_symbols > 0 || stats.total_relationships > 0;
    if !has_persisted_index_state {
        return Ok(false);
    }

    Ok(!db.index_engine_version_matches(
        &route.workspace_id,
        SEMANTIC_INDEX_ENGINE_COMPONENT,
        SEMANTIC_INDEX_ENGINE_VERSION,
    )?)
}

async fn record_current_index_engine_version(
    handler: &JulieServerHandler,
    route: &IndexRoute,
) -> Result<()> {
    let Some(db_arc) = route.database_for_write(handler).await? else {
        return Ok(());
    };

    let db = match db_arc.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(
                "Database mutex poisoned while storing semantic engine version, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };
    db.set_index_engine_version(
        &route.workspace_id,
        SEMANTIC_INDEX_ENGINE_COMPONENT,
        SEMANTIC_INDEX_ENGINE_VERSION,
    )?;

    if let Some(runtime) = route.indexing_runtime.as_ref() {
        runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear_repair_reason(IndexingRepairReason::SemanticVersionChanged);
    }

    Ok(())
}

async fn current_index_totals(
    handler: &JulieServerHandler,
    route: &IndexRoute,
) -> Result<(usize, usize, usize, Option<i64>)> {
    let db_to_query = route.database_for_read(handler).await?;

    let Some(db_arc) = db_to_query else {
        return Ok((0, 0, 0, None));
    };

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
    let canonical_revision = db.get_current_canonical_revision(&route.workspace_id)?;
    Ok((
        stats.total_symbols as usize,
        stats.total_files as usize,
        stats.total_relationships as usize,
        canonical_revision,
    ))
}
