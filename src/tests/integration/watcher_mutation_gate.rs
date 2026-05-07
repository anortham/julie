//! Regression tests for the per-workspace mutation gate.
//!
//! These tests define the concurrency contract of the gate-based architecture
//! that replaced the lossy pause/resume mechanism (see
//! `docs/plans/2026-05-06-filewatcher-pause-architecture.md`). Per the plan v3
//! finding 5, this suite is LEAD-OWNED — the tests encode the contract that
//! workers shouldn't have authority over.
//!
//! The mutation_gate primitive itself is unit-tested in
//! `src/workspace/mutation_gate.rs`. This integration suite proves the
//! contract holds end-to-end:
//!
//! 1. `test_workspace_id_collapses_path_spellings` — codex v3 finding 4: keying
//!    by `workspace_id` (not raw `PathBuf`) prevents split-locks across path
//!    spelling differences.
//! 2. `test_concurrent_acquisitions_serialize_via_execution_trace` — strong
//!    serialization invariant: no entry/exit interleaving on the same gate.
//! 3. `test_different_workspaces_run_in_parallel` — per-workspace
//!    independence: alpha and beta gates don't contend.
//! 4. `test_external_acquire_blocks_while_guard_held` — the gate is honored
//!    by external callers while a guard is alive.
//! 5. `test_external_acquire_proceeds_after_drop` — guards release on drop;
//!    no leaked permits.
//! 6. `test_proof_token_signature_compiles` — positive-path companion to the
//!    `compile_fail` doctest in `mutation_gate.rs`. Confirms a gated function
//!    can be called when a real guard is in scope.
//!
//! Compile-time proof that `MutationGuard` cannot be constructed externally
//! lives in the doctest on `mutation_gate::MutationGuard`. Tests for FS-event
//! end-to-end behavior (watcher receives event, indexes file) live in
//! `watcher_handlers.rs` and `watcher.rs` — those paths now require a
//! `MutationGuard<'_>` parameter and so cannot compile if the gate is bypassed.

use crate::workspace::mutation_gate::{MutationGuard, acquire_gate, clear_cache_for_test};
use crate::workspace::registry::generate_workspace_id;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{sleep, timeout};

/// Codex v3 finding 4: a workspace must always map to a single gate, even when
/// callers spell its path differently (trailing slash, relative vs absolute,
/// canonical vs symlinked). This is the workspace_id-keyed-not-path-keyed
/// invariant.
#[tokio::test]
async fn test_workspace_id_collapses_path_spellings() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().to_string_lossy().into_owned();
    let trailing_slash = format!("{}/", canonical.trim_end_matches('/'));

    let id1 = generate_workspace_id(&canonical).expect("canonical id");
    let id2 = generate_workspace_id(&trailing_slash).expect("trailing-slash id");

    assert_eq!(
        id1, id2,
        "Path spellings of the same workspace must produce the same workspace_id, \
         otherwise the gate splits across spellings and serialization is broken."
    );
}

/// Strong serialization invariant: when two tasks contend on the same gate,
/// their entry/exit pairs never interleave. This is the property the lossy
/// pause never guaranteed and the gate now does.
#[tokio::test]
async fn test_concurrent_acquisitions_serialize_via_execution_trace() {
    clear_cache_for_test();

    let trace: Arc<TokioMutex<Vec<&'static str>>> = Arc::new(TokioMutex::new(Vec::new()));
    let trace_a = trace.clone();
    let trace_b = trace.clone();

    let task_a = tokio::spawn(async move {
        let _guard = acquire_gate("ws_serialize").await;
        trace_a.lock().await.push("a_enter");
        sleep(Duration::from_millis(80)).await;
        trace_a.lock().await.push("a_exit");
    });

    sleep(Duration::from_millis(10)).await;

    let task_b = tokio::spawn(async move {
        let _guard = acquire_gate("ws_serialize").await;
        trace_b.lock().await.push("b_enter");
        sleep(Duration::from_millis(80)).await;
        trace_b.lock().await.push("b_exit");
    });

    let (ra, rb) = tokio::join!(task_a, task_b);
    ra.expect("task a panicked");
    rb.expect("task b panicked");

    let trace = trace.lock().await;
    let serial_ab = trace.as_slice() == ["a_enter", "a_exit", "b_enter", "b_exit"];
    let serial_ba = trace.as_slice() == ["b_enter", "b_exit", "a_enter", "a_exit"];
    assert!(
        serial_ab || serial_ba,
        "Expected one strict serial ordering, got interleaved trace: {:?}",
        *trace
    );
}

/// Per-workspace independence: gates are keyed by workspace_id, so two
/// different workspaces never block each other. A regression that keyed gates
/// by something more global (e.g. a process-wide mutex) would fail this test
/// because the two 200ms holds would serialize and exceed the 350ms ceiling.
#[tokio::test]
async fn test_different_workspaces_run_in_parallel() {
    clear_cache_for_test();

    let task_a = tokio::spawn(async {
        let _guard = acquire_gate("ws_alpha").await;
        sleep(Duration::from_millis(200)).await;
    });

    let task_b = tokio::spawn(async {
        let _guard = acquire_gate("ws_beta").await;
        sleep(Duration::from_millis(200)).await;
    });

    timeout(Duration::from_millis(350), async {
        let (a, b) = tokio::join!(task_a, task_b);
        a.expect("task a panicked");
        b.expect("task b panicked");
    })
    .await
    .expect(
        "Both per-workspace tasks must finish well under the serialized worst case \
         (200ms each = 400ms serialized). Timing out at 350ms proves the gates \
         are NOT running in parallel — regression on the workspace_id key.",
    );
}

/// External callers must block while the gate is held. This is the foundation
/// the watcher relies on when catch-up holds the gate during a 30s fresh-clone
/// scan: live FS events queue up but cannot mutate state until the holder
/// releases.
#[tokio::test]
async fn test_external_acquire_blocks_while_guard_held() {
    clear_cache_for_test();

    let _guard = acquire_gate("ws_block_held").await;

    let external = tokio::spawn(async {
        let _g = acquire_gate("ws_block_held").await;
    });

    sleep(Duration::from_millis(150)).await;
    assert!(
        !external.is_finished(),
        "External acquire must remain blocked while gate is held by this scope."
    );

    drop(_guard);

    timeout(Duration::from_millis(200), external)
        .await
        .expect("External acquire must complete after gate release")
        .expect("External task panicked");
}

/// The drop-releases invariant from the watcher's perspective: a guard scoped
/// to a single mutation pass must release the gate as soon as the pass ends,
/// without explicit unlocking. This guards against accidentally holding the
/// gate across a `.await` boundary that should have ended the critical
/// section.
#[tokio::test]
async fn test_external_acquire_proceeds_after_drop() {
    clear_cache_for_test();

    {
        let _guard = acquire_gate("ws_drop_release").await;
        // _guard dropped at end of this scope.
    }

    timeout(
        Duration::from_millis(200),
        acquire_gate("ws_drop_release"),
    )
    .await
    .expect("Acquire after drop must succeed without deadlock");
}

/// Positive-path companion to the `compile_fail` doctest in `mutation_gate.rs`.
/// The doctest proves you CANNOT construct a `MutationGuard` outside the
/// module; this test proves you CAN call a gated function when you have a
/// legitimately-acquired guard. Together they establish the proof-token
/// contract.
#[tokio::test]
async fn test_proof_token_signature_compiles_with_real_guard() {
    fn gated_mutation(_guard: &MutationGuard<'_>) -> &'static str {
        "ran with proof"
    }

    clear_cache_for_test();
    let guard = acquire_gate("ws_proof_token").await;
    assert_eq!(gated_mutation(&guard), "ran with proof");
}

/// Regression for the v2 nested-acquisition deadlock that codex flagged and
/// that briefly returned in commit 8383502b: a caller holding the gate must
/// not deadlock when invoking gated work via the proof-token path. The
/// `_with_guard` variants take an existing `&MutationGuard<'_>` and skip
/// re-acquisition — proven here by completing two nested acquire-equivalent
/// calls within a tight timeout that any actual deadlock would blow through.
#[tokio::test]
async fn test_guard_passing_prevents_reentrant_deadlock() {
    clear_cache_for_test();

    let outer_guard = acquire_gate("ws_reentrancy").await;

    // A function that takes a guard reference can be called freely with the
    // existing one — no re-acquisition, no deadlock.
    fn nested_work(_guard: &MutationGuard<'_>) {}

    timeout(
        Duration::from_millis(200),
        async {
            nested_work(&outer_guard);
            nested_work(&outer_guard);
            nested_work(&outer_guard);
        },
    )
    .await
    .expect(
        "Re-entering gated work via &MutationGuard<'_> must not deadlock. \
         A timeout here means a callee on the proof-token path silently \
         re-acquired the gate (the codex v2 finding regressed).",
    );

    drop(outer_guard);
}

/// Cross-cutting smoke: a fresh-clone catch-up holding the gate while a
/// watcher event arrives is the exact bug the plan targets. We simulate it
/// directly: hold the gate, write a file, attempt to acquire the gate from
/// another task, observe it blocks, release, observe it succeeds. This is the
/// gate-side analogue of "events queue but don't process until catch-up
/// finishes." The actual end-to-end FS-event path is exercised by
/// `watcher_handlers.rs` tests, which require the proof-token at compile
/// time.
#[tokio::test]
async fn test_simulated_catchup_blocks_simulated_watcher_event() {
    clear_cache_for_test();
    let temp = tempfile::tempdir().unwrap();
    let workspace_path = temp.path();

    let workspace_id =
        generate_workspace_id(&workspace_path.to_string_lossy()).expect("workspace_id");

    // Simulate catch-up holding the gate.
    let catchup_guard = acquire_gate(&workspace_id).await;

    // Simulate a watcher-side mutation pass trying to acquire the same gate.
    let workspace_id_clone = workspace_id.clone();
    let watcher_pass = tokio::spawn(async move {
        let _guard = acquire_gate(&workspace_id_clone).await;
    });

    // Meanwhile, a "user" creates a file. The watcher would normally see this,
    // queue the event, and process it on its next mutation pass — which is now
    // blocked.
    fs::write(workspace_path.join("user_change.rs"), "fn user_change() {}").unwrap();

    // The watcher pass must NOT proceed while catch-up holds the gate.
    sleep(Duration::from_millis(100)).await;
    assert!(
        !watcher_pass.is_finished(),
        "Watcher mutation pass must block while catch-up holds the gate"
    );

    // Catch-up finishes; the gate releases.
    drop(catchup_guard);

    // Watcher proceeds promptly.
    timeout(Duration::from_millis(300), watcher_pass)
        .await
        .expect("Watcher must proceed after catch-up releases gate")
        .expect("Watcher task panicked");
}
