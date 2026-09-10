//! Event dispatching logic for the file watcher.

use std::path::{Path, PathBuf};
use tracing::info;

use super::types::{FileChangeEvent, FileChangeType};

/// Skip DELETE events that are atomic saves (file still exists).
///
/// Returns `Some(path)` when a DELETE is skipped so the caller can drop that
/// path from its dedup map. Store writes happen in the queue processor.
pub(crate) fn dispatch_file_event(
    event: FileChangeEvent,
    _workspace_root: &Path,
) -> Option<PathBuf> {
    match event.change_type {
        FileChangeType::Deleted if event.path.exists() => {
            info!(
                "Skipping DELETE for {} (file still exists, likely atomic save)",
                event.path.display()
            );
            Some(event.path)
        }
        _ => None,
    }
}
