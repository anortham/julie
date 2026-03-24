use crate::watcher::filtering;
use crate::watcher::types::{FileChangeEvent, FileChangeType};
use anyhow::Result;
use ignore::gitignore::Gitignore;
use notify::event::{ModifyKind, RenameMode};
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
        // Rename with both paths known (inotify on Linux). Emit a proper Renamed event.
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            if event.paths.len() == 2 {
                let from = event.paths[0].clone();
                let to = event.paths[1].clone();
                let from_relevant = filtering::should_process_deletion(
                    &from,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                );
                let to_relevant = filtering::should_index_file(
                    &to,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                );
                if from_relevant || to_relevant {
                    let change_event = FileChangeEvent {
                        path: to.clone(),
                        change_type: FileChangeType::Renamed { from, to },
                        timestamp: SystemTime::now(),
                    };
                    queue_file_change(index_queue.clone(), change_event).await;
                }
            }
        }
        // Old path moved away (Windows/inotify split rename). Treat as deletion.
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
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
        // New path appeared (Windows/inotify split rename). Treat as creation.
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
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
        // Unknown rename (macOS FSEvents emits RenameMode::Any with one path).
        // Fall through to Modified so the affected file gets re-indexed.
        EventKind::Modify(ModifyKind::Name(_)) => {
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

const MAX_QUEUE_SIZE: usize = 1000;

/// Queue a file change event, capping the queue at MAX_QUEUE_SIZE.
/// If the queue is full, the oldest events are dropped with a warning.
async fn queue_file_change(
    index_queue: std::sync::Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    event: FileChangeEvent,
) {
    debug!("Queueing file change: {:?}", event);
    let mut queue = index_queue.lock().await;
    if queue.len() >= MAX_QUEUE_SIZE {
        // Drain oldest events to stay within cap. A burst this large likely
        // means a large directory operation (checkout, unzip) — we'll catch up
        // on the next full re-index rather than processing stale events.
        let drain_count = queue.len() - MAX_QUEUE_SIZE + 1;
        queue.drain(..drain_count);
        tracing::warn!(
            "Watcher queue exceeded {} items; dropped {} oldest events (large directory operation?)",
            MAX_QUEUE_SIZE,
            drain_count,
        );
    }
    queue.push_back(event);
}
