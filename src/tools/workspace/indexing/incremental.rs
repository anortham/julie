//! Scan the checkout and build the `PathChange` list for one apply.

use std::path::PathBuf;

use anyhow::Result;
use julie_index::checkout_store::PathChange;
pub(crate) use julie_runtime::workspace::reconcile::{
    ScannedFile, existing_path_hashes, path_changes, scan_indexable_files,
};
use tracing::{debug, info};

use super::route::IndexRoute;
use super::state::IndexingOperation;
use super::store_open::store_for_workspace;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;

impl ManageWorkspaceTool {
    /// Files whose hash differs from the store, plus the count of store paths
    /// missing from disk. Does not write.
    pub(crate) async fn filter_changed_files(
        &self,
        handler: &JulieServerHandler,
        all_files: Vec<PathBuf>,
        route: &IndexRoute,
    ) -> Result<(Vec<PathBuf>, usize)> {
        let store =
            store_for_workspace(handler, &route.workspace_id, &route.workspace_root).await?;
        let existing = existing_path_hashes(&store)?;
        if existing.is_empty() {
            info!(
                "🔄 Store has 0 paths - indexing all {} files",
                all_files.len()
            );
            return Ok((all_files, 0));
        }
        let scanned = scan_indexable_files(&route.workspace_root, &all_files)?;
        let changes = path_changes(&scanned, &existing, IndexingOperation::Incremental);
        let mut files_to_process = Vec::new();
        let mut orphans = 0;
        for change in &changes {
            match change {
                PathChange::Upsert { path, .. } => {
                    files_to_process.push(route.workspace_root.join(path));
                }
                PathChange::Remove { .. } => orphans += 1,
            }
        }
        debug!(
            "Checking {} files against {} store paths; {} changed, {} missing",
            all_files.len(),
            existing.len(),
            files_to_process.len(),
            orphans
        );
        Ok((files_to_process, orphans))
    }
}
