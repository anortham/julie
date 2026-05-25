//! Daemon-state and Julie-home exclusion coverage.

use super::*;

#[test]
fn test_daemon_state_files_in_blacklisted_filenames() {
    // These filenames must be rejected by the filename blacklist so a stray
    // daemon state file in a workspace never lands in the index.
    for daemon_file in &[
        "daemon.token",
        "daemon-mcp.token",
        "daemon-mcp-transport.json",
        "daemon.state",
    ] {
        assert!(
            BLACKLISTED_FILENAMES.contains(daemon_file),
            "{} must be in BLACKLISTED_FILENAMES",
            daemon_file,
        );
    }
}

#[test]
fn test_should_index_file_rejects_daemon_state_filenames() {
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;
    let temp_dir = tempfile::TempDir::new().unwrap();

    for filename in &[
        "daemon.token",
        "daemon-mcp.token",
        "daemon-mcp-transport.json",
        "daemon.state",
    ] {
        let path = temp_dir.path().join(filename);
        std::fs::write(&path, "secret\n").unwrap();
        let result = tool.should_index_file(&path, &blacklisted_exts, max_file_size, false);
        assert!(result.is_ok());
        assert!(
            !result.unwrap(),
            "{} must be rejected by filename blacklist",
            filename
        );
    }
}

#[test]
#[serial_test::serial(julie_home_env)]
fn test_should_index_file_rejects_paths_under_julie_home() {
    use crate::paths::DaemonPaths;

    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;

    let workspace = tempfile::TempDir::new().unwrap();
    // Operator sets JULIE_HOME *inside* the workspace tree — the exact
    // failure mode reported in Finding 1.
    let julie_home = workspace.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let prev = std::env::var("JULIE_HOME").ok();
    // SAFETY: serial guard ensures no other thread mutates JULIE_HOME.
    unsafe { std::env::set_var("JULIE_HOME", &julie_home) };

    // Verify the helper resolves the configured home to what we just set.
    let paths =
        DaemonPaths::try_new().expect("DaemonPaths::try_new should succeed with absolute path");
    assert!(paths.is_under_julie_home(&julie_home));

    // A daemon discovery file inside the JULIE_HOME tree must be rejected
    // even though its extension (.json) and filename (`discovery.json`) are
    // otherwise indexable.
    let discovery = julie_home.join("discovery.json");
    std::fs::write(&discovery, "{}\n").unwrap();
    let result = tool
        .should_index_file(&discovery, &blacklisted_exts, max_file_size, false)
        .expect("should_index_file must not error");
    assert!(
        !result,
        "files under the configured JULIE_HOME must NEVER be indexed"
    );

    // A normal source file in the workspace, OUTSIDE julie_home, must still
    // be indexable.
    let normal = workspace.path().join("src.rs");
    std::fs::write(&normal, "fn main() {}\n").unwrap();
    let result = tool
        .should_index_file(&normal, &blacklisted_exts, max_file_size, false)
        .expect("should_index_file must not error");
    assert!(
        result,
        "normal source files outside JULIE_HOME must still be indexed"
    );

    // Restore env.
    match prev {
        Some(v) => unsafe { std::env::set_var("JULIE_HOME", v) },
        None => unsafe { std::env::remove_var("JULIE_HOME") },
    }
}
