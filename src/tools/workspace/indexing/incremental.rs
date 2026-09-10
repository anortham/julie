//! Scan the checkout and build the `PathChange` list for one apply.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use julie_index::checkout_store::{CheckoutStore, PathChange};
use tracing::{debug, info, warn};

use super::file_policy::detect_language_for_indexing;
use super::route::IndexRoute;
use super::state::IndexingOperation;
use super::store_open::store_for_workspace;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use crate::utils::paths::to_relative_unix_style;

pub(crate) struct ScannedFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub hash: String,
    pub language: String,
}

pub(crate) fn scan_indexable_files(root: &Path, files: &[PathBuf]) -> Result<Vec<ScannedFile>> {
    let mut scanned = Vec::with_capacity(files.len());
    for file_path in files {
        let bytes = match std::fs::read(file_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                warn!(path = %file_path.display(), error = %err, "skipping unreadable file");
                continue;
            }
        };
        let path = relative_path(file_path, root);
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let language = detect_language_for_indexing(file_path);
        scanned.push(ScannedFile {
            path,
            bytes,
            hash,
            language,
        });
    }
    Ok(scanned)
}

pub(crate) fn existing_path_hashes(store: &CheckoutStore) -> Result<HashMap<String, String>> {
    let facts = store.current().facts()?;
    let mut map = HashMap::new();
    for row in facts.reader().paths()? {
        map.insert(row.path, row.blob_hash);
    }
    Ok(map)
}

/// Upserts for new or changed hashes; removes for paths missing from `scanned`.
/// Incremental skips upserts whose hash already matches.
pub(crate) fn path_changes(
    scanned: &[ScannedFile],
    existing: &HashMap<String, String>,
    operation: IndexingOperation,
) -> Vec<PathChange> {
    let scanned_paths: HashSet<&str> = scanned.iter().map(|file| file.path.as_str()).collect();
    let mut changes = Vec::new();
    let remove_missing = matches!(
        operation,
        IndexingOperation::Full | IndexingOperation::Incremental | IndexingOperation::CatchUp
    );
    if remove_missing {
        for path in existing.keys() {
            if !scanned_paths.contains(path.as_str()) {
                changes.push(PathChange::Remove { path: path.clone() });
            }
        }
    }
    let skip_unchanged = matches!(
        operation,
        IndexingOperation::Incremental | IndexingOperation::CatchUp
    );
    for file in scanned {
        if skip_unchanged {
            if let Some(hash) = existing.get(&file.path) {
                if hash == &file.hash {
                    continue;
                }
            }
        }
        changes.push(PathChange::Upsert {
            path: file.path.clone(),
            bytes: file.bytes.clone(),
            language: file.language.clone(),
        });
    }
    changes
}

pub(crate) fn relative_path(file_path: &Path, root: &Path) -> String {
    if file_path.is_absolute() {
        to_relative_unix_style(file_path, root).unwrap_or_else(|_| {
            file_path
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
                .to_string()
        })
    } else {
        file_path.to_string_lossy().replace('\\', "/")
    }
}

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
