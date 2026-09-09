use std::sync::{Arc, Mutex};

use anyhow::Result;

use super::run_embedding_pipeline;
use crate::embeddings::{DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity};
use julie_core::database::SymbolDatabase;

/// Mock embedding provider that returns one fewer vector than requested,
/// simulating a partial response from a real provider (e.g., GPU OOM).
struct PartialProvider {
    dims: usize,
}

impl EmbeddingProvider for PartialProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        Ok(vec![0.1f32; self.dims])
    }

    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        // Return one fewer vector than requested
        let count = texts.len().saturating_sub(1);
        Ok((0..count).map(|_| vec![0.1f32; self.dims]).collect())
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("partial-test-model", self.dims))
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "partial-mock".to_string(),
            device: "cpu".to_string(),
            model_name: "partial-test-model".to_string(),
            dimensions: self.dims,
        }
    }

    fn shutdown(&self) {}
}

fn setup_db_with_functions(count: usize) -> Arc<Mutex<SymbolDatabase>> {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).expect("create db");

    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified, last_indexed)
             VALUES ('src/lib.rs', 'rust', 'abc', 100, 0, 0)",
            [],
        )
        .unwrap();

    for i in 0..count {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, language,
                 start_line, start_col, end_line, end_col, start_byte, end_byte,
                 reference_score)
                 VALUES (?, ?, 'function', 'src/lib.rs', 'rust',
                         1, 0, 10, 0, 0, 100, 0.0)",
                rusqlite::params![format!("sym-{i}"), format!("do_work_{i}")],
            )
            .unwrap();
    }

    // Must forget the dir so the tempfile path stays valid while db is open
    std::mem::forget(dir);
    Arc::new(Mutex::new(db))
}

/// When a provider returns fewer vectors than symbols in a batch, the
/// pipeline should store the partial results rather than skipping the
/// entire batch.
///
/// Before fix: `continue` skips the batch entirely (0 embeddings stored).
/// After fix: `min(returned, requested)` embeddings are stored.
#[test]
fn test_pipeline_stores_partial_results_on_batch_mismatch() {
    // 3 symbols; PartialProvider returns len-1 = 2 vectors per batch.
    let db = setup_db_with_functions(3);
    let provider = PartialProvider { dims: 4 };

    let stats = run_embedding_pipeline(&db, &provider, None).expect("pipeline should not error");

    assert!(
        stats.symbols_embedded > 0,
        "Pipeline must store partial embeddings when provider returns fewer vectors \
         than requested. Got {} embeddings (expected > 0).",
        stats.symbols_embedded
    );
    assert_eq!(
        stats.symbols_embedded, 2,
        "Expected 2 embeddings stored (3 requested - 1 = 2 returned), got {}",
        stats.symbols_embedded
    );
}
