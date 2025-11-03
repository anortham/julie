//! Atomic cleanup utilities for test isolation

use anyhow::Result;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

/// Atomically cleanup .julie directory with retries
/// Prevents "disk I/O error 1802" from concurrent cleanup attempts
pub fn atomic_cleanup_julie_dir(workspace_path: &Path) -> Result<()> {
    let julie_dir = workspace_path.join(".julie");
    if !julie_dir.exists() {
        return Ok(());
    }

    // Attempt cleanup with exponential backoff
    // On Windows, file locking produces OS error 32 (process cannot access file)
    // which is ErrorKind::Other, not PermissionDenied
    for attempt in 1..=5 {
        match fs::remove_dir_all(&julie_dir) {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                std::thread::sleep(Duration::from_millis(100 * attempt));
                continue;
            }
            // Windows file locking error (OS error 32)
            Err(e) if e.kind() == io::ErrorKind::Other && e.raw_os_error() == Some(32) => {
                std::thread::sleep(Duration::from_millis(100 * attempt));
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("Failed to cleanup .julie directory after 5 attempts")
}
