use crate::watcher::types::{FileChangeEvent, FileChangeType};
use std::collections::VecDeque;
use std::path::Path;

pub(crate) const MAX_QUEUE_SIZE: usize = 1000;
pub(crate) const OVERFLOW_TARGET_SIZE: usize = 750;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EnqueueOutcome {
    pub drained: usize,
}

pub(crate) fn enqueue_file_change(
    queue: &mut VecDeque<FileChangeEvent>,
    event: FileChangeEvent,
) -> EnqueueOutcome {
    let incoming_path = affected_path(&event);

    if let Some(existing_index) = queue
        .iter()
        .rposition(|existing| affected_path(existing) == incoming_path)
    {
        if let Some(existing) = queue.get(existing_index).cloned() {
            queue[existing_index] = merge_file_change(&existing, event);
        }
        return EnqueueOutcome { drained: 0 };
    }

    let mut drained = 0;
    if queue.len() >= MAX_QUEUE_SIZE {
        while queue.len() > OVERFLOW_TARGET_SIZE {
            queue.pop_front();
            drained += 1;
        }
    }

    queue.push_back(event);
    EnqueueOutcome { drained }
}

pub(crate) fn merge_file_change(
    existing: &FileChangeEvent,
    incoming: FileChangeEvent,
) -> FileChangeEvent {
    let change_type = match (&existing.change_type, &incoming.change_type) {
        (FileChangeType::Modified, FileChangeType::Modified) => FileChangeType::Modified,
        (FileChangeType::Created, FileChangeType::Modified) => FileChangeType::Created,
        (FileChangeType::Deleted, FileChangeType::Created | FileChangeType::Modified) => {
            FileChangeType::Modified
        }
        (FileChangeType::Created, FileChangeType::Deleted) => FileChangeType::Deleted,
        (FileChangeType::Renamed { from, to }, FileChangeType::Modified) => {
            FileChangeType::Renamed {
                from: from.clone(),
                to: to.clone(),
            }
        }
        _ => incoming.change_type.clone(),
    };

    let path = match &change_type {
        FileChangeType::Renamed { to, .. } => to.clone(),
        _ => incoming.path.clone(),
    };

    FileChangeEvent {
        path,
        change_type,
        timestamp: incoming.timestamp,
    }
}

pub(crate) fn affected_path(event: &FileChangeEvent) -> &Path {
    match &event.change_type {
        FileChangeType::Renamed { to, .. } => to.as_path(),
        _ => event.path.as_path(),
    }
}
