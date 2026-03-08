//! Vector storage and KNN search for memory embeddings using sqlite-vec.
//!
//! Provides CRUD operations for checkpoint embeddings stored in the `memory_vectors`
//! virtual table (created by migration 012). Mirrors `vectors.rs` but keyed by
//! `checkpoint_id` instead of `symbol_id`.

use anyhow::{Context, Result};
use tracing::debug;
use zerocopy::AsBytes;

use super::SymbolDatabase;

impl SymbolDatabase {
    /// Store embeddings for a batch of memory checkpoints.
    ///
    /// Deletes existing embeddings for the same checkpoint_ids first (vec0 virtual
    /// tables don't support INSERT OR REPLACE), then inserts fresh embeddings.
    /// Returns the number of embeddings stored.
    pub fn store_memory_embeddings(
        &mut self,
        embeddings: &[(String, Vec<f32>)],
    ) -> Result<usize> {
        if embeddings.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;
        let mut count = 0;

        {
            let mut del_stmt =
                tx.prepare("DELETE FROM memory_vectors WHERE checkpoint_id = ?")?;
            let mut ins_stmt = tx.prepare(
                "INSERT INTO memory_vectors(checkpoint_id, embedding) VALUES (?, ?)",
            )?;

            for (checkpoint_id, vector) in embeddings {
                del_stmt.execute([checkpoint_id])?;
                ins_stmt.execute(rusqlite::params![checkpoint_id, vector.as_bytes()])?;
                count += 1;
            }
        }

        tx.commit()?;
        debug!("Stored {count} memory embeddings");
        Ok(count)
    }

    /// Delete the embedding for a single checkpoint.
    /// Returns the number of rows deleted (0 or 1).
    pub fn delete_memory_embedding(&mut self, checkpoint_id: &str) -> Result<usize> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM memory_vectors WHERE checkpoint_id = ?",
                [checkpoint_id],
            )
            .context("Failed to delete memory embedding")?;
        Ok(deleted)
    }

    /// Delete all memory embeddings.
    pub fn clear_all_memory_embeddings(&mut self) -> Result<()> {
        self.conn
            .execute("DELETE FROM memory_vectors", [])
            .context("Failed to clear memory embeddings")?;
        debug!("Cleared all memory embeddings");
        Ok(())
    }

    /// Count the total number of stored memory embeddings.
    pub fn memory_embedding_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM memory_vectors", [], |row| row.get(0))
            .context("Failed to count memory embeddings")?;
        Ok(count)
    }

    /// KNN search over memory embeddings: find checkpoints most similar to a query vector.
    ///
    /// Returns `(checkpoint_id, distance)` pairs ordered by ascending distance.
    /// Lower distance = more similar.
    pub fn knn_memory_search(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f64)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT checkpoint_id, distance
                 FROM memory_vectors
                 WHERE embedding MATCH ?
                 AND k = ?
                 ORDER BY distance",
            )
            .context("Failed to prepare memory KNN query")?;

        let results = stmt
            .query_map(
                rusqlite::params![query_vector.as_bytes(), limit as i64],
                |row| {
                    let checkpoint_id: String = row.get(0)?;
                    let distance: f64 = row.get(1)?;
                    Ok((checkpoint_id, distance))
                },
            )?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to execute memory KNN query")?;

        Ok(results)
    }

    /// Drop and recreate the `memory_vectors` table with new dimensions.
    ///
    /// This destroys all existing memory embeddings — used when the embedding
    /// model changes and vectors need re-computation.
    pub fn recreate_memory_vectors_table(&mut self, dimensions: usize) -> Result<()> {
        let tx = self.conn.transaction()?;

        tx.execute("DROP TABLE IF EXISTS memory_vectors", [])?;
        tx.execute(
            &format!(
                "CREATE VIRTUAL TABLE memory_vectors USING vec0(
                    checkpoint_id TEXT PRIMARY KEY,
                    embedding float[{dimensions}]
                )"
            ),
            [],
        )?;

        tx.commit()?;
        debug!("Recreated memory_vectors table with {dimensions}-dim vectors");
        Ok(())
    }
}
