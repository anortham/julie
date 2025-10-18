// Embedding storage and retrieval operations

use super::*;
use anyhow::Result;
use rusqlite::params;
use std::collections::HashMap;
use tracing::{debug, info};

impl SymbolDatabase {
    pub fn store_embedding_vector(
        &self,
        vector_id: &str,
        vector_data: &[f32],
        dimensions: usize,
        model_name: &str,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Serialize f32 vector to bytes using native endianness
        let bytes: Vec<u8> = vector_data.iter().flat_map(|f| f.to_le_bytes()).collect();

        self.conn.execute(
            "INSERT OR REPLACE INTO embedding_vectors
             (vector_id, dimensions, vector_data, model_name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![vector_id, dimensions as i64, bytes, model_name, now],
        )?;

        debug!(
            "Stored embedding vector: {} ({}D, {} bytes)",
            vector_id,
            dimensions,
            bytes.len()
        );
        Ok(())
    }

    /// Retrieve embedding vector data from BLOB
    pub fn get_embedding_vector(&self, vector_id: &str) -> Result<Option<Vec<f32>>> {
        let result = self.conn.query_row(
            "SELECT vector_data, dimensions FROM embedding_vectors WHERE vector_id = ?1",
            params![vector_id],
            |row| {
                let bytes: Vec<u8> = row.get(0)?;
                let dimensions: i64 = row.get(1)?;
                Ok((bytes, dimensions))
            },
        );

        match result {
            Ok((bytes, dimensions)) => {
                // Deserialize bytes back to f32 vector
                if bytes.len() != (dimensions as usize * 4) {
                    return Err(anyhow!(
                        "Invalid vector data size: expected {} bytes, got {}",
                        dimensions * 4,
                        bytes.len()
                    ));
                }

                let vector: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                Ok(Some(vector))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Failed to retrieve embedding vector: {}", e)),
        }
    }

    /// Store embedding metadata linking symbol to vector
    pub fn store_embedding_metadata(
        &self,
        symbol_id: &str,
        vector_id: &str,
        model_name: &str,
        embedding_hash: Option<&str>,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO embeddings
             (symbol_id, vector_id, model_name, embedding_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![symbol_id, vector_id, model_name, embedding_hash, now],
        )?;

        debug!(
            "Stored embedding metadata: symbol={}, vector={}, model={}",
            symbol_id, vector_id, model_name
        );
        Ok(())
    }

    /// 🚀 BLAZING-FAST bulk embedding storage for batch processing
    /// Inserts both vectors and metadata in a single transaction
    pub fn bulk_store_embeddings(
        &mut self,
        embeddings: &[(String, Vec<f32>)], // (symbol_id, vector)
        dimensions: usize,
        model_name: &str,
    ) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Use transaction for atomic bulk insert
        let tx = self.conn.transaction()?;

        // Prepare statements for batch insert
        let mut vector_stmt = tx.prepare(
            "INSERT OR REPLACE INTO embedding_vectors
             (vector_id, dimensions, vector_data, model_name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        let mut metadata_stmt = tx.prepare(
            "INSERT OR REPLACE INTO embeddings
             (symbol_id, vector_id, model_name, embedding_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for (symbol_id, vector_data) in embeddings {
            // Serialize vector to bytes
            let bytes: Vec<u8> = vector_data.iter().flat_map(|f| f.to_le_bytes()).collect();

            // Insert vector data (using symbol_id as vector_id for simplicity)
            vector_stmt.execute(params![
                symbol_id,
                dimensions as i64,
                bytes,
                model_name,
                now
            ])?;

            // Insert metadata linking symbol to vector
            metadata_stmt.execute(params![
                symbol_id,
                symbol_id, // vector_id = symbol_id
                model_name,
                None::<String>, // embedding_hash
                now
            ])?;
        }

        // Drop statements before committing
        drop(vector_stmt);
        drop(metadata_stmt);
        tx.commit()?;

        let duration = start_time.elapsed();
        debug!(
            "✅ Bulk embedding storage complete! {} embeddings in {:.2}ms ({:.0} embeddings/sec)",
            embeddings.len(),
            duration.as_millis(),
            embeddings.len() as f64 / duration.as_secs_f64()
        );

        Ok(())
    }

    /// Get embedding vector for a specific symbol
    pub fn get_embedding_for_symbol(
        &self,
        symbol_id: &str,
        model_name: &str,
    ) -> Result<Option<Vec<f32>>> {
        // First get the vector_id from embeddings metadata table
        let vector_id_result = self.conn.query_row(
            "SELECT vector_id FROM embeddings WHERE symbol_id = ?1 AND model_name = ?2",
            params![symbol_id, model_name],
            |row| row.get::<_, String>(0),
        );

        match vector_id_result {
            Ok(vector_id) => {
                // Then fetch the actual vector data
                self.get_embedding_vector(&vector_id)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Failed to get embedding metadata: {}", e)),
        }
    }

    /// Delete embedding vector and metadata
    pub fn delete_embedding(&self, vector_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM embedding_vectors WHERE vector_id = ?1",
            params![vector_id],
        )?;

        // Metadata will cascade delete automatically due to FK constraint
        debug!("Deleted embedding vector: {}", vector_id);
        Ok(())
    }

    /// Delete embeddings for a specific symbol
    pub fn delete_embeddings_for_symbol(&self, symbol_id: &str) -> Result<()> {
        // Get all vector_ids before deleting metadata
        let mut stmt = self
            .conn
            .prepare("SELECT vector_id FROM embeddings WHERE symbol_id = ?1")?;
        let vector_ids: Vec<String> = stmt
            .query_map(params![symbol_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Delete metadata (cascades due to FK)
        self.conn.execute(
            "DELETE FROM embeddings WHERE symbol_id = ?1",
            params![symbol_id],
        )?;

        // Delete vector data
        for vector_id in vector_ids {
            self.conn.execute(
                "DELETE FROM embedding_vectors WHERE vector_id = ?1",
                params![vector_id],
            )?;
        }

        debug!("Deleted embeddings for symbol: {}", symbol_id);
        Ok(())
    }

    /// Load all embeddings for a specific model from disk into memory
    pub fn load_all_embeddings(&self, model_name: &str) -> Result<HashMap<String, Vec<f32>>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.symbol_id, ev.vector_data, ev.dimensions
             FROM embeddings e
             JOIN embedding_vectors ev ON e.vector_id = ev.vector_id
             WHERE e.model_name = ?1",
        )?;

        let rows = stmt.query_map(params![model_name], |row| {
            let symbol_id: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            let dimensions: i64 = row.get(2)?;
            Ok((symbol_id, bytes, dimensions))
        })?;

        let mut embeddings = HashMap::new();
        let mut loaded_count = 0;

        for row_result in rows {
            let (symbol_id, bytes, dimensions) = row_result?;

            // Deserialize bytes to f32 vector
            if bytes.len() != (dimensions as usize * 4) {
                warn!(
                    "Skipping corrupted embedding for symbol {}: invalid size",
                    symbol_id
                );
                continue;
            }

            let vector: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            embeddings.insert(symbol_id, vector);
            loaded_count += 1;
        }

        info!(
            "Loaded {} embeddings for model '{}' from disk",
            loaded_count, model_name
        );
        Ok(embeddings)
    }

    /// Count total embeddings
    pub fn count_embeddings(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))?;

        Ok(count)
    }
}
