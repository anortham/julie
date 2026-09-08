//! src/tests/runtime_scheduler.rs
//! Verification tests for DirtyQueue, WorkspaceScheduler, and host slot admission.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use crate::workspace_runtime::dirty_queue::{DirtyOp, DirtyQueue};
use crate::workspace_runtime::scheduler::{
    FileCommitter, ProcessFairScheduler, SchedulerConfig, WorkspaceScheduler,
};
use julie_core::workspace::host_slots::IndexJobAdmission;
use julie_core::workspace::mutation_gate::Registry as MutationGateRegistry;
use tokio_util::sync::CancellationToken;

#[test]
fn dirty_queue_overflow_requests_rescan_without_unbounded_growth() {
    let mut queue = DirtyQueue::new(4096);
    for n in 0..5000 {
        queue.record(format!("source/file_{n}.rs"));
    }
    assert!(queue.rescan_required());
    assert!(queue.path_count() <= 4096);
    queue.record("source/last.rs".into());
    let chunk = queue.take_chunk();
    assert!(chunk.rescan_required);
}

#[test]
fn dirty_queue_deduplicates_paths_and_handles_renames() {
    let mut queue = DirtyQueue::new(10);
    queue.record("src/main.rs".into());
    queue.record("./src/main.rs".into());
    queue.record("src//main.rs".into());
    queue.record(r"src\main.rs".into());
    assert_eq!(queue.path_count(), 1);

    queue.record_rename("src/old.rs".into(), "src/new.rs".into());
    assert_eq!(queue.path_count(), 3);

    let chunk = queue.take_chunk();
    assert_eq!(chunk.paths.len(), 3);
    assert_eq!(chunk.paths[0], "src/main.rs");
    assert_eq!(chunk.entries[1].op, DirtyOp::Delete);
    assert_eq!(chunk.entries[1].path, "src/old.rs");
    assert_eq!(chunk.entries[2].op, DirtyOp::Update);
    assert_eq!(chunk.entries[2].path, "src/new.rs");
}

#[test]
fn dirty_queue_directory_delete_triggers_rescan() {
    let mut queue = DirtyQueue::new(10);
    queue.record("src/file1.rs".into());
    queue.record("src/file2.rs".into());
    assert_eq!(queue.path_count(), 2);

    queue.record_delete("src/nested".into(), true);
    assert!(queue.rescan_required());
    assert_eq!(queue.path_count(), 0);

    let chunk = queue.take_chunk();
    assert!(chunk.rescan_required);
}

#[test]
fn dirty_queue_take_chunk_preserves_subsequent_events() {
    let mut queue = DirtyQueue::new(10);
    queue.record("src/first.rs".into());

    let chunk1 = queue.take_chunk();
    assert_eq!(chunk1.paths, vec!["src/first.rs"]);

    // Event arriving while chunk1 is in-flight
    queue.record("src/second.rs".into());
    let chunk2 = queue.take_chunk();
    assert_eq!(chunk2.paths, vec!["src/second.rs"]);
    assert_eq!(chunk1.sequence + 1, chunk2.sequence);
}

struct MockCommitter {
    commits: Arc<AtomicUsize>,
    rescan_called: Arc<AtomicBool>,
    delay: Duration,
}

#[async_trait::async_trait]
impl FileCommitter for MockCommitter {
    async fn commit_file(&self, _path: &str, _op: DirtyOp) -> Result<(), String> {
        if self.delay > Duration::ZERO {
            tokio::time::sleep(self.delay).await;
        }
        self.commits.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn execute_full_rescan(&self) -> Result<usize, String> {
        self.rescan_called.store(true, Ordering::SeqCst);
        Ok(5)
    }
}

#[tokio::test]
async fn scheduler_enforces_quantum_and_requeues_unfinished() {
    let temp_dir = tempfile::tempdir().unwrap();
    let scheduler_dir = temp_dir.path().join("scheduler");
    let admission = Arc::new(IndexJobAdmission::open_or_init(&scheduler_dir, Some(1)).unwrap());
    let mutation_gate = Arc::new(MutationGateRegistry::new());

    let config = SchedulerConfig {
        quantum: Duration::from_millis(30),
        max_file_size: 1024 * 1024,
        admission_timeout: Duration::from_secs(5),
    };

    let scheduler = WorkspaceScheduler::new(
        "ws-1".into(),
        temp_dir.path().to_path_buf(),
        admission.clone(),
        mutation_gate,
        config,
    );

    // Record 5 files
    for i in 0..5 {
        scheduler
            .record_file_change(format!("src/file_{i}.rs"))
            .await;
    }

    let committer = MockCommitter {
        commits: Arc::new(AtomicUsize::new(0)),
        rescan_called: Arc::new(AtomicBool::new(false)),
        delay: Duration::from_millis(20), // Each commit takes 20ms, quantum is 30ms -> should yield after 2 files
    };

    let cancel = CancellationToken::new();
    let report = scheduler
        .execute_quantum(&committer, None, &cancel)
        .await
        .unwrap();

    assert!(
        report.quantum_yielded,
        "Quantum must yield when time exceeded"
    );
    assert!(
        report.files_processed >= 1,
        "At least 1 file must be committed"
    );
    assert!(
        report.requeued_count > 0,
        "Unfinished paths must be requeued"
    );
    assert!(
        scheduler.has_pending_work().await,
        "Remaining files must be in queue"
    );

    // Host slot must be released immediately upon yielding!
    let permit = admission
        .try_acquire()
        .expect("Host slot must be released after quantum");
    drop(permit);
}

#[tokio::test]
async fn process_fair_scheduler_fifo_and_dedup() {
    let fair = ProcessFairScheduler::new();

    assert!(fair.enqueue_workspace("ws-1").await);
    assert!(
        !fair.enqueue_workspace("ws-1").await,
        "Duplicate enqueue must be rejected"
    );
    assert!(fair.enqueue_workspace("ws-2").await);

    assert_eq!(fair.pending_count().await, 2);

    // Cancel workspace removes without stale entries
    fair.cancel_workspace("ws-1").await;
    assert_eq!(fair.pending_count().await, 1);
    assert!(
        fair.enqueue_workspace("ws-1").await,
        "Re-enqueue after cancellation must succeed"
    );
}
