//! Tests for embedded sidecar extraction and sidecar_root_path fallback chain.

#[cfg(test)]
#[cfg(feature = "embeddings-sidecar")]
mod tests {
    use std::fs;

    use crate::embeddings::sidecar_embedded::extract_embedded_sidecar;
    use crate::embeddings::sidecar_supervisor::INSTALL_MARKER_VERSION;

    // =========================================================================
    // Extraction function tests
    // =========================================================================

    #[test]
    fn test_extract_embedded_sidecar_writes_all_expected_files() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let target = tmp.path();

        extract_embedded_sidecar(target).expect("extraction should succeed");

        // pyproject.toml must exist
        assert!(
            target.join("pyproject.toml").exists(),
            "pyproject.toml should be extracted"
        );

        // All sidecar Python source files
        for name in ["__init__.py", "main.py", "runtime.py", "protocol.py"] {
            let path = target.join("sidecar").join(name);
            assert!(path.exists(), "sidecar/{name} should be extracted");
        }

        // Version marker must exist
        assert!(
            target.join(".embedded-version").exists(),
            ".embedded-version marker should be written"
        );

        // Marker content should match the install marker version exactly
        let marker = fs::read_to_string(target.join(".embedded-version"))
            .expect("read marker");
        assert_eq!(
            marker.trim(),
            INSTALL_MARKER_VERSION,
            "marker should equal version: got {marker}"
        );
    }

    #[test]
    fn test_extract_embedded_sidecar_skips_when_version_matches() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let target = tmp.path();

        // First extraction
        extract_embedded_sidecar(target).expect("first extraction");

        // Record mtime of pyproject.toml
        let mtime_before = fs::metadata(target.join("pyproject.toml"))
            .expect("metadata")
            .modified()
            .expect("mtime");

        // Small delay to ensure mtime would differ if re-written
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second extraction — should be a no-op
        extract_embedded_sidecar(target).expect("second extraction");

        let mtime_after = fs::metadata(target.join("pyproject.toml"))
            .expect("metadata")
            .modified()
            .expect("mtime");

        assert_eq!(
            mtime_before, mtime_after,
            "pyproject.toml mtime should be unchanged on skip"
        );
    }

    #[test]
    fn test_extract_embedded_sidecar_re_extracts_on_version_mismatch() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let target = tmp.path();

        // First extraction
        extract_embedded_sidecar(target).expect("first extraction");

        // Tamper with the version marker
        fs::write(target.join(".embedded-version"), "stale-version")
            .expect("tamper marker");

        // Second extraction — should re-extract because version mismatches
        extract_embedded_sidecar(target).expect("re-extraction after tamper");

        let marker = fs::read_to_string(target.join(".embedded-version"))
            .expect("read marker");
        assert_eq!(
            marker.trim(),
            INSTALL_MARKER_VERSION,
            "marker should be updated to current version: got {marker}"
        );
    }

    #[test]
    fn test_extract_does_not_include_test_files() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let target = tmp.path();

        extract_embedded_sidecar(target).expect("extraction");

        // The Python sidecar has a tests/ directory — it should NOT be extracted
        assert!(
            !target.join("tests").exists(),
            "tests/ directory should not be extracted"
        );

        // __pycache__ directories should not be extracted either
        assert!(
            !target.join("sidecar").join("__pycache__").exists(),
            "sidecar/__pycache__/ should not be extracted"
        );
    }

    // =========================================================================
    // sidecar_root_path fallback chain tests
    // =========================================================================

    #[test]
    #[serial_test::serial(embedding_env)]
    fn test_sidecar_root_path_env_override_wins() {
        use crate::embeddings::sidecar_supervisor::{sidecar_root_path, SIDECAR_ROOT_ENV};

        let fake_path = "/tmp/julie-test-sidecar-override";

        unsafe {
            std::env::set_var(SIDECAR_ROOT_ENV, fake_path);
        }
        let result = sidecar_root_path();
        unsafe {
            std::env::remove_var(SIDECAR_ROOT_ENV);
        }

        let path = result.expect("sidecar_root_path should return Ok");
        assert_eq!(
            path,
            std::path::PathBuf::from(fake_path),
            "env override should win"
        );
    }

    #[test]
    #[serial_test::serial(embedding_env)]
    fn test_sidecar_root_path_succeeds_from_source_checkout() {
        use crate::embeddings::sidecar_supervisor::sidecar_root_path;

        // Running from source checkout, so priority 3 (CARGO_MANIFEST_DIR) should match
        let path = sidecar_root_path().expect("sidecar_root_path should succeed from source checkout");
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains("python/embeddings_sidecar"),
            "expected path to contain 'python/embeddings_sidecar', got: {path_str}"
        );
    }
}
