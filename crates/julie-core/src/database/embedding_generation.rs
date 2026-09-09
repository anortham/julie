use super::SymbolDatabase;
use crate::embeddings_contract::CURRENT_EMBEDDING_FORMAT_VERSION;
use anyhow::{Result, anyhow, bail};
use rusqlite::{OptionalExtension, params};
use tracing::{debug, info};

fn get_unix_timestamp() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| anyhow!("System time error: {}", e))
}

pub use super::embedding_generation_types::*;

impl SymbolDatabase {
    pub(crate) fn create_embedding_generations_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embedding_generations (
                id INTEGER PRIMARY KEY AUTOINCREMENT, encoder_key TEXT NOT NULL,
                source_revision INTEGER NOT NULL, dimensions INTEGER NOT NULL,
                status TEXT NOT NULL CHECK(status IN ('building', 'ready', 'failed', 'stale', 'superseded')),
                eligible_symbols INTEGER NOT NULL DEFAULT 0, embedded_symbols INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_embedding_generations_lookup ON embedding_generations(encoder_key, source_revision, status);
            CREATE INDEX IF NOT EXISTS idx_embedding_generations_status ON embedding_generations(status);",
        )?;
        debug!("Created embedding_generations table and indexes");
        Ok(())
    }

    /// Begin a new embedding generation within an atomic transaction.
    ///
    /// Atomically marks existing building or ready generations as stale/superseded,
    /// checks vector dimensions / model identity against `embedding_config`,
    /// clears or recreates `symbol_vectors` if incompatible, updates config,
    /// and returns the new generation ID within a single transaction.
    pub fn begin_embedding_generation(
        &mut self,
        encoder_key: &str,
        source_revision: i64,
        dimensions: usize,
    ) -> Result<i64> {
        let now = get_unix_timestamp()?;

        let tx = self.conn.transaction()?;

        let current_config: Option<(String, i64, u32)> = tx
            .query_row(
                "SELECT model_name, dimensions, format_version FROM embedding_config WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let (needs_recreate, needs_clear) = match &current_config {
            Some((_, dims, _)) if *dims != dimensions as i64 => (true, false),
            Some((model, _, _)) if model != encoder_key => (false, true),
            Some(_) => (false, false),
            None => (true, false),
        };

        if needs_recreate {
            info!(
                "Embedding dimensions changed or new; recreating symbol_vectors table with dim {dimensions}"
            );
            tx.execute_batch(&format!(
                "DROP TABLE IF EXISTS symbol_vectors;
                 CREATE VIRTUAL TABLE symbol_vectors USING vec0(symbol_id text primary key, embedding float[{dimensions}]);"
            ))?;
        } else if needs_clear {
            info!(
                "Encoder key changed with identical dimensions ({dimensions}); clearing existing embeddings"
            );
            tx.execute("DELETE FROM symbol_vectors", [])?;
        }

        tx.execute(
            "INSERT INTO embedding_config (id, model_name, dimensions, format_version) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET model_name = excluded.model_name, dimensions = excluded.dimensions, format_version = excluded.format_version",
            params![encoder_key, dimensions as i64, CURRENT_EMBEDDING_FORMAT_VERSION],
        )?;
        tx.execute(
            "UPDATE embedding_generations SET status = 'superseded', updated_at = ?1 WHERE status IN ('building', 'ready')",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO embedding_generations (encoder_key, source_revision, dimensions, status, eligible_symbols, embedded_symbols, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'building', 0, 0, ?4, ?4)",
            params![encoder_key, source_revision, dimensions as i64, now],
        )?;

        let generation_id = tx.last_insert_rowid();
        tx.commit()?;

        debug!(
            "Began embedding generation {} for encoder '{}' at revision {}",
            generation_id, encoder_key, source_revision
        );
        Ok(generation_id)
    }

    /// Publish an embedding generation as ready.
    ///
    /// Verifies that the generation exists, status is building, the source revision matches,
    /// canonical revision matches (when canonical revisions exist), and stored vectors exist.
    pub fn publish_embedding_generation(
        &mut self,
        generation_id: i64,
        source_revision: i64,
        eligible_symbols: usize,
        embedded_symbols: usize,
    ) -> Result<()> {
        let now = get_unix_timestamp()?;
        let tx = self.conn.transaction()?;

        let (row_rev, status): (i64, String) = tx
            .query_row(
                "SELECT source_revision, status FROM embedding_generations WHERE id = ?1",
                params![generation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| anyhow!("Embedding generation {} not found", generation_id))?;

        if status != "building" {
            bail!(
                "Cannot publish embedding generation {generation_id}: current status is '{status}', expected 'building'"
            );
        }

        if row_rev != source_revision {
            bail!(
                "Cannot publish embedding generation {generation_id}: source revision mismatch (expected {row_rev}, got {source_revision})"
            );
        }

        // Verify canonical workspace revision if canonical revisions table is populated
        let canonical_rev: Option<i64> = tx
            .query_row(
                "SELECT revision FROM canonical_revisions ORDER BY revision DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(c_rev) = canonical_rev {
            if c_rev != source_revision {
                bail!(
                    "Cannot publish embedding generation {generation_id}: canonical revision {c_rev} differs from source revision {source_revision}"
                );
            }
        }

        // Verify actual stored vector coverage
        let actual_vector_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM symbol_vectors", [], |row| row.get(0))?;

        if actual_vector_count as usize != embedded_symbols {
            bail!(
                "Cannot publish embedding generation {generation_id}: actual stored vector count ({actual_vector_count}) does not match declared embedded symbols ({embedded_symbols})"
            );
        }

        if eligible_symbols == 0 && actual_vector_count != 0 {
            bail!(
                "Cannot publish embedding generation {generation_id}: eligible symbols is 0 but actual stored vector count is {actual_vector_count}"
            );
        }

        if embedded_symbols < eligible_symbols {
            bail!(
                "Cannot publish incomplete embedding generation {generation_id}: {embedded_symbols}/{eligible_symbols} symbols embedded"
            );
        }

        tx.execute(
            "UPDATE embedding_generations SET status = 'ready', eligible_symbols = ?1, embedded_symbols = ?2, updated_at = ?3 WHERE id = ?4",
            params![eligible_symbols as i64, embedded_symbols as i64, now, generation_id],
        )?;

        tx.commit()?;

        info!(
            "Published embedding generation {} (ready, {}/{} symbols, rev {})",
            generation_id, embedded_symbols, eligible_symbols, source_revision
        );
        Ok(())
    }

    /// Check if a compatible ready embedding generation exists for the given encoder and revision.
    pub fn embedding_generation_ready(
        &self,
        encoder_key: &str,
        source_revision: i64,
    ) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM embedding_generations WHERE encoder_key = ?1 AND source_revision = ?2 AND status = 'ready'",
            params![encoder_key, source_revision],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Store a batch of embeddings guarded by generation ID within a single atomic transaction.
    ///
    /// Refuses writes if the generation is no longer in `building` status.
    pub fn store_embeddings_for_generation(
        &mut self,
        generation_id: i64,
        embeddings: &[(String, Vec<f32>)],
    ) -> Result<usize> {
        let tx = self.conn.transaction()?;

        let is_building: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM embedding_generations WHERE id = ?1 AND status = 'building')",
            params![generation_id],
            |row| row.get(0),
        )?;

        if !is_building {
            bail!(
                "Refusing vector batch write: embedding generation {} is not in building state",
                generation_id
            );
        }

        let mut del_stmt = tx.prepare("DELETE FROM symbol_vectors WHERE symbol_id = ?1")?;
        let mut ins_stmt =
            tx.prepare("INSERT INTO symbol_vectors (symbol_id, embedding) VALUES (?1, ?2)")?;
        for (symbol_id, vector) in embeddings {
            del_stmt.execute(params![symbol_id])?;
            ins_stmt.execute(params![
                symbol_id,
                zerocopy::AsBytes::as_bytes(vector.as_slice())
            ])?;
        }
        drop(del_stmt);
        drop(ins_stmt);

        let count = embeddings.len();
        let now = get_unix_timestamp()?;
        tx.execute(
            "UPDATE embedding_generations SET embedded_symbols = embedded_symbols + ?1, updated_at = ?2 WHERE id = ?3",
            params![count as i64, now, generation_id],
        )?;

        tx.commit()?;
        Ok(count)
    }

    /// Store embeddings for an incremental file update, admitted only if an active ready generation exists.
    ///
    /// Validates generation existence, ready status, matching encoder key, and matching revision.
    /// Atomically deletes old embeddings for the file, inserts new embeddings, updates source revision to
    /// max(expected_revision, latest_canonical), and updates embedded/eligible symbol counts.
    pub fn store_file_embeddings_for_ready_generation(
        &mut self,
        file_path: &str,
        encoder_key: &str,
        expected_generation_id: i64,
        expected_source_revision: i64,
        embeddings: &[(String, Vec<f32>)],
    ) -> Result<()> {
        self.store_file_embeddings_for_ready_generation_with_extra_kinds(
            file_path,
            encoder_key,
            expected_generation_id,
            expected_source_revision,
            embeddings,
            &[],
        )
    }

    /// Store embeddings for an incremental file update with optional configured extra kinds per language.
    pub fn store_file_embeddings_for_ready_generation_with_extra_kinds(
        &mut self,
        file_path: &str,
        encoder_key: &str,
        expected_generation_id: i64,
        expected_source_revision: i64,
        embeddings: &[(String, Vec<f32>)],
        extra_kinds_per_lang: &[(String, Vec<String>)],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;

        let gen_row: Option<(String, i64, i64, String, i64, i64)> = tx
            .query_row(
                "SELECT encoder_key, dimensions, source_revision, status, eligible_symbols, embedded_symbols
                 FROM embedding_generations WHERE id = ?1",
                params![expected_generation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .optional()?;

        let Some((gen_encoder, dims, gen_rev, status, _old_eligible, _old_embedded)) = gen_row
        else {
            bail!(
                "Refusing watcher vector update for '{file_path}': no ready embedding generation exists (generation {expected_generation_id} not found)"
            );
        };

        if status != "ready" {
            bail!(
                "Refusing watcher vector update for '{file_path}': no ready embedding generation exists (generation {expected_generation_id} status is '{status}')"
            );
        }

        if gen_encoder != encoder_key {
            bail!(
                "Refusing watcher vector update for '{file_path}': encoder key mismatch (expected '{gen_encoder}', got '{encoder_key}')"
            );
        }

        if gen_rev != expected_source_revision {
            bail!(
                "Refusing watcher vector update for '{file_path}': generation {expected_generation_id} revision advanced from {expected_source_revision} to {gen_rev} during inference"
            );
        }

        if let Some((_, sample_vec)) = embeddings.first() {
            if sample_vec.len() != dims as usize {
                bail!(
                    "Refusing watcher vector update for '{file_path}': dimension mismatch (expected {dims}, got {})",
                    sample_vec.len()
                );
            }
        }

        // Delete old embeddings for this file and any orphaned embeddings
        tx.execute(
            "DELETE FROM symbol_vectors WHERE symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1) OR symbol_id NOT IN (SELECT id FROM symbols)",
            params![file_path],
        )?;

        // Insert new embeddings
        if !embeddings.is_empty() {
            let mut stmt =
                tx.prepare("INSERT INTO symbol_vectors (symbol_id, embedding) VALUES (?1, ?2)")?;
            for (symbol_id, vector) in embeddings {
                stmt.execute(params![
                    symbol_id,
                    zerocopy::AsBytes::as_bytes(vector.as_slice())
                ])?;
            }
        }

        let actual_vectors: i64 =
            tx.query_row("SELECT COUNT(*) FROM symbol_vectors", [], |r| r.get(0))?;

        let eligibility_filter =
            super::embedding_generation_eligibility::sql_symbol_eligibility_filter(
                "s",
                extra_kinds_per_lang,
            );

        let embeddable_query = format!("SELECT COUNT(*) FROM symbols s WHERE {eligibility_filter}");
        let embeddable_symbols: i64 = tx.query_row(&embeddable_query, [], |r| r.get(0))?;

        let canonical_rev: Option<i64> = tx
            .query_row(
                "SELECT revision FROM canonical_revisions ORDER BY revision DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let latest_canonical = canonical_rev.unwrap_or(expected_source_revision);
        let new_embedded = actual_vectors as usize;
        let new_eligible = (embeddable_symbols as usize).max(new_embedded);

        // Check if there are outstanding file revisions between expected_source_revision and latest_canonical
        // with missing embeddings. If earlier files failed or haven't been embedded, do not advance source_revision.
        let outstanding_query = format!(
            "SELECT EXISTS (
                SELECT 1 FROM revision_file_changes rfc
                WHERE rfc.revision > ?1 AND rfc.revision <= ?2
                  AND rfc.file_path != ?3
                  AND rfc.change_kind IN ('added', 'modified')
                  AND EXISTS (
                      SELECT 1 FROM symbols s
                      WHERE s.file_path = rfc.file_path
                        AND {eligibility_filter}
                        AND s.id NOT IN (SELECT symbol_id FROM symbol_vectors)
                  )
            )"
        );
        let has_outstanding_changed_files: bool = tx.query_row(
            &outstanding_query,
            params![expected_source_revision, latest_canonical, file_path],
            |r| r.get(0),
        )?;

        let new_source_rev = if has_outstanding_changed_files || new_embedded < new_eligible {
            expected_source_revision
        } else {
            latest_canonical.max(expected_source_revision)
        };

        let now = get_unix_timestamp()?;
        tx.execute(
            "UPDATE embedding_generations SET source_revision = ?1, eligible_symbols = ?2, embedded_symbols = ?3, updated_at = ?4 WHERE id = ?5",
            params![new_source_rev, new_eligible as i64, new_embedded as i64, now, expected_generation_id],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Mark a generation as failed (e.g. on pipeline error or unrecoverable worker cancellation).
    pub fn fail_embedding_generation(&mut self, generation_id: i64) -> Result<()> {
        let now = get_unix_timestamp()?;
        self.conn.execute(
            "UPDATE embedding_generations SET status = 'failed', updated_at = ?1 WHERE id = ?2 AND status = 'building'",
            params![now, generation_id],
        )?;
        Ok(())
    }

    /// Cleanup stale/abandoned generations (e.g. from process crash).
    ///
    /// Marks any leftover `building` generations as `failed`.
    pub fn cleanup_stale_generations(&mut self) -> Result<usize> {
        let now = get_unix_timestamp()?;
        self.conn.execute(
            "UPDATE embedding_generations SET status = 'failed', updated_at = ?1 WHERE status = 'building'",
            params![now],
        ).map_err(Into::into)
    }

    /// Retrieve an embedding generation record by ID.
    pub fn get_embedding_generation(&self, id: i64) -> Result<Option<EmbeddingGeneration>> {
        self.conn
            .query_row(
                "SELECT id, encoder_key, source_revision, dimensions, status, eligible_symbols, embedded_symbols, created_at, updated_at FROM embedding_generations WHERE id = ?1",
                params![id],
                |row| Ok(Self::map_embedding_generation_row(row)),
            )
            .optional()?
            .transpose()
    }

    /// Retrieve the latest ready generation for this database, if any.
    pub fn get_latest_ready_generation(&self) -> Result<Option<EmbeddingGeneration>> {
        self.conn
            .query_row(
                "SELECT id, encoder_key, source_revision, dimensions, status, eligible_symbols, embedded_symbols, created_at, updated_at FROM embedding_generations WHERE status = 'ready' ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok(Self::map_embedding_generation_row(row)),
            )
            .optional()?
            .transpose()
    }

    /// Convenience helper for unit tests and fixtures to populate and publish a generation immediately.
    pub fn publish_test_generation(
        &mut self,
        encoder_key: &str,
        source_revision: i64,
        dimensions: usize,
    ) -> Result<i64> {
        let gen_id = self.begin_embedding_generation(encoder_key, source_revision, dimensions)?;
        self.publish_embedding_generation(gen_id, source_revision, 0, 0)?;
        Ok(gen_id)
    }

    fn map_embedding_generation_row(row: &rusqlite::Row<'_>) -> Result<EmbeddingGeneration> {
        let status_str: String = row.get(4)?;
        let status = EmbeddingGenerationStatus::from_str(&status_str)
            .ok_or_else(|| anyhow!("Unknown embedding generation status: {}", status_str))?;

        Ok(EmbeddingGeneration {
            id: row.get(0)?,
            encoder_key: row.get(1)?,
            source_revision: row.get(2)?,
            dimensions: row.get::<_, i64>(3)? as usize,
            status,
            eligible_symbols: row.get::<_, i64>(5)? as usize,
            embedded_symbols: row.get::<_, i64>(6)? as usize,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}
