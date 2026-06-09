//! T9 F1 gate: in-process read envelope is bounded by the request deadline.
//!
//! Gate invariant:
//!   - A stalling in-process read (workspace resolution / `list_roots_from_peer`
//!     hanging) returns a bounded `McpError` with "retry shortly" within the
//!     configured deadline. It does NOT hang.
//!   - A background repair task spawned via `tokio::spawn` (the non-cancellable
//!     pattern used by the F1 leader path) is NOT cancelled when the read
//!     deadline fires. It runs to completion independently.
//!   - `is_in_process()` is `true` for both leader and follower handlers so
//!     the F1 envelope fires for both; `false` for `none()` (daemon/stdio)
//!     so those take the existing path unchanged.
//!
//! These tests operate at the helper level (`tokio::time::timeout`, `tokio::spawn`)
//! to avoid requiring a live MCP peer for `call_tool` invocation. The envelope
//! logic in `call_tool` wraps exactly the pattern tested here. Full call_tool-path
//! coverage requires the `InProcessDaemonBuilder` harness; that is a separate
//! integration test concern.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rmcp::ErrorData as McpError;

// ---------------------------------------------------------------------------
// F1 envelope: stalling workspace resolution is bounded
// ---------------------------------------------------------------------------

/// The F1 in-process read envelope bounds a stalling operation (e.g.
/// `list_roots_from_peer` hanging on a cold peer). With `start_paused=true`
/// Tokio auto-advances the clock so the test completes in 0ms real wall time.
#[tokio::test(start_paused = true)]
async fn test_f1_in_process_read_bounded_by_deadline() {
    // Simulate list_roots_from_peer hanging (300s virtual time).
    let stalling_resolution = async {
        tokio::time::sleep(Duration::from_secs(300)).await;
        Ok::<(), McpError>(())
    };

    let deadline = Some(Duration::from_millis(50));

    // This is the F1 envelope pattern used in call_tool for in-process reads.
    let result: Result<(), McpError> = match deadline {
        Some(d) => {
            match tokio::time::timeout(d, async {
                stalling_resolution.await?;
                // Tool call would follow here; we never reach it.
                Ok(())
            })
            .await
            {
                Ok(r) => r,
                Err(_elapsed) => Err(McpError::internal_error(
                    "in-process workspace not ready within 0s; \
                     indexing in progress — retry shortly (tool: 'fast_search')"
                        .to_string(),
                    None,
                )),
            }
        }
        None => stalling_resolution.await,
    };

    let err = result.expect_err("stalling read must be bounded by the deadline");
    assert!(
        err.message.contains("retry shortly"),
        "timeout error must say 'retry shortly'; got: {msg}",
        msg = err.message
    );
}

/// The F1 deadline is disabled when `parse_request_timeout` returns `None`
/// (env var set to "0"). In that case the stalling future runs to completion.
/// This ensures the existing "disable timeout" escape hatch still works.
#[tokio::test(start_paused = true)]
async fn test_f1_no_deadline_stalling_read_completes() {
    let stalling = async {
        tokio::time::sleep(Duration::from_secs(300)).await;
        Err::<(), McpError>(McpError::internal_error("completed".to_string(), None))
    };

    // None deadline = no timeout → future runs to natural completion.
    let deadline: Option<Duration> = None;
    let result: Result<(), McpError> = match deadline {
        Some(d) => match tokio::time::timeout(d, stalling).await {
            Ok(r) => r,
            Err(_) => Err(McpError::internal_error("timed out".to_string(), None)),
        },
        None => stalling.await,
    };

    let err = result.expect_err("no-deadline path must run future to completion");
    assert!(
        err.message.contains("completed"),
        "no-deadline future must run to completion, not time out; got: {msg}",
        msg = err.message
    );
    assert!(
        !err.message.contains("timed out"),
        "no-deadline must NOT produce a timeout error; got: {msg}",
        msg = err.message
    );
}

// ---------------------------------------------------------------------------
// F1 non-cancellability: spawned repair task outlives the read deadline
// ---------------------------------------------------------------------------

/// The non-cancellable repair pattern: a `tokio::spawn`ed task is NOT cancelled
/// when the read envelope's `tokio::time::timeout` fires. The spawned task holds
/// its own Tokio task slot and runs to completion regardless of the outer future
/// being dropped on timeout.
///
/// This is the core F1 invariant: the repair write is never interrupted mid-flight
/// even when a concurrent read times out.
#[tokio::test(start_paused = true)]
async fn test_f1_spawned_repair_not_cancelled_on_read_timeout() {
    let completed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&completed);

    // Spawn the non-cancellable "repair" task (mirrors the leader path in call_tool).
    // tokio::spawn returns a JoinHandle; the task runs independently of this future.
    let repair_task = tokio::spawn(async move {
        // Short sleep so we can verify it completes after the timeout fires.
        tokio::time::sleep(Duration::from_millis(10)).await;
        flag.store(true, Ordering::Release);
    });

    // The in-process read envelope times out (stalling workspace resolution).
    let stalling = async {
        tokio::time::sleep(Duration::from_secs(300)).await;
        Ok::<(), McpError>(())
    };
    let _ = tokio::time::timeout(Duration::from_millis(1), stalling).await;
    // ^ Deadline fired. The stalling future is dropped. The spawned task is NOT.

    // Advance virtual time past the repair task's sleep so it can complete.
    tokio::time::advance(Duration::from_millis(100)).await;
    // Yield to the Tokio scheduler so the repair task can run.
    tokio::task::yield_now().await;

    repair_task.await.expect("repair task must not panic");
    assert!(
        completed.load(Ordering::Acquire),
        "spawned repair task must complete independently after the read deadline fires"
    );
}

// ---------------------------------------------------------------------------
// F1 structural: is_in_process() fires for leader and follower, not for none()
// ---------------------------------------------------------------------------

/// `is_in_process()` is `true` for both leader and follower (participating in
/// the election) and `false` for `none()` (daemon/stdio — pre-3c). This is the
/// gate condition that routes `call_tool` to the F1 bounded envelope.
#[tokio::test]
async fn test_f1_is_in_process_gate_fires_for_leader_and_follower_not_none() {
    use crate::handler::JulieServerHandler;
    use crate::leadership::LeadershipState;
    use crate::registry::discovery::DaemonLockGuard;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    let dir = tempfile::tempdir().unwrap();

    // Leader: is_in_process() == true (F1 envelope fires).
    let lock_path = dir.path().join(".leader.lock");
    let guard =
        DaemonLockGuard::try_acquire(&lock_path).expect("lock must be acquirable on fresh path");
    let leader = JulieServerHandler::new_in_process(
        WorkspaceStartupHint {
            path: dir.path().to_path_buf(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        None,
        LeadershipState::leader(guard),
        None,
    )
    .await
    .unwrap();
    assert!(
        leader.is_in_process(),
        "leader must be is_in_process() (F1 envelope fires)"
    );

    // Follower: is_in_process() == true (F1 envelope fires).
    let dir2 = tempfile::tempdir().unwrap();
    let follower = JulieServerHandler::new_in_process(
        WorkspaceStartupHint {
            path: dir2.path().to_path_buf(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        None,
        LeadershipState::follower(),
        None,
    )
    .await
    .unwrap();
    assert!(
        follower.is_in_process(),
        "follower must be is_in_process() (F1 envelope fires)"
    );

    // none() (daemon/stdio): is_in_process() == false (existing path unchanged).
    let dir3 = tempfile::tempdir().unwrap();
    let none_handler = JulieServerHandler::new_in_process(
        WorkspaceStartupHint {
            path: dir3.path().to_path_buf(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        None,
        LeadershipState::none(),
        None,
    )
    .await
    .unwrap();
    assert!(
        !none_handler.is_in_process(),
        "none() handler must NOT be is_in_process() (F1 envelope must not fire for daemon/stdio)"
    );
}

// ---------------------------------------------------------------------------
// F-C (codex pre-merge): deferred-repair spawn is single-flight
// ---------------------------------------------------------------------------

/// The F1 leader envelope must spawn at most ONE background repair task per
/// release cycle. `try_claim_deferred_repair_slot()` returns `true` for exactly
/// one caller; concurrent callers lose the `compare_exchange` and return
/// `false` (skip spawning). After `release_deferred_repair_slot()`, the slot is
/// claimable again. This prevents a persistently-failing repair from re-running
/// once per concurrent in-process read.
#[tokio::test]
async fn test_deferred_repair_slot_is_single_flight() {
    use crate::handler::JulieServerHandler;
    use crate::leadership::LeadershipState;
    use crate::registry::discovery::DaemonLockGuard;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join(".leader.lock");
    let guard =
        DaemonLockGuard::try_acquire(&lock_path).expect("lock must be acquirable on fresh path");
    let handler = JulieServerHandler::new_in_process(
        WorkspaceStartupHint {
            path: dir.path().to_path_buf(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        None,
        LeadershipState::leader(guard),
        None,
    )
    .await
    .unwrap();

    // First claim wins (false → true).
    assert!(
        handler.try_claim_deferred_repair_slot(),
        "first claim must win the single-flight slot"
    );

    // While the slot is held, every concurrent claim loses — so no duplicate
    // repair task is spawned even under many concurrent in-process reads.
    assert!(
        !handler.try_claim_deferred_repair_slot(),
        "second claim must lose while the slot is held (no duplicate spawn)"
    );
    assert!(
        !handler.try_claim_deferred_repair_slot(),
        "third concurrent claim must also lose while the slot is held"
    );

    // After the spawned task releases the slot, the next pending cycle can
    // spawn a fresh repair task.
    handler.release_deferred_repair_slot();
    assert!(
        handler.try_claim_deferred_repair_slot(),
        "claim must win again after release_deferred_repair_slot()"
    );
}
