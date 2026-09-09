//! Planning logic for primary workspace repair and catch-up indexing.

use crate::handler::JulieServerHandler;
use crate::tools::workspace::indexing::engine_version::{
    SEMANTIC_INDEX_ENGINE_COMPONENT, SEMANTIC_INDEX_ENGINE_VERSION,
};
use crate::tools::workspace::indexing::state::IndexingRepairReason;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrimaryWorkspaceRepairPlan {
    pub reasons: Vec<IndexingRepairReason>,
}

pub(crate) async fn plan_primary_workspace_repair(
    handler: &JulieServerHandler,
) -> Result<Option<PrimaryWorkspaceRepairPlan>> {
    let route =
        match crate::tools::workspace::indexing::route::IndexRoute::for_current_primary(handler)
            .await
        {
            Ok(route) => route,
            Err(err) => {
                if handler.get_workspace().await?.is_none() {
                    debug!("No workspace found - indexing needed");
                    return Ok(Some(PrimaryWorkspaceRepairPlan {
                        reasons: vec![IndexingRepairReason::EmptyDatabase],
                    }));
                }

                return Err(anyhow::Error::new(err));
            }
        };

    let current_primary_root = route.workspace_root.clone();
    let db_path = route.db_path.clone();
    let db_arc = match route.database_for_read(handler).await? {
        Some(db) => db,
        None => {
            debug!(
                "No database at {} for workspace '{}' — initial indexing needed",
                db_path.display(),
                route.workspace_id
            );
            return Ok(Some(PrimaryWorkspaceRepairPlan {
                reasons: vec![IndexingRepairReason::EmptyDatabase],
            }));
        }
    };

    let (has_symbols_result, semantic_version_matches, indexed_files_raw, stored_repairs) = {
        let db: std::sync::MutexGuard<'_, crate::database::SymbolDatabase> = match db_arc.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned during startup check, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };

        let has_symbols_result = db.has_symbols_for_workspace();
        match has_symbols_result {
            Ok(true) => (
                Ok(true),
                Some(db.index_engine_version_matches(
                    &route.workspace_id,
                    SEMANTIC_INDEX_ENGINE_COMPONENT,
                    SEMANTIC_INDEX_ENGINE_VERSION,
                )?),
                db.get_all_indexed_files()?,
                db.list_indexing_repairs()?,
            ),
            Ok(false) => (Ok(false), None, Vec::new(), db.list_indexing_repairs()?),
            Err(err) => (Err(err), None, Vec::new(), Vec::new()),
        }
    };

    match has_symbols_result {
        Ok(has_symbols) => {
            if !has_symbols {
                info!("📊 Database is empty - indexing needed");
                let mut reasons = vec![IndexingRepairReason::EmptyDatabase];
                for repair in stored_repairs {
                    if let Some(reason) = IndexingRepairReason::from_str(&repair.reason) {
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                }
                return Ok(Some(PrimaryWorkspaceRepairPlan { reasons }));
            }

            let mut reasons = Vec::new();

            if !semantic_version_matches.unwrap_or(false) {
                info!("📊 Index semantic version changed or missing - full indexing needed");
                reasons.push(IndexingRepairReason::SemanticVersionChanged);
            }

            let db_mtime = get_database_mtime(&db_path)?;
            let max_file_mtime = get_max_file_mtime_in_workspace(&current_primary_root)?;

            debug!(
                "Staleness check: db_mtime={:?}, max_file_mtime={:?}, stale={}",
                db_mtime,
                max_file_mtime,
                max_file_mtime > db_mtime
            );

            if max_file_mtime > db_mtime {
                info!("📊 Database is stale (files modified after last index) - indexing needed");
                reasons.push(IndexingRepairReason::StaleFiles);
            }

            let indexed_files: HashSet<String> = indexed_files_raw.into_iter().collect();

            let workspace_files =
                julie_core::workspace_scan::scan_workspace_files(&current_primary_root)?;
            let new_files: Vec<_> = workspace_files.difference(&indexed_files).collect();

            debug!(
                "New file check: indexed={}, workspace={}, new={}",
                indexed_files.len(),
                workspace_files.len(),
                new_files.len()
            );

            if !new_files.is_empty() {
                info!(
                    "📊 Found {} new files not in database - indexing needed",
                    new_files.len()
                );
                debug!("New files: {:?}", new_files);
                reasons.push(IndexingRepairReason::NewFiles);
            }

            let deleted_files: Vec<_> = indexed_files.difference(&workspace_files).collect();

            if !deleted_files.is_empty() {
                info!(
                    "📊 Found {} deleted files still in database - cleanup needed",
                    deleted_files.len()
                );
                debug!("Deleted files: {:?}", deleted_files);
                reasons.push(IndexingRepairReason::DeletedFiles);
            }

            for repair in stored_repairs {
                if let Some(reason) = IndexingRepairReason::from_str(&repair.reason) {
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }

            if reasons.is_empty() {
                let embedding_count = match db_arc.lock() {
                    Ok(db) => db.embedding_count().unwrap_or(0),
                    Err(poisoned) => poisoned.into_inner().embedding_count().unwrap_or(0),
                };
                if embedding_count == 0 {
                    let workspace_id = handler.require_primary_workspace_identity().ok();
                    let task_already_running = match workspace_id.as_ref() {
                        Some(ws_id) => handler.embedding_tasks.lock().await.contains_key(ws_id),
                        None => false,
                    };

                    if task_already_running {
                        debug!(
                            "Skipping MissingEmbeddings — embedding task already in flight \
                             for the primary workspace"
                        );
                    } else {
                        info!(
                            "📊 Workspace has symbols but 0 embeddings - scheduling catch-up embedding"
                        );
                        reasons.push(IndexingRepairReason::MissingEmbeddings);
                    }
                }
            }

            if reasons.is_empty() {
                info!("✅ Index is up-to-date - no indexing needed");
                Ok(None)
            } else {
                Ok(Some(PrimaryWorkspaceRepairPlan { reasons }))
            }
        }
        Err(e) => {
            debug!(
                "Error checking database symbols: {} - assuming indexing needed",
                e
            );
            Ok(Some(PrimaryWorkspaceRepairPlan {
                reasons: vec![IndexingRepairReason::EmptyDatabase],
            }))
        }
    }
}

fn get_database_mtime(db_path: &Path) -> Result<SystemTime> {
    if !db_path.exists() {
        return Ok(SystemTime::UNIX_EPOCH);
    }

    let metadata = std::fs::metadata(&db_path)
        .with_context(|| format!("Failed to get metadata for database: {}", db_path.display()))?;

    metadata
        .modified()
        .with_context(|| format!("Failed to get mtime for database: {}", db_path.display()))
}

fn get_max_file_mtime_in_workspace(workspace_root: &Path) -> Result<SystemTime> {
    use crate::utils::walk::{WalkConfig, build_walker};

    let mut max_mtime = SystemTime::UNIX_EPOCH;

    for result in build_walker(workspace_root, &WalkConfig::stale_scan()) {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        if !julie_core::workspace_scan::is_code_file(entry.path()) {
            continue;
        }

        if let Ok(metadata) = std::fs::metadata(entry.path()) {
            if let Ok(mtime) = metadata.modified() {
                if mtime > max_mtime {
                    max_mtime = mtime;
                }
            }
        }
    }

    Ok(max_mtime)
}
