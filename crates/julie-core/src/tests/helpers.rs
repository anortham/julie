//! Local test helpers for julie-core tests.
//!
//! These replicate the builder API from `julie-test-support` for the types that
//! live in THIS crate (`FileInfo`, `SymbolDatabase`).  We cannot use the
//! `julie-test-support` builders for such types because the dev-dep cycle causes
//! Cargo to compile two separate rlibs for `julie-core` (one for the test binary,
//! one as the dep of `julie-test-support`), and the type-system then rejects
//! values crossing the rlib boundary even though they are logically the same.
//!
//! For types from external crates (`Symbol`, `Relationship`, `Identifier` from
//! `julie_extractors`, `Connection` from `rusqlite`) we reuse `julie-test-support`
//! directly — those types unify fine.

use anyhow::Result;
use crate::database::{SymbolDatabase, types::FileInfo};

/// Build a `FileInfo` with stable test defaults.  Override individual fields via
/// the returned builder.
pub fn file_info_builder(path: impl Into<String>) -> FileInfoBuilder {
    FileInfoBuilder::new(path.into())
}

/// Check-and-insert: stores `file_info` only when the path is not already in the
/// database.  Returns `true` when inserted, `false` when skipped.
pub fn store_file_info_if_missing(db: &SymbolDatabase, file_info: &FileInfo) -> Result<bool> {
    if db.get_file_hash(&file_info.path)?.is_some() {
        return Ok(false);
    }
    db.store_file_info(file_info)?;
    Ok(true)
}

/// Bulk-set `reference_score` for the given `(id, score)` pairs.
pub fn set_symbol_reference_scores(db: &SymbolDatabase, scores: &[(&str, f64)]) -> Result<()> {
    for (id, score) in scores {
        let updated = db.conn.execute(
            "UPDATE symbols SET reference_score = ?1 WHERE id = ?2",
            rusqlite::params![score, id],
        )?;
        if updated == 0 {
            anyhow::bail!("failed to set reference_score for missing symbol id `{id}`");
        }
    }
    Ok(())
}

pub struct FileInfoBuilder {
    path: String,
    language: String,
    hash: String,
    size: i64,
    last_modified: i64,
    last_indexed: i64,
    symbol_count: i32,
    line_count: i32,
    content: Option<String>,
}

impl FileInfoBuilder {
    pub fn new(path: String) -> Self {
        Self {
            hash: format!("hash-{path}"),
            path,
            language: "rust".to_string(),
            size: 0,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: None,
        }
    }

    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }
    pub fn hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = hash.into();
        self
    }
    pub fn size(mut self, size: i64) -> Self {
        self.size = size;
        self
    }
    pub fn last_modified(mut self, last_modified: i64) -> Self {
        self.last_modified = last_modified;
        self
    }
    pub fn last_indexed(mut self, last_indexed: i64) -> Self {
        self.last_indexed = last_indexed;
        self
    }
    pub fn symbol_count(mut self, symbol_count: i32) -> Self {
        self.symbol_count = symbol_count;
        self
    }
    pub fn line_count(mut self, line_count: i32) -> Self {
        self.line_count = line_count;
        self
    }
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn build(self) -> FileInfo {
        FileInfo {
            path: self.path,
            language: self.language,
            hash: self.hash,
            size: self.size,
            last_modified: self.last_modified,
            last_indexed: self.last_indexed,
            symbol_count: self.symbol_count,
            line_count: self.line_count,
            content: self.content,
        }
    }
}
