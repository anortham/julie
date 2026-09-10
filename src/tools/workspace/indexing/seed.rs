//! Seed a new checkout's index from a sibling checkout of the same repository.
//!
//! Copies the sibling's `symbols.db` and `tantivy/` directory, rebinds the
//! per-workspace state rows, then runs the ordinary incremental scan so only
//! files that differ from the sibling are re-extracted.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use julie_core::workspace::git_identity::git_common_dir;
use julie_core::workspace::mutation_gate::MutationGuard;
use rusqlite::{Connection, OpenFlags, params};
use tracing::{info, warn};

use super::index::IndexResult;
use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;
use crate::registry::workspace_registry_store::WorkspaceRegistryStore;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SeedReport {
    pub sibling_root: PathBuf,
    pub copied_files: usize,
    pub reextracted_files: usize,
    pub removed_files: usize,
    pub elapsed_ms: u128,
}

impl fmt::Display for SeedReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Seeded from {}: {} files copied, {} re-extracted, {} removed in {} ms",
            self.sibling_root.display(),
            self.copied_files,
            self.reextracted_files,
            self.removed_files,
            self.elapsed_ms
        )
    }
}

/// The registered, ready checkout of the same repository indexed most recently.
pub(crate) fn sibling_checkout(registry: &WorkspaceRegistryStore, root: &Path) -> Option<PathBuf> {
    let common = git_common_dir(root)?;
    let root = root.canonicalize().ok()?;
    registry
        .list_workspaces()
        .ok()?
        .into_iter()
        .filter(|row| row.status == "ready" && Path::new(&row.path) != root)
        .filter(|row| git_common_dir(Path::new(&row.path)).as_deref() == Some(common.as_path()))
        .max_by_key(|row| row.last_indexed.unwrap_or(0))
        .map(|row| PathBuf::from(row.path))
}

/// Recursively copy `from` into `to`, skipping the `.tantivy-*` writer marker files.
pub(crate) fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(".tantivy-") {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn checkpoint_sibling(db_path: &Path) -> Result<bool> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let busy: i64 = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
    Ok(busy == 0)
}

fn rebind_workspace_rows(db_path: &Path, workspace_id: &str, root: &Path) -> Result<()> {
    let db = SymbolDatabase::new(db_path)?;
    let path = root.to_string_lossy().to_string();
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    db.conn.execute(
        "UPDATE workspaces SET id = ?1, path = ?2, name = ?3",
        params![workspace_id, path, name],
    )?;
    db.conn.execute(
        "UPDATE index_engine_state SET workspace_id = ?1",
        params![workspace_id],
    )?;
    db.conn.execute(
        "UPDATE projection_states SET workspace_id = ?1",
        params![workspace_id],
    )?;
    Ok(())
}

impl ManageWorkspaceTool {
    /// Seed `root` from a registered sibling when `root` has no index yet.
    pub(crate) async fn seed_if_index_missing(
        &self,
        handler: &JulieServerHandler,
        root: &Path,
        guard: &MutationGuard<'_>,
    ) -> Result<Option<(SeedReport, IndexResult)>> {
        let workspace_id = generate_workspace_id(&root.to_string_lossy())?;
        let index_root = handler.workspace_index_dir_for(&workspace_id).await?;
        if index_root.join("db").join("symbols.db").exists() {
            return Ok(None);
        }
        let Some(registry) =
            crate::tools::workspace::commands::registry::registry_store_for_handler(handler)?
        else {
            return Ok(None);
        };
        let sibling = sibling_checkout(&registry, root);
        self.seed_from_sibling(handler, root, &index_root, sibling.as_deref(), guard)
            .await
    }

    /// Copy the sibling's index into `index_root`, then run the incremental scan on `root`.
    ///
    /// Returns `None` when there is no usable sibling, so the caller falls back
    /// to indexing from scratch.
    pub(crate) async fn seed_from_sibling(
        &self,
        handler: &JulieServerHandler,
        root: &Path,
        index_root: &Path,
        sibling: Option<&Path>,
        guard: &MutationGuard<'_>,
    ) -> Result<Option<(SeedReport, IndexResult)>> {
        let Some(sibling) = sibling else {
            return Ok(None);
        };
        let started = Instant::now();
        let sibling_id = generate_workspace_id(&sibling.to_string_lossy())?;
        let sibling_index_root = handler.workspace_index_dir_for(&sibling_id).await?;
        let sibling_db = sibling_index_root.join("db").join("symbols.db");
        if !sibling_db.exists() {
            return Ok(None);
        }
        match checkpoint_sibling(&sibling_db) {
            Ok(true) => {}
            Ok(false) | Err(_) => {
                warn!(sibling = %sibling.display(), "Sibling index busy; indexing from scratch");
                return Ok(None);
            }
        }

        let workspace_id = generate_workspace_id(&root.to_string_lossy())?;
        let target_db = index_root.join("db").join("symbols.db");
        let copy = {
            let (sibling_index_root, index_root, target_db, root) = (
                sibling_index_root.clone(),
                index_root.to_path_buf(),
                target_db.clone(),
                root.to_path_buf(),
            );
            let workspace_id = workspace_id.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                std::fs::create_dir_all(index_root.join("db"))?;
                std::fs::copy(&sibling_db, &target_db)?;
                let sibling_tantivy = sibling_index_root.join("tantivy");
                if sibling_tantivy.is_dir() {
                    copy_dir(&sibling_tantivy, &index_root.join("tantivy"))?;
                }
                rebind_workspace_rows(&target_db, &workspace_id, &root)
            })
            .await
            .context("sibling seed copy task panicked")?
        };
        if let Err(err) = copy {
            warn!(error = %err, "Sibling seed copy failed; indexing from scratch");
            let _ = std::fs::remove_dir_all(index_root);
            return Ok(None);
        }

        let result = self
            .index_workspace_files(handler, root, false, guard)
            .await?;
        let report = SeedReport {
            sibling_root: sibling.to_path_buf(),
            copied_files: result.files_total.saturating_sub(result.files_processed),
            reextracted_files: result.files_processed,
            removed_files: result.orphans_cleaned,
            elapsed_ms: started.elapsed().as_millis(),
        };
        info!(workspace_id = %workspace_id, "{report}");
        Ok(Some((report, result)))
    }
}
