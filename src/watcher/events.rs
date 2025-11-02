//! File system event processing pipeline
//!
//! This module handles the conversion of notify::Event instances into
//! FileChangeEvent entries queued for processing.

use crate::watcher::types::FileChangeEvent;
use crate::watcher::types::FileChangeType;
use anyhow::Result;
use notify::{Event, EventKind};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::time::SystemTime;
use tokio::sync::Mutex as TokioMutex;
use tracing::debug;

/// Process a file system event and queue any relevant changes
pub async fn process_file_system_event(
    supported_extensions: &HashSet<String>,
    ignore_patterns: &[glob::Pattern],
    index_queue: std::sync::Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    event: Event,
) -> Result<()> {
    debug!("Processing file system event: {:?}", event);

    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if should_index_file(&path, supported_extensions, ignore_patterns) {
                    let change_event = FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Created,
                        timestamp: SystemTime::now(),
                    };
                    queue_file_change(index_queue.clone(), change_event).await;
                }
            }
        }
        EventKind::Modify(_) => {
            for path in event.paths {
                if should_index_file(&path, supported_extensions, ignore_patterns) {
                    let change_event = FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Modified,
                        timestamp: SystemTime::now(),
                    };
                    queue_file_change(index_queue.clone(), change_event).await;
                }
            }
        }
        EventKind::Remove(_) => {
            for path in event.paths {
                // Check ignore patterns before processing deletion
                if should_index_file(&path, supported_extensions, ignore_patterns) {
                    let change_event = FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Deleted,
                        timestamp: SystemTime::now(),
                    };
                    queue_file_change(index_queue.clone(), change_event).await;
                }
            }
        }
        _ => {
            // Handle other events like renames if needed
            debug!("Ignoring event kind: {:?}", event.kind);
        }
    }

    Ok(())
}

/// Check if a file should be indexed (local helper using filtering module functions)
fn should_index_file(
    path: &Path,
    supported_extensions: &HashSet<String>,
    ignore_patterns: &[glob::Pattern],
) -> bool {
    // Check if it's a file
    if !path.is_file() {
        return false;
    }

    // Check extension
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if !supported_extensions.contains(ext) {
            return false;
        }
    } else {
        return false; // No extension
    }

    // Check ignore patterns
    let path_str = path.to_string_lossy();
    for pattern in ignore_patterns {
        if pattern.matches(&path_str) {
            return false;
        }
    }

    true
}

/// Queue a file change event for processing
async fn queue_file_change(
    index_queue: std::sync::Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    event: FileChangeEvent,
) {
    debug!("Queueing file change: {:?}", event);

    let mut queue = index_queue.lock().await;
    queue.push_back(event);
}
