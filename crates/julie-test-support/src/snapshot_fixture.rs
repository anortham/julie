//! A checkout store built from a fixture tree: facts in a temp file, Tantivy
//! in RAM, one snapshot published. Tool tests read it through `FakeToolContext`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use julie_core::embeddings_identity::EncoderIdentity;
use julie_core::file_policy::detect_language_for_indexing_with_content;
use julie_core::workspace::mutation_gate::Registry;
use julie_facts::rows::VectorRow;
use julie_index::checkout_store::{CheckoutStore, PathChange};
use julie_index::snapshot::Snapshot;
use julie_index::vectors::encoder_row;
use tempfile::TempDir;

pub struct SnapshotFixture {
    pub store: CheckoutStore,
    root: PathBuf,
    _facts_dir: TempDir,
}

fn collect_changes(root: &Path, dir: &Path, changes: &mut Vec<PathChange>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("read fixture dir {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_changes(root, &path, changes)?;
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let relative = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let language = detect_language_for_indexing_with_content(
            Path::new(&relative),
            &String::from_utf8_lossy(&bytes),
        );
        changes.push(PathChange::Upsert {
            path: relative,
            bytes,
            language,
        });
    }
    Ok(())
}

impl SnapshotFixture {
    /// Extract every file under `tree` into a fresh store and publish one snapshot.
    pub fn from_tree(tree: impl AsRef<Path>) -> Result<SnapshotFixture> {
        let root = tree
            .as_ref()
            .canonicalize()
            .with_context(|| format!("fixture tree {}", tree.as_ref().display()))?;
        let facts_dir = tempfile::tempdir()?;
        let store =
            CheckoutStore::open_with_ram_index(&facts_dir.path().join("facts.sqlite"), &root)?;
        let mut changes = Vec::new();
        collect_changes(&root, &root, &mut changes)?;
        let guard = Registry::new()
            .try_acquire("snapshot-fixture")
            .expect("fresh registry is uncontended");
        store.apply(&changes, &guard)?;
        Ok(Self {
            store,
            root,
            _facts_dir: facts_dir,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.store.current()
    }

    /// Record `identity` as the store's encoder, write one vector per named
    /// symbol (the first graph symbol with that name), and publish. Every
    /// snapshot taken afterwards serves the vectors.
    pub fn store_named_vectors(
        &self,
        identity: &EncoderIdentity,
        vectors: &[(&str, Vec<f32>)],
    ) -> Result<()> {
        let encoder = encoder_row(identity)?;
        let snapshot = self.snapshot();
        let graph = snapshot.graph();
        let rows = vectors
            .iter()
            .map(|(name, vector)| {
                let id = graph
                    .find_by_name(name)
                    .first()
                    .copied()
                    .with_context(|| format!("no symbol named {name} in the fixture"))?;
                let row = graph.symbol(id);
                Ok(VectorRow {
                    blob_hash: row.blob_hash.clone(),
                    symbol_ordinal: row.ordinal,
                    vector: vector.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.store.set_encoder(&encoder)?;
        self.store.store_vectors(&encoder.id, &rows)?;
        self.store.publish_vectors()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use julie_context::{ToolContext, WorkspaceTarget};

    use super::*;
    use crate::FakeToolContext;

    fn seed_tree() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/seed/a")
    }

    #[test]
    fn from_tree_builds_the_seed_snapshot_in_under_two_seconds() {
        let started = Instant::now();
        let fixture = SnapshotFixture::from_tree(seed_tree()).unwrap();
        assert!(started.elapsed().as_secs() < 2, "{:?}", started.elapsed());

        let snapshot = fixture.snapshot();
        assert_eq!(snapshot.graph().paths().len(), 8);
        assert!(snapshot.graph().find_by_name("alpha_app").len() == 1);
        assert!(snapshot.graph().find_by_name("shared_helper_marker").len() >= 1);
        assert_eq!(
            snapshot.searcher().num_docs() as usize,
            8 + snapshot.graph().len()
        );
        assert!(
            snapshot
                .file_text("app.py")
                .unwrap()
                .unwrap()
                .contains("alpha_app")
        );
        assert_eq!(fixture.store.status().blob_count, 8);
    }

    #[tokio::test]
    async fn fake_tool_context_serves_the_fixture_snapshot() {
        let fixture = SnapshotFixture::from_tree(seed_tree()).unwrap();
        let expected = Arc::as_ptr(&fixture.snapshot());
        let ctx = FakeToolContext::new().with_snapshot_fixture(fixture);

        let served = ctx.snapshot(&WorkspaceTarget::Primary).await.unwrap();
        assert_eq!(Arc::as_ptr(&served), expected);
        let by_id = ctx
            .snapshot(&WorkspaceTarget::Target("other".to_string()))
            .await
            .unwrap();
        assert_eq!(Arc::as_ptr(&by_id), expected);
        assert!(
            FakeToolContext::new()
                .snapshot(&WorkspaceTarget::Primary)
                .await
                .is_err()
        );
    }
}
