use crate::watcher::filtering;
use crate::watcher::types::{FileChangeEvent, FileChangeType};
use anyhow::Result;
use ignore::gitignore::Gitignore;
use notify::{Event, EventKind};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::time::SystemTime;
use tokio::sync::Mutex as TokioMutex;
use tracing::debug;

/// Process a file system event and queue any relevant changes
pub async fn process_file_system_event(
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
    index_queue: std::sync::Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    event: Event,
) -> Result<()> {
    debug!("Processing file system event: {:?}", event);

    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if filtering::should_index_file(
                    &path,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                ) {
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
                if filtering::should_index_file(
                    &path,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                ) {
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
                if filtering::should_process_deletion(
                    &path,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                ) {
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
            debug!("Ignoring event kind: {:?}", event.kind);
        }
    }

    Ok(())
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
