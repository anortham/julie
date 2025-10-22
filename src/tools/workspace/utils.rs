// src/tools/workspace/utils.rs
//! Shared utility functions for workspace operations

use anyhow::Result;
use std::path::Path;

/// Calculate the total size of a directory recursively
///
/// **IMPORTANT**: This function performs blocking I/O and should be called
/// from within `tokio::task::spawn_blocking` when used in async contexts.
///
/// # Arguments
/// * `path` - The directory path to calculate size for
///
/// # Returns
/// * `Ok(u64)` - Total size in bytes
/// * `Err` - I/O errors during directory traversal
///
/// # Example
/// ```rust,no_run
/// # use std::path::PathBuf;
/// # use anyhow::Result;
/// # fn calculate_dir_size(path: &std::path::Path) -> Result<u64> { Ok(0) }
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// // In async context:
/// let path = PathBuf::from("/path/to/dir");
/// let size = tokio::task::spawn_blocking(move || {
///     calculate_dir_size(&path)
/// }).await??;
/// # Ok(())
/// # }
/// ```
pub fn calculate_dir_size<P: AsRef<Path>>(path: P) -> Result<u64> {
    let mut total_size = 0;
    let entries = std::fs::read_dir(path)?;

    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            total_size += calculate_dir_size(entry.path())?;
        } else {
            total_size += metadata.len();
        }
    }

    Ok(total_size)
}
