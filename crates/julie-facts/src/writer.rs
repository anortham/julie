//! The one writer for `facts.sqlite`. Blob rows are inserted once and never
//! updated or deleted; only `paths` changes after that.

use anyhow::Result;
use julie_extractors::ExtractionResults;
use rusqlite::{OptionalExtension, params};

use crate::insert::insert_blob_rows;
use crate::rows::Normalization;
use crate::store::FactsStore;
use crate::version::SEMANTIC_INDEX_ENGINE_VERSION;

/// Runs extraction for one file. Injected so tests pass a fake.
pub trait Extractor {
    fn extract(&self, path: &str, content: &str, language: &str) -> ExtractionResults;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathChange {
    Upsert {
        path: String,
        bytes: Vec<u8>,
        language: String,
    },
    Remove {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Applied {
    pub new_blobs: usize,
    pub reused_blobs: usize,
    pub removed_paths: usize,
    /// `(path, blob_hash)` for every upserted path, in change order.
    pub paths_now: Vec<(String, String)>,
}

pub struct FactsWriter<'a> {
    store: &'a mut FactsStore,
    extractor: &'a dyn Extractor,
    normalization: Normalization,
}

impl<'a> FactsWriter<'a> {
    pub fn new(
        store: &'a mut FactsStore,
        extractor: &'a dyn Extractor,
        normalization: Normalization,
    ) -> Self {
        Self {
            store,
            extractor,
            normalization,
        }
    }

    /// Apply every change in one transaction. Upserts hash their bytes, extract
    /// only when the blob is new, and point the path at the blob.
    pub fn apply(&mut self, changes: &[PathChange]) -> Result<Applied> {
        let tx = self.store.conn_mut().transaction()?;
        let mut applied = Applied::default();
        for change in changes {
            match change {
                PathChange::Upsert {
                    path,
                    bytes,
                    language,
                } => {
                    let hash = blake3::hash(bytes).to_hex().to_string();
                    let known: Option<i64> = tx
                        .query_row("SELECT 1 FROM blobs WHERE hash = ?1", [&hash], |r| r.get(0))
                        .optional()?;
                    if known.is_some() {
                        applied.reused_blobs += 1;
                    } else {
                        let content = String::from_utf8_lossy(bytes);
                        let mut results = self.extractor.extract(path, &content, language);
                        self.normalization.apply(&mut results);
                        tx.execute(
                            "INSERT INTO blobs (hash, language, extractor_version, byte_len) VALUES (?1, ?2, ?3, ?4)",
                            params![hash, language, SEMANTIC_INDEX_ENGINE_VERSION, bytes.len() as i64],
                        )?;
                        insert_blob_rows(&tx, &hash, &results)?;
                        applied.new_blobs += 1;
                    }
                    let updated = tx.execute(
                        "UPDATE paths SET blob_hash = ?2, language = ?3 WHERE path = ?1",
                        params![path, hash, language],
                    )?;
                    if updated == 0 {
                        tx.execute(
                            "INSERT INTO paths (path, blob_hash, language) VALUES (?1, ?2, ?3)",
                            params![path, hash, language],
                        )?;
                    }
                    applied.paths_now.push((path.clone(), hash));
                }
                PathChange::Remove { path } => {
                    applied.removed_paths +=
                        tx.execute("DELETE FROM paths WHERE path = ?1", [path])?;
                }
            }
        }
        tx.commit()?;
        Ok(applied)
    }
}
