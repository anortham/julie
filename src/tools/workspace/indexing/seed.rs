//! Seed a new checkout from a sibling of the same repository.
//!
//! Copies blobs and fact rows for every hash in the new tree (`ATTACH` the
//! sibling read-only, `INSERT ... SELECT` by hash), extracts only missing
//! blobs, builds `paths`, and rebuilds Tantivy.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use julie_core::workspace::git_identity::git_common_dir;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_facts::{FactsStore, Opened};
use julie_index::checkout_store::{CheckoutStore, FACTS_FILE, PathChange};
use rusqlite::Connection;
use tracing::{info, warn};

use super::incremental::{ScannedFile, scan_indexable_files};
use super::index::IndexResult;
use super::store_open::store_dir;
use crate::handler::JulieServerHandler;
use crate::registry::workspace_registry_store::WorkspaceRegistryStore;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;

const FACT_TABLES: &[&str] = &[
    "symbols",
    "identifiers",
    "relationships",
    "types",
    "source_regions",
    "structural_facts",
    "complexity_metrics",
    "literals",
    "type_arguments",
    "diagnostics",
    "test_verdicts",
    "vectors",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SeedReport {
    pub sibling_root: PathBuf,
    pub copied: usize,
    pub extracted: usize,
    pub removed: usize,
    pub elapsed_ms: u128,
}

impl fmt::Display for SeedReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Seeded from {}: {} blobs copied, {} extracted, {} removed in {} ms",
            self.sibling_root.display(),
            self.copied,
            self.extracted,
            self.removed,
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
#[cfg(test)]
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

fn copy_sibling_blobs(
    target: &Connection,
    sibling_facts: &Path,
    hashes: &[String],
) -> Result<usize> {
    if hashes.is_empty() {
        return Ok(0);
    }
    let uri = format!("file:{}?mode=ro", sibling_facts.display());
    target.execute("ATTACH DATABASE ?1 AS sibling", [uri])?;
    target.execute_batch("CREATE TEMP TABLE wanted (hash TEXT PRIMARY KEY)")?;
    {
        let mut insert = target.prepare("INSERT OR IGNORE INTO wanted (hash) VALUES (?1)")?;
        for hash in hashes {
            insert.execute([hash])?;
        }
    }
    let copied = target.execute(
        "INSERT OR IGNORE INTO blobs SELECT b.* FROM sibling.blobs b JOIN wanted w ON w.hash = b.hash",
        [],
    )?;
    for table in FACT_TABLES {
        let sql = format!(
            "INSERT OR IGNORE INTO {table} SELECT t.* FROM sibling.{table} t JOIN wanted w ON w.hash = t.blob_hash"
        );
        target.execute(&sql, [])?;
    }
    let _ = target.execute(
        "INSERT OR IGNORE INTO encoder SELECT * FROM sibling.encoder",
        [],
    );
    target.execute_batch("DROP TABLE wanted; DETACH DATABASE sibling")?;
    Ok(copied)
}

fn create_facts(path: &Path) -> Result<FactsStore> {
    match FactsStore::open(path)? {
        Opened::Ready(store) => Ok(store),
        Opened::VersionMismatch { .. } => {
            if let Some(parent) = path.parent() {
                if parent.exists() {
                    std::fs::remove_dir_all(parent)?;
                    std::fs::create_dir_all(parent)?;
                }
            }
            match FactsStore::open(path)? {
                Opened::Ready(store) => Ok(store),
                Opened::VersionMismatch {
                    found_schema,
                    found_engine,
                } => anyhow::bail!(
                    "facts.sqlite still mismatched after recreate: schema {found_schema}, engine {found_engine}"
                ),
            }
        }
    }
}

fn upserts_for(scanned: &[ScannedFile]) -> Vec<PathChange> {
    scanned
        .iter()
        .map(|file| PathChange::Upsert {
            path: file.path.clone(),
            bytes: file.bytes.clone(),
            language: file.language.clone(),
        })
        .collect()
}

impl ManageWorkspaceTool {
    /// Seed `root` from a registered sibling when `root` has no facts store yet.
    pub(crate) async fn seed_if_index_missing(
        &self,
        handler: &JulieServerHandler,
        root: &Path,
        guard: &MutationGuard<'_>,
    ) -> Result<Option<(SeedReport, IndexResult)>> {
        let workspace_id = generate_workspace_id(&root.to_string_lossy())?;
        let index_root = handler.workspace_index_dir_for(&workspace_id).await?;
        if store_dir(&index_root).join(FACTS_FILE).exists() {
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

    /// Copy sibling blobs whose hashes are in `root`, extract the rest, build
    /// paths, and rebuild Tantivy.
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
        let sibling_facts =
            store_dir(&handler.workspace_index_dir_for(&sibling_id).await?).join(FACTS_FILE);
        if !sibling_facts.exists() {
            return Ok(None);
        }

        let write_julieignore = !handler
            .suppress_workspace_file_writes
            .load(std::sync::atomic::Ordering::Relaxed);
        let root_buf = root.to_path_buf();
        let tool = self.clone();
        let discovered = tokio::task::spawn_blocking(move || {
            if write_julieignore {
                tool.discover_indexable_files(&root_buf)
            } else {
                tool.discover_indexable_files_with_options(&root_buf, false)
            }
        })
        .await
        .context("seed file discovery task panicked")??;
        let scanned = scan_indexable_files(root, &discovered)?;
        let hashes: Vec<String> = scanned.iter().map(|file| file.hash.clone()).collect();

        let store_dir = store_dir(index_root);
        std::fs::create_dir_all(&store_dir)?;
        let facts_path = store_dir.join(FACTS_FILE);
        let copied = {
            let facts_path = facts_path.clone();
            let sibling_facts = sibling_facts.clone();
            tokio::task::spawn_blocking(move || -> Result<usize> {
                let facts = create_facts(&facts_path)?;
                copy_sibling_blobs(facts.conn(), &sibling_facts, &hashes)
            })
            .await
            .context("sibling blob copy task panicked")?
        };
        let copied = match copied {
            Ok(copied) => copied,
            Err(err) => {
                warn!(error = %err, "Sibling seed copy failed; indexing from scratch");
                let _ = std::fs::remove_dir_all(&store_dir);
                return Ok(None);
            }
        };

        let root_buf = root.to_path_buf();
        let store_dir_buf = store_dir.clone();
        let store =
            tokio::task::spawn_blocking(move || CheckoutStore::open(&store_dir_buf, &root_buf))
                .await
                .context("seed store open task panicked")??;
        let changes = upserts_for(&scanned);
        let applied = store.apply(&changes, guard)?;
        store.rebuild_tantivy_if_needed(guard)?;

        let stats = store.status();
        let snapshot = store.current();
        let workspace_id = generate_workspace_id(&root.to_string_lossy())?;
        let report = SeedReport {
            sibling_root: sibling.to_path_buf(),
            copied,
            extracted: applied.new_blobs,
            removed: applied.removed_paths,
            elapsed_ms: started.elapsed().as_millis(),
        };
        info!(workspace_id = %workspace_id, "{report}");
        Ok(Some((
            report,
            IndexResult {
                files_processed: applied.new_blobs,
                orphans_cleaned: applied.removed_paths,
                facts_revision: None,
                files_total: snapshot.graph().paths().len(),
                symbols_total: stats.graph.symbols,
                relationships_total: stats.graph.edges,
                duration_ms: started.elapsed().as_millis() as u64,
            },
        )))
    }
}
