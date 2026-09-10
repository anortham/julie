//! The one writer for `facts.sqlite`. Blob rows are inserted once and never
//! updated or deleted; only `paths` changes after that, and `vectors` is
//! cleared when the encoder changes (see [`FactsWriter::set_encoder`]).

use anyhow::Result;
use julie_extractors::ExtractionResults;
use rusqlite::{OptionalExtension, params};

use crate::insert::insert_blob_rows;
use crate::rows::{EncoderRow, Normalization, VectorRow};
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

    /// Record the encoder whose vectors the store holds. A row for a different
    /// identity replaces the old one and deletes every `vectors` row: vectors
    /// from two encoders are not comparable, and the scan reads one matrix, so
    /// this is the single place facts rows are ever deleted after insert.
    pub fn set_encoder(&mut self, row: &EncoderRow) -> Result<()> {
        let tx = self.store.conn_mut().transaction()?;
        let current: Option<EncoderRow> = tx
            .query_row(
                "SELECT id, model_checksum, dimensions, pooling, normalization, instruction_policy FROM encoder",
                [],
                encoder_row,
            )
            .optional()?;
        if current.as_ref() == Some(row) {
            return Ok(());
        }
        tx.execute("DELETE FROM vectors", [])?;
        tx.execute("DELETE FROM encoder", [])?;
        tx.execute(
            "INSERT INTO encoder (id, model_checksum, dimensions, pooling, normalization, instruction_policy) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.id,
                row.model_checksum,
                row.dimensions,
                row.pooling,
                row.normalization,
                row.instruction_policy
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Insert `rows` for `encoder_id` in one transaction. A row already present
    /// for the same blob, ordinal, and encoder is replaced.
    pub fn store_vectors(&mut self, encoder_id: &str, rows: &[VectorRow]) -> Result<usize> {
        let tx = self.store.conn_mut().transaction()?;
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO vectors (blob_hash, symbol_ordinal, encoder_id, vector) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for row in rows {
                insert.execute(params![
                    row.blob_hash,
                    row.symbol_ordinal,
                    encoder_id,
                    row.to_bytes()
                ])?;
            }
        }
        tx.commit()?;
        Ok(rows.len())
    }
}

pub(crate) fn encoder_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EncoderRow> {
    Ok(EncoderRow {
        id: row.get(0)?,
        model_checksum: row.get(1)?,
        dimensions: row.get(2)?,
        pooling: row.get(3)?,
        normalization: row.get(4)?,
        instruction_policy: row.get(5)?,
    })
}
