//! Smoke tests for embedding dependencies (sqlite-vec + zerocopy).
//!
//! These tests validate that the core embedding dependencies compile and work
//! correctly with Julie's existing dependency tree (rusqlite 0.37 bundled).

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use zerocopy::AsBytes;

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
