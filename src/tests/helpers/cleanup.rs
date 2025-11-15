//! Atomic cleanup utilities for test isolation

use anyhow::Result;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

/// Atomically cleanup .julie directory with retries
/// Prevents "disk I/O error 1802" from concurrent cleanup attempts
///
/// 🚨 SAFETY: This function includes multiple safety checks to prevent
/// accidental deletion of production .julie directories during testing.
pub fn atomic_cleanup_julie_dir(workspace_path: &Path) -> Result<()> {
    let julie_dir = workspace_path.join(".julie");
    if !julie_dir.exists() {
        return Ok(());
    }

    // Windows: Give previous test's database connections time to close
    // SQLite connections may not close immediately when handler goes out of scope
    #[cfg(target_os = "windows")]
    std::thread::sleep(Duration::from_millis(250));

    // 🚨 SAFETY CHECK 1: NEVER delete .julie from project root during tests
    // The project root contains env!("CARGO_MANIFEST_DIR") and is where production Julie runs
    let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_canonical = workspace_path
        .canonicalize()
        .unwrap_or_else(|_| workspace_path.to_path_buf());
    let project_canonical = project_root.canonicalize().unwrap_or(project_root.clone());

    if workspace_canonical == project_canonical {
        panic!(
            "🚨 SAFETY VIOLATION: Attempted to delete production .julie directory!\n\
             Path: {}\n\
             This is the project root. Tests must NEVER delete production data.\n\
             Use TempDir::new() or other guaranteed temp directories for tests.",
            workspace_path.display()
        );
    }

    // 🚨 SAFETY CHECK 2: NEVER delete .julie from current working directory
    // This catches cases where tests use std::env::current_dir() during cargo test
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_canonical = cwd.canonicalize().unwrap_or(cwd.clone());
        if workspace_canonical == cwd_canonical {
            panic!(
                "🚨 SAFETY VIOLATION: Attempted to delete .julie from current working directory!\n\
                 Path: {}\n\
                 Tests using std::env::current_dir() can delete production data.\n\
                 Use TempDir::new() or other guaranteed temp directories for tests.",
                workspace_path.display()
            );
        }
    }

    // 🚨 SAFETY CHECK 3: Require path to be in a temp directory
    // Temp directories are typically /tmp/, /var/folders/ (macOS), or C:\Users\...\AppData\Local\Temp
    let path_str = workspace_canonical.to_string_lossy();
    let is_temp_dir = path_str.starts_with("/tmp/")
        || path_str.starts_with("/var/folders/")  // macOS temp
        || path_str.contains(r"\AppData\Local\Temp\")  // Windows temp
        || path_str.contains("/fixtures/test-workspaces/")  // Test fixtures (Unix)
        || path_str.contains(r"\fixtures\test-workspaces\"); // Test fixtures (Windows)

    if !is_temp_dir {
        panic!(
            "🚨 SAFETY VIOLATION: Path is not in a recognized temp directory!\n\
             Path: {}\n\
             Only temp directories should be cleaned during tests.\n\
             Expected: /tmp/, /var/folders/, or Windows temp directories.\n\
             Use TempDir::new() to ensure proper test isolation.",
            workspace_path.display()
        );
    }

    // All safety checks passed - proceed with cleanup
    // Attempt cleanup with exponential backoff
    // On Windows, file locking produces OS error 32 (process cannot access file)
    // which is ErrorKind::Other, not PermissionDenied
    // Increased retries and delays for Windows SQLite connection cleanup (10 attempts, up to 5 seconds)
    for attempt in 1..=10 {
        match fs::remove_dir_all(&julie_dir) {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                std::thread::sleep(Duration::from_millis(200 * attempt));
                continue;
            }
            // Windows file locking error (OS error 32)
            Err(e) if e.kind() == io::ErrorKind::Other && e.raw_os_error() == Some(32) => {
                std::thread::sleep(Duration::from_millis(200 * attempt));
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("Failed to cleanup .julie directory after 10 attempts")
}
