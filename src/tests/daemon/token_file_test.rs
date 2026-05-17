//! Tests for [`crate::daemon::token_file`] — write_token / read_token primitives.

mod tests {
    use crate::daemon::token_file::{read_token, write_token};
    use crate::paths::DaemonPaths;
    use tempfile::TempDir;

    fn tmp_dir() -> TempDir {
        tempfile::TempDir::new().expect("TempDir::new failed")
    }

    /// Round-trip: write then read returns the same token string.
    #[test]
    fn test_token_file_round_trip() {
        let dir = tmp_dir();
        let path = dir.path().join("daemon.token");
        let token = "supersecrettoken1234";

        write_token(&path, token).expect("write_token failed");
        let got = read_token(&path).expect("read_token failed");

        assert_eq!(got, token);
    }

    /// POSIX only: file mode must be exactly 0600 after write_token.
    #[cfg(unix)]
    #[test]
    fn test_token_file_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tmp_dir();
        let path = dir.path().join("daemon.token");

        write_token(&path, "tok").expect("write_token failed");

        let meta = std::fs::metadata(&path).expect("metadata failed");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected mode 0600, got {mode:04o}");
    }

    /// Second write wins; mode is still 0600 after overwrite.
    #[test]
    fn test_token_file_overwrites_existing() {
        let dir = tmp_dir();
        let path = dir.path().join("daemon.token");

        write_token(&path, "first_token").expect("first write_token failed");
        write_token(&path, "second_token").expect("second write_token failed");

        let got = read_token(&path).expect("read_token failed");
        assert_eq!(got, "second_token");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("metadata failed")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                mode, 0o600,
                "mode after overwrite: expected 0600 got {mode:04o}"
            );
        }
    }

    /// The `token_file()` getter on DaemonPaths returns a path under julie_home
    /// that ends with "daemon.token".
    #[test]
    fn test_token_file_path_matches_paths_helper() {
        let dir = tmp_dir();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let token_path = paths.token_file();

        assert!(
            token_path.starts_with(dir.path()),
            "token_file() should be under julie_home"
        );
        assert_eq!(
            token_path.file_name().and_then(|n| n.to_str()),
            Some("daemon.token"),
            "token_file() should end with daemon.token"
        );
    }

    /// read_token strips trailing whitespace / newlines from the stored bytes.
    #[test]
    fn test_read_token_trims_trailing_whitespace() {
        let dir = tmp_dir();
        let path = dir.path().join("daemon.token");

        // Write raw bytes including a trailing newline (simulating what writeln! produces).
        std::fs::write(&path, "abc123\n").expect("fs::write failed");

        let got = read_token(&path).expect("read_token failed");
        assert_eq!(got, "abc123");
    }
}
