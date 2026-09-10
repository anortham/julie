//! Planning logic for primary workspace catch-up indexing.
//!
//! Compares `paths` to the scanned tree and blob hashes to disk. A failed
//! store open (engine mismatch) means the directory was deleted.

use crate::handler::JulieServerHandler;
use crate::tools::workspace::indexing::state::IndexingRepairReason;
use crate::tools::workspace::indexing::store_open::{
    delete_store_dir, store_dir, store_for_workspace,
};
use anyhow::Result;
use std::collections::HashSet;
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

    let index_dir = handler.workspace_index_dir_for(&route.workspace_id).await?;
    let store_path = store_dir(&index_dir);

    let store = match store_for_workspace(handler, &route.workspace_id, &route.workspace_root).await
    {
        Ok(store) => store,
        Err(err) => {
            warn!(error = %err, "checkout store failed to open; deleting store/ for reindex");
            delete_store_dir(&store_path)?;
            return Ok(Some(PrimaryWorkspaceRepairPlan {
                reasons: vec![IndexingRepairReason::SemanticVersionChanged],
            }));
        }
    };

    let facts = match store.current().facts() {
        Ok(facts) => facts,
        Err(err) => {
            warn!(error = %err, "facts reader failed; deleting store/ for reindex");
            delete_store_dir(&store_path)?;
            return Ok(Some(PrimaryWorkspaceRepairPlan {
                reasons: vec![IndexingRepairReason::SemanticVersionChanged],
            }));
        }
    };
    let path_rows = facts.reader().paths()?;
    if path_rows.is_empty() {
        info!("📊 Store has no paths - indexing needed");
        return Ok(Some(PrimaryWorkspaceRepairPlan {
            reasons: vec![IndexingRepairReason::EmptyDatabase],
        }));
    }

    let stored_paths: HashSet<String> = path_rows.iter().map(|row| row.path.clone()).collect();
    let workspace_files = julie_core::workspace_scan::scan_workspace_files(&route.workspace_root)?;

    let mut reasons = Vec::new();
    if workspace_files.difference(&stored_paths).next().is_some() {
        info!("📊 Found files not in the store - indexing needed");
        reasons.push(IndexingRepairReason::NewFiles);
    }
    if stored_paths.difference(&workspace_files).next().is_some() {
        info!("📊 Found store paths missing on disk - cleanup needed");
        reasons.push(IndexingRepairReason::DeletedFiles);
    }

    let mut stale = false;
    for row in &path_rows {
        if !workspace_files.contains(&row.path) {
            continue;
        }
        let abs = route.workspace_root.join(&row.path);
        match std::fs::read(&abs) {
            Ok(bytes) => {
                let hash = blake3::hash(&bytes).to_hex().to_string();
                if hash != row.blob_hash {
                    stale = true;
                    break;
                }
            }
            Err(_) => {
                stale = true;
                break;
            }
        }
    }
    if stale {
        info!("📊 Blob hash mismatch against disk - indexing needed");
        reasons.push(IndexingRepairReason::StaleFiles);
    }

    let embeddings_possible = handler.embedding_provider().await.is_some()
        || (!handler
            .semantics_disabled
            .load(std::sync::atomic::Ordering::Acquire)
            && !julie_pipeline::embeddings::init::embeddings_disabled_by_env());
    if reasons.is_empty() && embeddings_possible {
        let vector_count = store.status().vector_count;
        if vector_count == 0 {
            let task_already_running = handler
                .embedding_tasks
                .lock()
                .await
                .contains_key(&route.workspace_id);
            if task_already_running {
                debug!("Skipping MissingEmbeddings — embedding task already in flight");
            } else {
                info!("📊 Workspace has symbols but 0 embeddings - scheduling catch-up embedding");
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
