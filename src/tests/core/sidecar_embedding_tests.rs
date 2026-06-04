//! Top-crate-only sidecar test: `sidecar_root_path` resolution from a source checkout.
//!
//! The bulk of the sidecar tests were relocated into `julie-pipeline`
//! (`crates/julie-pipeline/src/tests/sidecar_embedding_tests.rs`). This single test
//! stays in the top crate because it relies on `CARGO_MANIFEST_DIR` pointing at the
//! workspace root (which contains `python/embeddings_sidecar`). From the
//! `julie-pipeline` crate, `CARGO_MANIFEST_DIR` is the pipeline crate dir, so this
//! priority-3 branch of the fallback chain can only be exercised here.

#[cfg(test)]
#[cfg(feature = "embeddings-sidecar")]
mod tests {
    #[test]
    #[serial_test::serial(embedding_env)]
    fn test_sidecar_root_path_succeeds_from_source_checkout() {
        use crate::embeddings::sidecar_supervisor::sidecar_root_path;

        // Running from source checkout, so priority 3 (CARGO_MANIFEST_DIR) should match.
        let path =
            sidecar_root_path().expect("sidecar_root_path should succeed from source checkout");
        let path_str = path.to_string_lossy().replace('\\', "/");
        assert!(
            path_str.contains("python/embeddings_sidecar"),
            "expected path to contain 'python/embeddings_sidecar', got: {path_str}"
        );
    }
}
