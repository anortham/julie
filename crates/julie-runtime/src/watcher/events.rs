use crate::watcher::filtering;
use crate::watcher::queue;
use crate::watcher::types::{FileChangeEvent, FileChangeType};
use anyhow::Result;
use ignore::gitignore::Gitignore;
use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use tokio::sync::Mutex as TokioMutex;
use tracing::debug;

/// Process a file system event and queue any relevant changes.
///
/// `needs_rescan` is set to true if the queue overflows (>1000 events). The caller
/// should trigger a workspace-wide staleness check when this flag is observed.
pub async fn process_file_system_event(
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
    index_queue: std::sync::Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    event: Event,
    needs_rescan: &Arc<AtomicBool>,
) -> Result<()> {
    debug!("Processing file system event: {:?}", event);
    let change_events =
        classify_file_system_event(supported_extensions, gitignore, workspace_root, event);

    if change_events.is_empty() {
        return Ok(());
    }

    for change_event in change_events {
        queue_file_change(index_queue.clone(), change_event, needs_rescan).await;
    }

    Ok(())
}

fn classify_file_system_event(
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
    event: Event,
) -> Vec<FileChangeEvent> {
    let mut queued = Vec::new();
    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if filtering::should_index_file(
                    &path,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                ) {
                    queued.push(FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Created,
                        timestamp: SystemTime::now(),
                    });
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
                    queued.push(FileChangeEvent {
                        path: to.clone(),
                        change_type: FileChangeType::Renamed { from, to },
                        timestamp: SystemTime::now(),
                    });
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
                    queued.push(FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Deleted,
                        timestamp: SystemTime::now(),
                    });
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
                    queued.push(FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Created,
                        timestamp: SystemTime::now(),
                    });
                }
            }
        }
        // Unknown rename (macOS FSEvents emits RenameMode::Any with one path).
        // Fix A: check whether the path still exists to determine the correct event type.
        // - Path gone: the file was moved away — emit Deleted to clean up stale DB entries.
        // - Path exists: the file was moved here — emit Modified to re-index its content.
        // Previously this always fell through to Modified, causing should_index_file to
        // return false for a gone path (it checks path.is_file()), silently dropping the
        // event and leaving orphaned symbols/embeddings in the database.
        EventKind::Modify(ModifyKind::Name(_)) => {
            for path in event.paths {
                if path.exists() {
                    if filtering::should_index_file(
                        &path,
                        supported_extensions,
                        gitignore,
                        workspace_root,
                    ) {
                        queued.push(FileChangeEvent {
                            path: path.clone(),
                            change_type: FileChangeType::Modified,
                            timestamp: SystemTime::now(),
                        });
                    }
                } else if filtering::should_process_deletion(
                    &path,
                    supported_extensions,
                    gitignore,
                    workspace_root,
                ) {
                    queued.push(FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Deleted,
                        timestamp: SystemTime::now(),
                    });
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
                    queued.push(FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Modified,
                        timestamp: SystemTime::now(),
                    });
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
                    queued.push(FileChangeEvent {
                        path: path.clone(),
                        change_type: FileChangeType::Deleted,
                        timestamp: SystemTime::now(),
                    });
                }
            }
        }
        _ => {
            debug!("Ignoring event kind: {:?}", event.kind);
        }
    }

    queued
}

/// Queue a file change event with per-path coalescing and overflow headroom drain.
///
/// Events for the same affected path are merged so repeated activity does not
/// consume additional queue slots. Distinct events respect the queue cap and
/// drain to `OVERFLOW_TARGET_SIZE` when a new event would overflow.
async fn queue_file_change(
    index_queue: std::sync::Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    event: FileChangeEvent,
    needs_rescan: &Arc<AtomicBool>,
) {
    debug!("Queueing file change: {:?}", event);
    let mut queue = index_queue.lock().await;
    let outcome = queue::enqueue_file_change(&mut queue, event);
    if outcome.drained > 0 {
        needs_rescan.store(true, Ordering::Release);
        tracing::warn!(
            max_size = queue::MAX_QUEUE_SIZE,
            target_size = queue::OVERFLOW_TARGET_SIZE,
            dropped = outcome.drained,
            final_len = queue.len(),
            "Watcher queue overflow drain applied; rescan scheduled"
        );
    }
}
