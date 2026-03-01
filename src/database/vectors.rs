//! Vector storage and KNN search using sqlite-vec.
//!
//! Provides CRUD operations for symbol embeddings stored in the `symbol_vectors`
//! virtual table (created by migration 010).
//!
//! # Vector Format
//!
//! Vectors are stored as 384-dimensional float arrays. The `zerocopy::AsBytes` trait
//! is used for zero-copy serialization of `Vec<f32>` → `&[u8]` when passing to sqlite-vec.

use anyhow::{Context, Result};
use tracing::debug;
use zerocopy::AsBytes;

use super::SymbolDatabase;

impl SymbolDatabase {
    /// Store embeddings for a batch of symbols.
    ///
    /// Deletes existing embeddings for the same symbol_ids first (vec0 virtual tables
    /// don't support INSERT OR REPLACE), then inserts fresh embeddings.
    /// Returns the number of embeddings stored.
    pub fn store_embeddings(&mut self, embeddings: &[(String, Vec<f32>)]) -> Result<usize> {
        if embeddings.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;
        let mut count = 0;

        {
            let mut del_stmt = tx.prepare("DELETE FROM symbol_vectors WHERE symbol_id = ?")?;
            let mut ins_stmt =
                tx.prepare("INSERT INTO symbol_vectors(symbol_id, embedding) VALUES (?, ?)")?;

            for (symbol_id, vector) in embeddings {
                del_stmt.execute([symbol_id])?;
                ins_stmt.execute(rusqlite::params![symbol_id, vector.as_bytes()])?;
                count += 1;
            }
        }

        tx.commit()?;
        debug!("Stored {count} embeddings");
        Ok(count)
    }

    /// Delete all embeddings for symbols belonging to a given file path.
    ///
    /// Joins on the `symbols` table to find symbol_ids for the file, then deletes
    /// matching rows from `symbol_vectors`.
    ///
    /// **Important:** Call this BEFORE deleting symbols from the `symbols` table,
    /// because the join requires symbol records to still exist.
    pub fn delete_embeddings_for_file(&mut self, file_path: &str) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM symbol_vectors WHERE symbol_id IN (
                SELECT id FROM symbols WHERE file_path = ?
            )",
            [file_path],
        )?;
        if deleted > 0 {
            debug!("Deleted {deleted} embeddings for file: {file_path}");
        }
        Ok(deleted)
    }

    /// Delete embedding rows for specific symbol IDs.
    ///
    /// Returns the number of deleted rows.
    pub fn delete_embeddings_for_symbol_ids(&mut self, symbol_ids: &[String]) -> Result<usize> {
        if symbol_ids.is_empty() {
            return Ok(0);
        }

        let placeholders: Vec<&str> = symbol_ids.iter().map(|_| "?").collect();
        let sql = format!(
            "DELETE FROM symbol_vectors WHERE symbol_id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = symbol_ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();
        let deleted = self.conn.execute(&sql, params.as_slice())?;

        if deleted > 0 {
            debug!("Deleted {deleted} embeddings for selected symbol IDs");
        }

        Ok(deleted)
    }

    /// Delete embedding rows that no longer have a matching symbol.
    pub fn delete_orphan_embeddings(&mut self) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM symbol_vectors
             WHERE symbol_id NOT IN (SELECT id FROM symbols)",
            [],
        )?;
        if deleted > 0 {
            debug!("Deleted {deleted} orphan embeddings");
        }
        Ok(deleted)
    }

    /// KNN (K-Nearest Neighbors) search: find symbols most similar to a query vector.
    ///
    /// Returns `(symbol_id, distance)` pairs ordered by ascending distance.
    /// Lower distance = more similar.
    pub fn knn_search(&self, query_vector: &[f32], limit: usize) -> Result<Vec<(String, f64)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT symbol_id, distance
                 FROM symbol_vectors
                 WHERE embedding MATCH ?
                 AND k = ?
                 ORDER BY distance",
            )
            .context("Failed to prepare KNN query")?;

        let results = stmt
            .query_map(
                rusqlite::params![query_vector.as_bytes(), limit as i64],
                |row| {
                    let symbol_id: String = row.get(0)?;
                    let distance: f64 = row.get(1)?;
                    Ok((symbol_id, distance))
                },
            )?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to execute KNN query")?;

        Ok(results)
    }

    /// Retrieve the stored embedding for a single symbol.
    ///
    /// Returns `Ok(None)` if the symbol has no embedding stored.
    /// The raw bytes from sqlite-vec are deserialized back to `Vec<f32>` using
    /// `f32::from_le_bytes` on 4-byte chunks.
    pub fn get_embedding(&self, symbol_id: &str) -> Result<Option<Vec<f32>>> {
        use rusqlite::OptionalExtension;

        let blob: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT embedding FROM symbol_vectors WHERE symbol_id = ?",
                [symbol_id],
                |row| row.get(0),
            )
            .optional()
            .context("Failed to query embedding for symbol")?;

        match blob {
            None => Ok(None),
            Some(bytes) => {
                if bytes.len() % 4 != 0 {
                    anyhow::bail!(
                        "Malformed embedding blob length for symbol {}: {} bytes (expected multiple of 4)",
                        symbol_id,
                        bytes.len()
                    );
                }

                let floats: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                Ok(Some(floats))
            }
        }
    }

    /// Get the set of symbol IDs that already have stored embeddings.
    ///
    /// Used by the incremental embedding pipeline to skip symbols that
    /// don't need re-embedding.
    pub fn get_embedded_symbol_ids(&self) -> Result<std::collections::HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT symbol_id FROM symbol_vectors")
            .context("Failed to prepare embedded symbol IDs query")?;

        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<std::collections::HashSet<String>, _>>()
            .context("Failed to collect embedded symbol IDs")?;

        Ok(ids)
    }

    /// Count the total number of stored embeddings.
    pub fn embedding_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbol_vectors", [], |row| row.get(0))
            .context("Failed to count embeddings")?;
        Ok(count)
    }

    /// Delete embeddings for symbols in the specified languages.
    ///
    /// Used to purge non-code embeddings (markdown, json, toml, etc.) that
    /// pollute semantic search by matching NL queries too closely.
    pub fn delete_embeddings_for_languages(&mut self, languages: &[&str]) -> Result<usize> {
        if languages.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<&str> = languages.iter().map(|_| "?").collect();
        let sql = format!(
            "DELETE FROM symbol_vectors WHERE symbol_id IN (
                SELECT id FROM symbols WHERE language IN ({})
            )",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = languages
            .iter()
            .map(|l| l as &dyn rusqlite::types::ToSql)
            .collect();
        let deleted = self.conn.execute(&sql, params.as_slice())?;
        if deleted > 0 {
            debug!("Purged {deleted} embeddings for non-code languages: {languages:?}");
        }
        Ok(deleted)
    }

    /// Delete all embeddings (used during re-indexing).
    pub fn clear_all_embeddings(&mut self) -> Result<()> {
        self.conn.execute("DELETE FROM symbol_vectors", [])?;
        debug!("Cleared all embeddings from symbol_vectors");
        Ok(())
    }
}
