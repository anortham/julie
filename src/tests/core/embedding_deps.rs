//! Smoke tests for embedding dependencies (fastembed + sqlite-vec + zerocopy).
//!
//! These tests validate that the core embedding dependencies compile and work
//! correctly with Julie's existing dependency tree (rusqlite 0.37 bundled).

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    #[cfg(feature = "embeddings-ort")]
    use serial_test::serial;
    use zerocopy::AsBytes;

    #[test]
    fn test_default_build_enables_ort_backend_feature() {
        assert!(
            cfg!(feature = "embeddings-ort"),
            "Default build should enable embeddings-ort feature"
        );
    }

    #[test]
    fn test_default_build_enables_sidecar_backend_feature() {
        assert!(
            cfg!(feature = "embeddings-sidecar"),
            "Default build should enable embeddings-sidecar feature"
        );
    }

    /// Verify sqlite-vec loads and vec_version() returns a version string.
    #[test]
    fn test_sqlite_vec_registration_and_version() {
        // Register sqlite-vec as auto-extension (idempotent via Once in production,
        // but safe to call directly in tests)
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let conn = Connection::open_in_memory().expect("Failed to open in-memory db");
        let version: String = conn
            .query_row("SELECT vec_version()", [], |row| row.get(0))
            .expect("vec_version() should return a string");

        assert!(!version.is_empty(), "vec_version() should not be empty");
        // Version should be a semver-ish string like "0.1.6"
        assert!(
            version.contains('.'),
            "vec_version() should contain a dot: {version}"
        );
    }

    /// Verify sqlite-vec can serialize and deserialize vectors via zerocopy.
    #[test]
    fn test_sqlite_vec_vector_roundtrip() {
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let conn = Connection::open_in_memory().expect("Failed to open in-memory db");
        let v: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4];

        let json: String = conn
            .query_row("SELECT vec_to_json(?)", [v.as_bytes()], |row| row.get(0))
            .expect("vec_to_json should work");

        // Should contain the float values
        assert!(json.contains("0.1"), "JSON should contain 0.1: {json}");
        assert!(json.contains("0.4"), "JSON should contain 0.4: {json}");
    }

    /// Verify fastembed can initialize and produce correct embeddings.
    ///
    /// Tests both single and batch embedding in one test to avoid concurrent
    /// model initialization issues (hf-hub download race condition with 416 errors).
    ///
    /// NOTE: This test downloads the BGE-small model on first run (~30MB).
    /// Subsequent runs use the cached model.
    #[cfg(feature = "embeddings-ort")]
    #[test]
    #[serial(fastembed)]
    fn test_fastembed_single_and_batch_embedding() {
        use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

        // Use a stable absolute cache path — fastembed defaults to relative
        // `.fastembed_cache/` which breaks when cargo test changes CWD.
        let cache_dir =
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
                .join(".cache")
                .join("fastembed");

        let mut model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(false),
        )
        .expect("fastembed model should initialize");

        // Single embedding: verify dimensions
        let embeddings = model
            .embed(vec!["test embedding input".to_string()], None)
            .expect("single embedding should succeed");

        assert_eq!(embeddings.len(), 1, "Should produce one embedding");
        assert_eq!(
            embeddings[0].len(),
            384,
            "BGE-small-en-v1.5 should produce 384-dim vectors"
        );

        // Embeddings should be normalized (L2 norm ≈ 1.0)
        let norm: f32 = embeddings[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "Embedding should be roughly unit-normalized, got {norm}"
        );

        // Batch embedding: verify multiple texts
        let texts = vec![
            "function to handle errors".to_string(),
            "class for database connection".to_string(),
            "struct representing user data".to_string(),
        ];

        let batch_embeddings = model
            .embed(texts, None)
            .expect("batch embedding should succeed");

        assert_eq!(batch_embeddings.len(), 3, "Should produce three embeddings");
        for (i, emb) in batch_embeddings.iter().enumerate() {
            assert_eq!(emb.len(), 384, "Embedding {i} should be 384-dim");
        }
    }

    #[cfg(feature = "embeddings-ort")]
    #[test]
    fn test_ort_policy_matches_platform() {
        let policy = crate::embeddings::ort_execution_provider_policy_kinds();

        #[cfg(target_os = "windows")]
        assert_eq!(policy, vec!["directml", "cpu"]);

        #[cfg(not(target_os = "windows"))]
        assert!(
            policy.is_empty(),
            "macOS/Linux should have no accelerated EP"
        );
    }

    /// Verify zerocopy AsBytes works for f32 slices (used to pass vectors to sqlite-vec).
    #[test]
    fn test_zerocopy_f32_to_bytes() {
        let v: Vec<f32> = vec![1.0, 2.0, 3.0];
        let bytes = v.as_bytes();
        // 3 floats × 4 bytes each = 12 bytes
        assert_eq!(bytes.len(), 12, "3 f32s should be 12 bytes");

        // Verify roundtrip: bytes back to f32s
        let floats: &[f32] =
            unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const f32, bytes.len() / 4) };
        assert_eq!(floats, &[1.0, 2.0, 3.0]);
    }
}
