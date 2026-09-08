//! src/workspace_runtime/scheduler.rs
//! Workspace indexing scheduler enforcing:
//! 1. 10-second scheduling quantum checked between completed file commits.
//! 2. Cross-process host slot admission before workspace mutation gate.
//! 3. Enforcing source size limits before reading files.
//! 4. Releasing host slot permit before requeuing unfinished paths.
//! 5. Never holding host slot during semantic provider inference or while idle.
//! 6. Fair FIFO scheduling across workspaces in the process.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use super::dirty_queue::{DirtyOp, DirtyQueue};
use julie_core::workspace::host_slots::{AdmissionError, IndexJobAdmission};
use julie_core::workspace::mutation_gate::Registry as MutationGateRegistry;

pub const DEFAULT_SCHEDULING_QUANTUM: Duration = Duration::from_secs(10);
pub const DEFAULT_MAX_SOURCE_SIZE_BYTES: usize = 1024 * 1024; // 1 MiB
pub const DEFAULT_ADMISSION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Host admission error: {0}")]
    Admission(#[from] AdmissionError),
    #[error("Cancelled by caller")]
    Cancelled,
    #[error("Timeout waiting for admission or execution")]
    Timeout,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Commit error on path {path}: {message}")]
    Commit { path: String, message: String },
    #[error("Internal scheduler error: {0}")]
    Internal(String),
}

/// Configuration for indexing quantum and file size limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub quantum: Duration,
    pub max_file_size: usize,
    pub admission_timeout: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            quantum: DEFAULT_SCHEDULING_QUANTUM,
            max_file_size: DEFAULT_MAX_SOURCE_SIZE_BYTES,
            admission_timeout: DEFAULT_ADMISSION_TIMEOUT,
        }
    }
}

/// Telemetry and execution report returned by `WorkspaceScheduler::execute_quantum`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuantumReport {
    pub admission_wait: Duration,
    pub slot_index: Option<usize>,
    pub longest_file_duration: Duration,
    pub longest_file_path: Option<String>,
    pub files_processed: usize,
    pub files_skipped_size: usize,
    pub quantum_yielded: bool,
    pub rescan_performed: bool,
    pub requeued_count: usize,
    pub total_duration: Duration,
}

/// Async committer trait abstracting file parsing and database/search indexing.
#[async_trait::async_trait]
pub trait FileCommitter: Send + Sync {
    async fn commit_file(&self, path: &str, op: DirtyOp) -> Result<(), String>;
    async fn execute_full_rescan(&self) -> Result<usize, String>;
}

/// Per-workspace index scheduler.
pub struct WorkspaceScheduler {
    pub workspace_id: String,
    pub workspace_root: PathBuf,
    pub queue: Arc<Mutex<DirtyQueue>>,
    pub admission: Arc<IndexJobAdmission>,
    pub mutation_gate: Arc<MutationGateRegistry>,
    pub config: SchedulerConfig,
}

impl WorkspaceScheduler {
    pub fn new(
        workspace_id: String,
        workspace_root: PathBuf,
        admission: Arc<IndexJobAdmission>,
        mutation_gate: Arc<MutationGateRegistry>,
        config: SchedulerConfig,
    ) -> Self {
        Self {
            workspace_id,
            workspace_root,
            queue: Arc::new(Mutex::new(DirtyQueue::default())),
            admission,
            mutation_gate,
            config,
        }
    }

    pub fn queue(&self) -> &Arc<Mutex<DirtyQueue>> {
        &self.queue
    }

    pub async fn record_file_change(&self, path: String) {
        self.queue.lock().await.record(path);
    }

    pub async fn record_rename(&self, old_path: String, new_path: String) {
        self.queue.lock().await.record_rename(old_path, new_path);
    }

    pub async fn record_delete(&self, path: String, is_dir: bool) {
        self.queue.lock().await.record_delete(path, is_dir);
    }

    pub async fn has_pending_work(&self) -> bool {
        !self.queue.lock().await.is_empty()
    }

    /// Execute one indexing quantum.
    ///
    /// Lifecycle invariants enforced:
    /// 1. Takes dirty chunk snapshot (events arriving later accumulate safely).
    /// 2. Acquires host slot permit BEFORE per-workspace mutation gate.
    /// 3. Never holds SQLite mutex while waiting for host slot.
    /// 4. Checks source file size limits before reading.
    /// 5. Checks 10-second quantum between completed file commits.
    /// 6. Releases host slot BEFORE requeuing unfinished paths.
    /// 7. Never holds host slot during semantic provider inference or while idle.
    pub async fn execute_quantum<C: FileCommitter>(
        &self,
        committer: &C,
        deadline: Option<Instant>,
        cancel: &CancellationToken,
    ) -> Result<QuantumReport, SchedulerError> {
        let total_start = Instant::now();
        let mut report = QuantumReport::default();

        if cancel.is_cancelled() {
            return Err(SchedulerError::Cancelled);
        }

        // Step 1: Snapshot chunk from dirty queue.
        let chunk = {
            let mut q = self.queue.lock().await;
            if q.is_empty() {
                return Ok(report);
            }
            q.take_chunk()
        };

        if cancel.is_cancelled() {
            let mut q = self.queue.lock().await;
            q.requeue_paths(chunk.entries);
            return Err(SchedulerError::Cancelled);
        }

        // Step 2: Acquire host slot admission BEFORE acquiring mutation gate.
        let admission_start = Instant::now();
        let permit = match self.admission.acquire(deadline, Some(cancel)).await {
            Ok(p) => p,
            Err(e) => {
                let mut q = self.queue.lock().await;
                q.requeue_paths(chunk.entries);
                return Err(SchedulerError::Admission(e));
            }
        };
        report.admission_wait = admission_start.elapsed();
        report.slot_index = Some(permit.slot_index());

        if cancel.is_cancelled() {
            drop(permit);
            let mut q = self.queue.lock().await;
            q.requeue_paths(chunk.entries);
            return Err(SchedulerError::Cancelled);
        }

        // Step 3: Acquire per-workspace mutation gate under held host permit.
        let mutation_guard = self.mutation_gate.acquire(&self.workspace_id).await;

        let quantum_start = Instant::now();

        // Step 4: Full directory rescan if latched
        if chunk.rescan_required {
            info!(workspace_id = %self.workspace_id, "Executing full reconciliation rescan");
            match committer.execute_full_rescan().await {
                Ok(scanned_count) => {
                    report.files_processed += scanned_count;
                    report.rescan_performed = true;
                }
                Err(msg) => {
                    drop(mutation_guard);
                    drop(permit);
                    return Err(SchedulerError::Commit {
                        path: self.workspace_root.to_string_lossy().into(),
                        message: msg,
                    });
                }
            }
        }

        // Step 5: Process dirty entries with per-file commit & quantum check
        let mut unfinished_entries = Vec::new();

        for (idx, entry) in chunk.entries.iter().enumerate() {
            if cancel.is_cancelled() {
                unfinished_entries = chunk.entries[idx..].to_vec();
                break;
            }

            // Enforce source size limit before reading file
            if entry.op == DirtyOp::Update {
                let full_path = self.workspace_root.join(&entry.path);
                if let Ok(meta) = std::fs::metadata(&full_path) {
                    if meta.len() > self.config.max_file_size as u64 {
                        report.files_skipped_size += 1;
                        warn!(
                            workspace_id = %self.workspace_id,
                            path = %entry.path,
                            size = meta.len(),
                            max_size = self.config.max_file_size,
                            "Skipping file exceeding max_file_size limit"
                        );
                        continue;
                    }
                }
            }

            // Commit single file
            let file_start = Instant::now();
            if let Err(msg) = committer.commit_file(&entry.path, entry.op).await {
                error!(
                    workspace_id = %self.workspace_id,
                    path = %entry.path,
                    error = %msg,
                    "Failed to commit file"
                );
                drop(mutation_guard);
                drop(permit);
                return Err(SchedulerError::Commit {
                    path: entry.path.clone(),
                    message: msg,
                });
            }

            let file_duration = file_start.elapsed();
            if file_duration > report.longest_file_duration {
                report.longest_file_duration = file_duration;
                report.longest_file_path = Some(entry.path.clone());
            }
            report.files_processed += 1;

            // Check 10-second scheduling quantum between completed file commits
            if quantum_start.elapsed() >= self.config.quantum {
                debug!(
                    workspace_id = %self.workspace_id,
                    elapsed_ms = quantum_start.elapsed().as_millis(),
                    quantum_ms = self.config.quantum.as_millis(),
                    "Scheduling quantum reached; yielding turn"
                );
                report.quantum_yielded = true;
                unfinished_entries = chunk.entries[idx + 1..].to_vec();
                break;
            }
        }

        // Step 6: Quantum yield / Completion handling
        // CRITICAL INVARIANT: Drop mutation guard & host slot permit BEFORE requeueing!
        drop(mutation_guard);
        drop(permit); // Host slot is now released immediately to other contenders

        if report.quantum_yielded || !unfinished_entries.is_empty() {
            report.requeued_count = unfinished_entries.len();
            let mut q = self.queue.lock().await;
            q.requeue_paths(unfinished_entries);
        }

        report.total_duration = total_start.elapsed();
        Ok(report)
    }
}

/// Process-level coordinator ensuring FIFO order among workspaces
/// and at most one queued job per workspace.
pub struct ProcessFairScheduler {
    queue: Mutex<VecDeque<String>>,
    active_or_queued: Mutex<HashSet<String>>,
    schedulers: RwLock<HashMap<String, Arc<WorkspaceScheduler>>>,
}

impl Default for ProcessFairScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessFairScheduler {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            active_or_queued: Mutex::new(HashSet::new()),
            schedulers: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register_workspace(&self, scheduler: Arc<WorkspaceScheduler>) {
        let mut map = self.schedulers.write().await;
        map.insert(scheduler.workspace_id.clone(), scheduler);
    }

    pub async fn unregister_workspace(&self, workspace_id: &str) {
        let mut map = self.schedulers.write().await;
        map.remove(workspace_id);
    }

    /// Enqueue a workspace for an index turn.
    /// Returns true if enqueued; false if already queued or running (dedup).
    pub async fn enqueue_workspace(&self, workspace_id: &str) -> bool {
        let mut active = self.active_or_queued.lock().await;
        if active.contains(workspace_id) {
            return false;
        }

        active.insert(workspace_id.to_string());
        let mut q = self.queue.lock().await;
        q.push_back(workspace_id.to_string());
        true
    }

    /// Pop next workspace in FIFO order.
    pub async fn pop_next_workspace(&self) -> Option<Arc<WorkspaceScheduler>> {
        let id = {
            let mut q = self.queue.lock().await;
            q.pop_front()?
        };

        let map = self.schedulers.read().await;
        map.get(&id).cloned()
    }

    /// Complete a turn: if has_more_work, requeue at back of FIFO; otherwise mark idle.
    pub async fn finish_turn(&self, workspace_id: &str, has_more_work: bool) {
        let mut active = self.active_or_queued.lock().await;
        if has_more_work {
            let mut q = self.queue.lock().await;
            q.push_back(workspace_id.to_string());
        } else {
            active.remove(workspace_id);
        }
    }

    /// Remove a workspace from the queue upon cancellation to avoid leaving stale entries.
    pub async fn cancel_workspace(&self, workspace_id: &str) {
        let mut active = self.active_or_queued.lock().await;
        active.remove(workspace_id);
        let mut q = self.queue.lock().await;
        q.retain(|id| id != workspace_id);
    }

    pub async fn pending_count(&self) -> usize {
        self.queue.lock().await.len()
    }
}
