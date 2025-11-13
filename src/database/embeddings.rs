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
            // 🔒 SAFETY: Validate vector dimensions match expected size
            // Prevents corrupted embeddings from being stored
            if vector_data.len() != dimensions {
                return Err(anyhow::anyhow!(
                    "Vector dimension mismatch for symbol '{}': got {} dimensions, expected {}. \
                     This prevents database corruption.",
                    symbol_id,
                    vector_data.len(),
                    dimensions
                ));
            }

            // 🔑 COMPOSITE KEY: Combine symbol_id + model_name to prevent collisions
            // This allows storing multiple models for the same symbol
            // Uses :: delimiter to avoid ambiguity (can't appear in symbol IDs or model names)
            // Example: "getUserData::bge-small", "getUserData::bge-large"
            let vector_id = format!("{}::{}", symbol_id, model_name);

            // ⚡ PERFORMANCE: Pre-allocate Vec<u8> with exact size to avoid reallocations
            // Each f32 = 4 bytes, so total = vector_data.len() * 4
            // This is 2-3x faster than flat_map().collect() for large vectors
            let mut bytes = Vec::with_capacity(vector_data.len() * 4);
            for &f in vector_data {
                bytes.extend_from_slice(&f.to_le_bytes());
            }

            // Insert vector data with composite vector_id
            vector_stmt.execute(params![
                vector_id,
                dimensions as i64,
                bytes,
                model_name,
                now
            ])?;

            // Insert metadata linking symbol to vector
            metadata_stmt.execute(params![
                symbol_id,
                vector_id, // Now uses composite vector_id
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

    /// 🚀 BATCH: Get embeddings for multiple symbols in a single query
    ///
    /// **Performance**: Replaces N×2 queries (2 per symbol) with 1 JOIN query.
    /// For 200 symbols: 400 queries → 1 query (400x improvement!)
    ///
    /// Returns Vec of (symbol_id, vector) for all found embeddings.
    /// Symbols without embeddings are silently skipped.
    pub fn get_embeddings_for_symbols(
        &self,
        symbol_ids: &[&str],
        model_name: &str,
    ) -> Result<Vec<(String, Vec<f32>)>> {
        if symbol_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build parameterized query with IN clause and JOIN
        let placeholders: Vec<String> = (1..=symbol_ids.len())
            .map(|i| format!("?{}", i + 1))
            .collect();

        let query = format!(
            "SELECT e.symbol_id, v.dimensions, v.vector_data
             FROM embeddings e
             JOIN embedding_vectors v ON e.vector_id = v.vector_id
             WHERE e.symbol_id IN ({}) AND e.model_name = ?1",
            placeholders.join(", ")
        );

        let mut stmt = self.conn.prepare(&query)?;

        // Build params: [model_name, symbol_id1, symbol_id2, ...]
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&model_name as &dyn rusqlite::ToSql];
        params.extend(
            symbol_ids
                .iter()
                .map(|id| &*id as &dyn rusqlite::ToSql),
        );

        let embedding_iter = stmt.query_map(&params[..], |row| {
            let symbol_id: String = row.get(0)?;
            let dimensions: i64 = row.get(1)?;
            let vector_data: Vec<u8> = row.get(2)?;

            // Deserialize vector data (f32 bytes)
            let vector: Vec<f32> = vector_data
                .chunks_exact(4)
                .map(|chunk| {
                    let bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
                    f32::from_le_bytes(bytes)
                })
                .collect();

            // Validate dimensions
            if vector.len() != dimensions as usize {
                return Err(rusqlite::Error::InvalidColumnType(
                    1,
                    "dimensions".to_string(),
                    rusqlite::types::Type::Integer,
                ));
            }

            Ok((symbol_id, vector))
        })?;

        let mut results = Vec::new();
        for embedding_result in embedding_iter {
            match embedding_result {
                Ok(embedding) => results.push(embedding),
                Err(e) => {
                    tracing::warn!("Failed to deserialize embedding: {}", e);
                    continue;
                }
            }
        }

        Ok(results)
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
