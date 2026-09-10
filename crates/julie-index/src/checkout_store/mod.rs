//! The one writer for a checkout's index directory: `facts.sqlite`, the
//! in-memory graph, and the Tantivy projection. Every write goes through
//! [`CheckoutStore::apply`] under the workspace mutation gate and ends by
//! publishing a new [`Snapshot`]; readers keep the one they hold.

mod extractor;
mod index_dir;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use anyhow::{Context, Result};
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_facts::rows::{EncoderRow, Normalization, VectorRow};
use julie_facts::{FactsStore, FactsWriter, Opened};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};
use tracing::info;

pub use julie_facts::{Applied, PathChange};

use crate::graph::{Graph, GraphStats};
use crate::search::projection::from_facts;
use crate::search::schema::SchemaFields;
use crate::snapshot::{Snapshot, read_verified};
use crate::vectors::VectorSet;
use extractor::FactsExtractor;

/// Index-directory entry that holds the store while the old `db/` and
/// `tantivy/` are still written beside it; Task 13 moves it up one level.
pub const STORE_DIR: &str = "store";
pub const FACTS_FILE: &str = "facts.sqlite";
pub const TANTIVY_DIR: &str = "tantivy";
const WRITER_HEAP_BYTES: usize = 64_000_000;

/// `facts.sqlite` was written by another schema or engine version. The store
/// never migrates; the caller deletes the index directory and reindexes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("facts.sqlite version mismatch: schema {found_schema}, engine {found_engine}")]
pub struct VersionMismatch {
    pub found_schema: i32,
    pub found_engine: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TantivyState {
    Present,
    Building,
    Stale,
    Absent,
}

#[derive(Debug, Clone)]
pub struct StoreStatus {
    pub blob_count: u64,
    pub facts_bytes: u64,
    pub tantivy: TantivyState,
    pub tantivy_age_secs: Option<u64>,
    pub graph: GraphStats,
    pub last_write_at: Option<SystemTime>,
    /// Vectors bound to symbols of the current snapshot.
    pub vector_count: u64,
}

pub struct CheckoutStore {
    root: PathBuf,
    facts_path: PathBuf,
    facts: Mutex<FactsStore>,
    extractor: FactsExtractor,
    normalization: Normalization,
    index: Index,
    reader: IndexReader,
    writes: Mutex<()>,
    fields: SchemaFields,
    tantivy_dir: Option<PathBuf>,
    tantivy_state: Mutex<TantivyState>,
    vectors: Mutex<Arc<VectorSet>>,
    last_write_at: Mutex<Option<SystemTime>>,
    current: RwLock<Arc<Snapshot>>,
}

fn open_facts(path: &Path) -> Result<FactsStore> {
    match FactsStore::open(path)? {
        Opened::Ready(store) => Ok(store),
        Opened::VersionMismatch {
            found_schema,
            found_engine,
        } => Err(VersionMismatch {
            found_schema,
            found_engine,
        }
        .into()),
    }
}

impl CheckoutStore {
    /// Open `<index_dir>/facts.sqlite` and `<index_dir>/tantivy/`, creating
    /// both when missing. A facts version mismatch is returned as
    /// [`VersionMismatch`] without touching the file.
    pub fn open(index_dir: &Path, root: &Path) -> Result<CheckoutStore> {
        std::fs::create_dir_all(index_dir)
            .with_context(|| format!("create index dir {}", index_dir.display()))?;
        let facts_path = index_dir.join(FACTS_FILE);
        let facts = open_facts(&facts_path)?;
        let tantivy_dir = index_dir.join(TANTIVY_DIR);
        let (index, state) = index_dir::open_or_create(&tantivy_dir)?;
        Self::build(facts, facts_path, index, Some(tantivy_dir), state, root)
    }

    /// Facts on disk at `facts_path`, Tantivy in RAM. Used by test fixtures.
    pub fn open_with_ram_index(facts_path: &Path, root: &Path) -> Result<CheckoutStore> {
        let facts = open_facts(facts_path)?;
        Self::build(
            facts,
            facts_path.to_path_buf(),
            index_dir::in_ram(),
            None,
            TantivyState::Present,
            root,
        )
    }

    fn build(
        facts: FactsStore,
        facts_path: PathBuf,
        index: Index,
        tantivy_dir: Option<PathBuf>,
        state: TantivyState,
        root: &Path,
    ) -> Result<CheckoutStore> {
        let root = root.to_path_buf();
        let graph = Arc::new(Graph::load(&facts.reader(), None)?);
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let fields = SchemaFields::new(&index.schema());
        let vectors = Arc::new(load_vectors(&facts, &graph)?);
        let snapshot = Snapshot::new(
            Arc::clone(&graph),
            reader_searcher(&reader),
            fields.clone(),
            Arc::clone(&vectors),
            facts_path.clone(),
            root.clone(),
        );
        Ok(Self {
            extractor: FactsExtractor::new(root.clone()),
            normalization: extractor::normalization(),
            root,
            facts_path,
            facts: Mutex::new(facts),
            index,
            reader,
            writes: Mutex::new(()),
            fields,
            tantivy_dir,
            tantivy_state: Mutex::new(state),
            vectors: Mutex::new(vectors),
            last_write_at: Mutex::new(None),
            current: RwLock::new(Arc::new(snapshot)),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn current(&self) -> Arc<Snapshot> {
        Arc::clone(&self.current.read().unwrap_or_else(|p| p.into_inner()))
    }

    /// Write facts, derive the next graph, re-project the changed paths,
    /// commit, and publish. Readers holding the previous snapshot are unaffected.
    pub fn apply(&self, changes: &[PathChange], guard: &MutationGuard<'_>) -> Result<Applied> {
        self.rebuild_tantivy_if_needed(guard)?;
        let mut latest: HashMap<&str, Option<(&[u8], &str)>> = HashMap::new();
        for change in changes {
            match change {
                PathChange::Upsert {
                    path,
                    bytes,
                    language,
                } => {
                    latest.insert(path, Some((bytes, language)));
                }
                PathChange::Remove { path } => {
                    latest.insert(path, None);
                }
            }
        }
        let changed: Vec<String> = latest
            .iter()
            .filter(|(_, v)| v.is_some())
            .map(|(p, _)| p.to_string())
            .collect();
        let removed: Vec<String> = latest
            .iter()
            .filter(|(_, v)| v.is_none())
            .map(|(p, _)| p.to_string())
            .collect();

        let (applied, graph) = {
            let mut facts = self.facts.lock().unwrap_or_else(|p| p.into_inner());
            let applied = FactsWriter::new(&mut facts, &self.extractor, self.normalization.clone())
                .apply(changes)?;
            let graph = self
                .current()
                .graph()
                .apply_paths(&facts.reader(), &changed, &removed)?;
            (applied, Arc::new(graph))
        };

        self.write_documents(|writer| {
            for path in latest.keys() {
                writer.delete_term(Term::from_field_text(self.fields.file_path, path));
            }
            for (path, upsert) in &latest {
                if let Some((bytes, language)) = upsert {
                    self.add_path_documents(writer, &graph, path, language, bytes)?;
                }
            }
            Ok(())
        })?;
        self.publish(graph)?;
        *self.last_write_at.lock().unwrap_or_else(|p| p.into_inner()) = Some(SystemTime::now());
        Ok(applied)
    }

    /// Re-project every path from facts when the Tantivy directory was absent,
    /// stale, or empty while facts has paths. Returns whether a rebuild ran.
    pub fn rebuild_tantivy_if_needed(&self, _guard: &MutationGuard<'_>) -> Result<bool> {
        let graph = Arc::clone(self.current().graph());
        if !self.needs_rebuild(&graph) {
            return Ok(false);
        }
        info!(root = %self.root.display(), paths = graph.paths().len(), "rebuilding tantivy from facts");
        self.set_tantivy_state(TantivyState::Building);
        let paths = self
            .facts
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .reader()
            .paths()?;
        self.write_documents(|writer| {
            writer.delete_all_documents()?;
            for row in &paths {
                if let Some(bytes) = read_verified(&self.root, &row.path, &row.blob_hash) {
                    self.add_path_documents(writer, &graph, &row.path, &row.language, &bytes)?;
                }
            }
            Ok(())
        })?;
        self.publish(graph)?;
        self.set_tantivy_state(TantivyState::Present);
        Ok(true)
    }

    /// Record the encoder whose vectors this store holds. A different identity
    /// deletes every stored vector and publishes an empty set.
    pub fn set_encoder(&self, encoder: &EncoderRow) -> Result<()> {
        {
            let mut facts = self.facts.lock().unwrap_or_else(|p| p.into_inner());
            FactsWriter::new(&mut facts, &self.extractor, self.normalization.clone())
                .set_encoder(encoder)?;
        }
        self.publish_vectors()
    }

    /// Append vector rows for `encoder_id`. Rows reach readers on the next
    /// [`CheckoutStore::publish_vectors`], so a batch loop pays one reload.
    pub fn store_vectors(&self, encoder_id: &str, rows: &[VectorRow]) -> Result<usize> {
        let mut facts = self.facts.lock().unwrap_or_else(|p| p.into_inner());
        FactsWriter::new(&mut facts, &self.extractor, self.normalization.clone())
            .store_vectors(encoder_id, rows)
    }

    /// Reload every stored vector from facts, bind it to the current graph,
    /// and publish. O(vectors) SQLite reads; call once per embedding run.
    pub fn publish_vectors(&self) -> Result<()> {
        let graph = Arc::clone(self.current().graph());
        let vectors = {
            let facts = self.facts.lock().unwrap_or_else(|p| p.into_inner());
            load_vectors(&facts, &graph)?
        };
        *self.vectors.lock().unwrap_or_else(|p| p.into_inner()) = Arc::new(vectors);
        self.publish(graph)
    }

    pub fn status(&self) -> StoreStatus {
        let (blob_count, facts_bytes) = {
            let facts = self.facts.lock().unwrap_or_else(|p| p.into_inner());
            let reader = facts.reader();
            (
                reader.blob_count().unwrap_or(0),
                reader.file_size_bytes().unwrap_or(0),
            )
        };
        let current = self.current();
        let tantivy_age_secs = self
            .tantivy_dir
            .as_ref()
            .and_then(|dir| std::fs::metadata(dir.join("meta.json")).ok())
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .map(|age| age.as_secs());
        StoreStatus {
            blob_count,
            facts_bytes,
            tantivy: *self.tantivy_state.lock().unwrap_or_else(|p| p.into_inner()),
            tantivy_age_secs,
            graph: current.graph().stats(),
            last_write_at: *self.last_write_at.lock().unwrap_or_else(|p| p.into_inner()),
            vector_count: current.vectors().len() as u64,
        }
    }

    fn needs_rebuild(&self, graph: &Graph) -> bool {
        let state = *self.tantivy_state.lock().unwrap_or_else(|p| p.into_inner());
        state != TantivyState::Present
            || (!graph.paths().is_empty() && self.reader.searcher().num_docs() == 0)
    }

    fn set_tantivy_state(&self, state: TantivyState) {
        *self.tantivy_state.lock().unwrap_or_else(|p| p.into_inner()) = state;
    }

    /// Run `write` on a fresh writer, commit, and release the writer before
    /// the next batch; then reload the reader.
    fn write_documents(&self, write: impl FnOnce(&mut IndexWriter) -> Result<()>) -> Result<()> {
        let slot = self.writes.lock().unwrap_or_else(|p| p.into_inner());
        let mut writer = self.index.writer(WRITER_HEAP_BYTES)?;
        let result =
            write(&mut writer).and_then(|()| writer.commit().map(|_| ()).map_err(Into::into));
        if result.is_err() {
            writer.rollback()?;
        }
        drop(writer);
        drop(slot);
        result?;
        self.reader.reload()?;
        Ok(())
    }

    fn add_path_documents(
        &self,
        writer: &mut IndexWriter,
        graph: &Graph,
        path: &str,
        language: &str,
        bytes: &[u8],
    ) -> Result<()> {
        let text = String::from_utf8_lossy(bytes);
        let empty = crate::graph::FileRows {
            path: path.to_string(),
            symbols: Arc::from([]),
            identifiers: Arc::from([]),
            relationships: Arc::from([]),
        };
        let rows = graph.file_rows(path).unwrap_or(&empty);
        for doc in from_facts::documents(rows, language, &text) {
            writer.add_document(from_facts::tantivy_document(&self.fields, &doc))?;
        }
        Ok(())
    }

    fn publish(&self, graph: Arc<Graph>) -> Result<()> {
        let vectors = {
            let mut held = self.vectors.lock().unwrap_or_else(|p| p.into_inner());
            let bound = Arc::new(held.bind(|key| graph.symbol_by_row_id(key)));
            *held = Arc::clone(&bound);
            bound
        };
        let snapshot = Snapshot::new(
            graph,
            reader_searcher(&self.reader),
            self.fields.clone(),
            vectors,
            self.facts_path.clone(),
            self.root.clone(),
        );
        *self.current.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(snapshot);
        Ok(())
    }
}

fn load_vectors(facts: &FactsStore, graph: &Graph) -> Result<VectorSet> {
    let reader = facts.reader();
    let Some(encoder) = reader.encoder()? else {
        return Ok(VectorSet::empty());
    };
    let rows = reader.vectors_for_encoder(&encoder.id)?;
    Ok(VectorSet::from_rows(Some(encoder), &rows, |key| {
        graph.symbol_by_row_id(key)
    }))
}

fn reader_searcher(reader: &IndexReader) -> tantivy::Searcher {
    reader.searcher()
}
