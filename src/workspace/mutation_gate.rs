//! Mutation gate for workspace writers.
//!
//! Provides a per-workspace `AsyncMutex`-backed proof-token API that serializes
//! all mutation paths (watcher event processor, repair scan, catch-up indexer,
//! force-reindex, etc.) without the old lossy pause/resume approach.
//!
//! # Usage
//!
//! ```rust,no_run
//! use julie::workspace::mutation_gate::{MutationGuard, acquire_gate};
//!
//! async fn my_mutation(workspace_id: &str) {
//!     let guard = acquire_gate(workspace_id).await;
//!     do_write(&guard).await;
//! }
//!
//! async fn do_write(_guard: &MutationGuard<'_>) {
//!     // only callable with a valid guard in scope
//! }
//! ```
//!
//! # Compile-time enforcement
//!
//! `MutationGuard` cannot be constructed outside this module — the inner field
//! is private and there is no public constructor.  Any function that accepts
//! `_guard: &MutationGuard<'_>` therefore cannot be called without first
//! calling [`acquire_gate`], which is a compile-time guarantee.
//!
//! ```compile_fail
//! use julie::workspace::mutation_gate::MutationGuard;
//!
//! // ERROR: field `_guard` of struct `MutationGuard` is private
//! let _bad = MutationGuard { _guard: todo!() };
//! ```

use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use std::sync::Arc;

/// Returns (or initialises) the process-wide cache mapping workspace IDs to
/// their corresponding async mutex.
fn gate_cache() -> &'static StdMutex<HashMap<String, Arc<AsyncMutex<()>>>> {
    static GATES: OnceLock<StdMutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    GATES.get_or_init(|| StdMutex::new(HashMap::new()))
}

/// Look up or create the `Arc<AsyncMutex<()>>` for a given workspace ID.
///
/// Two calls with the same `workspace_id` always return a clone of the same
/// `Arc`, so acquiring the mutex from either side actually contends on the
/// same lock.
fn gate_arc_for(workspace_id: &str) -> Arc<AsyncMutex<()>> {
    let mut map = match gate_cache().lock() {
        Ok(g) => g,
        Err(poisoned) => {
            tracing::warn!(
                "Mutation gate cache mutex poisoned, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };
    map.entry(workspace_id.to_owned())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

/// Proof token: holds the async mutex guard for the duration of a mutation.
///
/// Every mutation function that touches workspace state takes
/// `_guard: &MutationGuard<'_>` as a required parameter.  Because this struct
/// has no public constructor, callers must go through [`acquire_gate`] — making
/// it impossible to invoke a gated function without holding the lock.
///
/// Dropping a `MutationGuard` releases the lock immediately.
pub struct MutationGuard<'a> {
    /// Private — no public constructor means external code cannot fabricate a
    /// guard without going through `acquire_gate`.
    _guard: OwnedMutexGuard<()>,
    /// Phantom lifetime so the guard can be borrowed by mutation functions.
    _lifetime: std::marker::PhantomData<&'a ()>,
}

/// Acquire the workspace mutation gate for `workspace_id`, blocking until any
/// other writer releases it.
///
/// Returns a [`MutationGuard`] that must be passed (by reference) to every
/// mutation function.  The lock is released when the guard is dropped.
///
/// # Deadlock note
///
/// `tokio::sync::Mutex` is **not** reentrant.  Do not call `acquire_gate` with
/// the same `workspace_id` while already holding a guard for that workspace —
/// it will deadlock.  The proof-token API makes nested acquisition visible at
/// compile time: if you already have a `&MutationGuard<'_>` in scope, pass
/// that reference instead of calling `acquire_gate` again.
pub async fn acquire_gate(workspace_id: &str) -> MutationGuard<'static> {
    let arc = gate_arc_for(workspace_id);
    let guard = arc.lock_owned().await;
    MutationGuard {
        _guard: guard,
        _lifetime: std::marker::PhantomData,
    }
}

/// Clear the process-wide gate cache.
///
/// Only for use in tests.  Resets all per-workspace mutexes so that each test
/// starts from a clean state.
#[cfg(test)]
pub fn clear_cache_for_test() {
    if let Ok(mut map) = gate_cache().lock() {
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Same workspace_id produces the same Arc (pointer equality).
    #[tokio::test]
    async fn test_same_workspace_id_returns_same_arc() {
        clear_cache_for_test();

        let a = gate_arc_for("ws_aabbccdd");
        let b = gate_arc_for("ws_aabbccdd");

        assert!(
            Arc::ptr_eq(&a, &b),
            "Two calls with the same workspace_id must return the same Arc"
        );
    }

    /// Different workspace IDs produce different Arcs (separate locks).
    #[tokio::test]
    async fn test_different_workspace_ids_return_different_arcs() {
        clear_cache_for_test();

        let a = gate_arc_for("ws_aabbccdd");
        let b = gate_arc_for("ws_11223344");

        assert!(
            !Arc::ptr_eq(&a, &b),
            "Different workspace_ids must produce different Arcs"
        );
    }

    /// Dropping a guard releases the lock — a subsequent acquire_gate call
    /// on the same workspace should not deadlock.
    #[tokio::test]
    async fn test_guard_drop_releases_lock() {
        clear_cache_for_test();

        {
            let _g = acquire_gate("ws_drop_test").await;
            // _g is dropped here
        }

        // If the guard was not released, this would deadlock; the timeout makes
        // the test fail instead.
        let result = timeout(Duration::from_millis(200), acquire_gate("ws_drop_test")).await;
        assert!(result.is_ok(), "Lock should be released after guard drop");
    }

    /// Concurrent acquire_gate calls for the same workspace serialize: the
    /// second caller cannot proceed until the first releases the lock.
    #[tokio::test]
    async fn test_concurrent_acquisition_serializes() {
        clear_cache_for_test();

        // Acquire the gate and hold it in a background task.
        let arc = gate_arc_for("ws_concurrent");
        // Lock the underlying mutex directly so we can hold it without a MutationGuard.
        let direct_lock = arc.clone().lock_owned().await;

        // Spawn a task that tries to acquire the gate for the same workspace.
        let handle = tokio::spawn(async { acquire_gate("ws_concurrent").await });

        // The spawned task should NOT be able to acquire the gate while we hold
        // `direct_lock`.  Give it 100 ms; if it resolves within that window,
        // something is wrong.
        let should_timeout = timeout(Duration::from_millis(100), handle).await;
        assert!(
            should_timeout.is_err(),
            "Second acquire_gate should block while first lock is held"
        );

        // Now release the lock and verify the spawned task can finish.
        drop(direct_lock);

        // Spawn a fresh task since the original handle was consumed.
        let result = timeout(Duration::from_millis(200), acquire_gate("ws_concurrent")).await;
        assert!(
            result.is_ok(),
            "acquire_gate should succeed after first lock is released"
        );
    }

    /// A gated function (taking `&MutationGuard<'_>`) can only be called with a
    /// real guard obtained from `acquire_gate`.  This test verifies the runtime
    /// path works; the compile-time enforcement is proved by the `compile_fail`
    /// doctest on `MutationGuard`.
    #[tokio::test]
    async fn test_proof_token_required_to_call_gated_fn() {
        clear_cache_for_test();

        fn gated_mutation(_guard: &MutationGuard<'_>) -> &'static str {
            "mutation ran"
        }

        let guard = acquire_gate("ws_proof").await;
        let result = gated_mutation(&guard);
        assert_eq!(result, "mutation ran");
    }
}
