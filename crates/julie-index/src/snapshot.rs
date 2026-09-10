//! One immutable view of a checkout: the resolved graph, a pinned Tantivy
//! searcher, the vector set, and read-only access to facts. Tools hold an
//! `Arc<Snapshot>` for the whole call; a later write publishes a new one.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use julie_facts::FactsStore;
use rusqlite::OptionalExtension;
use tantivy::Searcher;
use tracing::warn;

use crate::graph::Graph;
use crate::search::schema::SchemaFields;
use crate::vectors::VectorSet;

pub struct Snapshot {
    graph: Arc<Graph>,
    searcher: Searcher,
    fields: SchemaFields,
    vectors: Arc<VectorSet>,
    facts_path: PathBuf,
    root: PathBuf,
    published_at: SystemTime,
}

impl Snapshot {
    pub(crate) fn new(
        graph: Arc<Graph>,
        searcher: Searcher,
        fields: SchemaFields,
        vectors: Arc<VectorSet>,
        facts_path: PathBuf,
        root: PathBuf,
    ) -> Self {
        Self {
            graph,
            searcher,
            fields,
            vectors,
            facts_path,
            root,
            published_at: SystemTime::now(),
        }
    }

    pub fn graph(&self) -> &Arc<Graph> {
        &self.graph
    }

    pub fn searcher(&self) -> &Searcher {
        &self.searcher
    }

    /// Field handles for the searcher's schema.
    pub fn fields(&self) -> &SchemaFields {
        &self.fields
    }

    /// A read-only facts connection. `rusqlite::Connection` is not `Sync`, so
    /// each call opens its own instead of sharing one behind a mutex; call
    /// `.reader()` on the result.
    pub fn facts(&self) -> Result<FactsStore> {
        FactsStore::open_read_only(&self.facts_path)
    }

    pub fn vectors(&self) -> &Arc<VectorSet> {
        &self.vectors
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn published_at(&self) -> SystemTime {
        self.published_at
    }

    /// Bytes of `path` from the checkout as text, only when they still hash to
    /// the blob the facts were extracted from. `None` means the path is unknown,
    /// unreadable, or edited since the last apply (the watcher will re-apply).
    pub fn file_text(&self, path: &str) -> Result<Option<String>> {
        let blob_hash: Option<String> = self
            .facts()?
            .conn()
            .query_row(
                "SELECT blob_hash FROM paths WHERE path = ?1",
                [path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(blob_hash
            .and_then(|hash| read_verified(&self.root, path, &hash))
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
    }
}

/// Read `<root>/<path>` and return the bytes only when they hash to `blob_hash`.
pub(crate) fn read_verified(root: &Path, path: &str, blob_hash: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(root.join(path)).ok()?;
    if blake3::hash(&bytes).to_hex().as_str() == blob_hash {
        Some(bytes)
    } else {
        warn!(
            path,
            "checkout bytes differ from the facts blob; skipping until the watcher re-applies"
        );
        None
    }
}
