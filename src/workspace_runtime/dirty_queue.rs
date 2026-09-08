//! src/workspace_runtime/dirty_queue.rs
//! Bounded, coalescing dirty path queue for workspace file changes.
//!
//! Enforces:
//! 1. Bounded capacity: at most 4,096 entries before latching `rescan_required = true`.
//! 2. Path coalescing: duplicate paths collapse by canonical relative path.
//! 3. Renames recorded as old-path delete + new-path update.
//! 4. Directory deletions trigger `rescan_required` and clear path set.
//! 5. `take_chunk()` snapshots queue sequence without clearing subsequent events.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const DEFAULT_DIRTY_QUEUE_CAPACITY: usize = 4096;

/// Operation type for a dirty path entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DirtyOp {
    /// File created or modified.
    Update,
    /// File deleted.
    Delete,
}

/// A specific file change entry in the dirty queue.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DirtyEntry {
    pub path: String,
    pub op: DirtyOp,
}

/// A snapshot chunk taken from the dirty queue for indexing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyChunk {
    /// Canonical relative paths in discovery sequence.
    pub paths: Vec<String>,
    /// Detailed operations for each path.
    pub entries: Vec<DirtyEntry>,
    /// Whether a full directory reconciliation scan is required.
    pub rescan_required: bool,
    /// Monotonic sequence identifier for this chunk.
    pub sequence: u64,
}

impl DirtyChunk {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && !self.rescan_required
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }
}

/// Normalizes a path string into a canonical Unix-style relative path.
/// Collapses redundant slashes, removes '.', resolves '..', and trims whitespace.
pub fn normalize_relative_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim();
    let mut parts = Vec::new();

    for component in trimmed.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }

    parts.join("/")
}

/// Coalescing, bounded queue for file modification events.
#[derive(Debug, Clone)]
pub struct DirtyQueue {
    limit: usize,
    paths: HashSet<String>,
    ordered_entries: Vec<DirtyEntry>,
    rescan_required: bool,
    sequence: u64,
}

impl Default for DirtyQueue {
    fn default() -> Self {
        Self::new(DEFAULT_DIRTY_QUEUE_CAPACITY)
    }
}

impl DirtyQueue {
    pub fn new(limit: usize) -> Self {
        Self {
            limit: if limit == 0 {
                DEFAULT_DIRTY_QUEUE_CAPACITY
            } else {
                limit
            },
            paths: HashSet::new(),
            ordered_entries: Vec::new(),
            rescan_required: false,
            sequence: 0,
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn rescan_required(&self) -> bool {
        self.rescan_required
    }

    pub fn path_count(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && !self.rescan_required
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Record a file create/update event.
    /// Duplicate paths collapse by canonical relative path.
    pub fn record(&mut self, path: String) {
        self.record_op(path, DirtyOp::Update);
    }

    /// Record a file or directory deletion.
    /// Directory deletions clear the pending path set and latch `rescan_required = true`.
    pub fn record_delete(&mut self, path: String, is_dir: bool) {
        if is_dir {
            self.trigger_directory_deletion();
            return;
        }
        self.record_op(path, DirtyOp::Delete);
    }

    /// Record a rename as old-path delete + new-path update.
    pub fn record_rename(&mut self, old_path: String, new_path: String) {
        self.record_delete(old_path, false);
        self.record(new_path);
    }

    /// Explicitly trigger a directory deletion event.
    pub fn trigger_directory_deletion(&mut self) {
        self.paths.clear();
        self.ordered_entries.clear();
        self.rescan_required = true;
    }

    /// Internal helper recording an operation with bounded capacity and deduplication.
    fn record_op(&mut self, path: String, op: DirtyOp) {
        if self.rescan_required {
            return;
        }

        let canonical = normalize_relative_path(&path);
        if canonical.is_empty() {
            return;
        }

        if self.paths.contains(&canonical) {
            // Update operation in-place if already queued
            if let Some(existing) = self
                .ordered_entries
                .iter_mut()
                .find(|e| e.path == canonical)
            {
                existing.op = op;
            }
            return;
        }

        if self.paths.len() >= self.limit {
            self.paths.clear();
            self.ordered_entries.clear();
            self.rescan_required = true;
            return;
        }

        self.paths.insert(canonical.clone());
        self.ordered_entries.push(DirtyEntry {
            path: canonical,
            op,
        });
    }

    /// Snapshot and drain the current queue into a `DirtyChunk`.
    /// Monotonically increments sequence ID and resets queue rescan flag
    /// so events arriving during the active scan are preserved.
    pub fn take_chunk(&mut self) -> DirtyChunk {
        self.sequence = self.sequence.wrapping_add(1);
        let rescan_required = self.rescan_required;
        let entries = std::mem::take(&mut self.ordered_entries);
        let paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
        self.paths.clear();
        self.rescan_required = false;

        DirtyChunk {
            paths,
            entries,
            rescan_required,
            sequence: self.sequence,
        }
    }

    /// Requeue unfinished paths (e.g. from quantum preemption) back into the queue.
    pub fn requeue_paths(&mut self, unfinished: Vec<DirtyEntry>) {
        if self.rescan_required {
            return;
        }
        for entry in unfinished {
            self.record_op(entry.path, entry.op);
        }
    }

    /// Reset queue to empty state.
    pub fn clear(&mut self) {
        self.paths.clear();
        self.ordered_entries.clear();
        self.rescan_required = false;
    }
}
