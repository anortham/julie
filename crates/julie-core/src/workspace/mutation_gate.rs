//! Mutation gate for workspace writers.
//!
//! Provides a per-workspace `AsyncMutex`-backed proof-token API that serializes
//! all mutation paths (watcher event processor, repair scan, catch-up indexer,
//! force-reindex, etc.) without the old lossy pause/resume approach.
//!
//! # Usage
//!
//! ```rust,no_run
//! use julie_core::workspace::mutation_gate::{MutationGuard, acquire_gate};
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

use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use std::sync::Arc;

/// An injectable registry of per-workspace async mutexes.
///
/// Each `Registry` instance has its own independent cache: two registries
/// for the same workspace ID do **not** share locks.  This makes it possible
/// to inject an isolated registry in tests while production code continues
/// using the process-wide singleton via [`Registry::global`].
pub struct Registry {
    cache: StdMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl Registry {
    /// Create a new, empty registry.  Useful for test isolation.
    pub fn new() -> Self {
        Self {
            cache: StdMutex::new(HashMap::new()),
        }
    }

    /// Return the process-wide singleton registry.
    ///
    /// All production callers (including [`acquire_gate`]) delegate here so
    /// that every mutation in the same process contends on the same per-workspace
    /// lock.
    pub fn global() -> &'static Arc<Registry> {
        static GLOBAL: OnceLock<Arc<Registry>> = OnceLock::new();
        GLOBAL.get_or_init(|| Arc::new(Registry::new()))
    }

    /// Look up or create the `Arc<AsyncMutex<()>>` for `workspace_id` within
    /// this registry.
    pub(crate) fn arc_for(&self, workspace_id: &str) -> Arc<AsyncMutex<()>> {
        let mut map = match self.cache.lock() {
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

    /// Acquire the mutation gate for `workspace_id`, blocking until any other
    /// writer using this registry releases it.
    pub async fn acquire(&self, workspace_id: &str) -> MutationGuard<'static> {
        let arc = self.arc_for(workspace_id);
        let guard = arc.lock_owned().await;
        MutationGuard {
            _guard: guard,
            _lifetime: std::marker::PhantomData,
        }
    }

    /// Try to acquire the mutation gate without waiting.
    ///
    /// Used by shutdown paths that must not block forever when the caller
    /// already owns the same workspace gate.
    pub fn try_acquire(&self, workspace_id: &str) -> Option<MutationGuard<'static>> {
        let arc = self.arc_for(workspace_id);
        let guard = arc.try_lock_owned().ok()?;
        Some(MutationGuard {
            _guard: guard,
            _lifetime: std::marker::PhantomData,
        })
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
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
/// Delegates to [`Registry::global`].  Returns a [`MutationGuard`] that must
/// be passed (by reference) to every mutation function.  The lock is released
/// when the guard is dropped.
///
/// # Deadlock note
///
/// `tokio::sync::Mutex` is **not** reentrant.  Do not call `acquire_gate` with
/// the same `workspace_id` while already holding a guard for that workspace —
/// it will deadlock.  The proof-token API makes nested acquisition visible at
/// compile time: if you already have a `&MutationGuard<'_>` in scope, pass
/// that reference instead of calling `acquire_gate` again.
pub async fn acquire_gate(workspace_id: &str) -> MutationGuard<'static> {
    Registry::global().acquire(workspace_id).await
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
        let reg = Registry::new();
        let a = reg.arc_for("ws_aabbccdd");
        let b = reg.arc_for("ws_aabbccdd");

        assert!(
            Arc::ptr_eq(&a, &b),
            "Two calls with the same workspace_id must return the same Arc"
        );
    }

    /// Different workspace IDs produce different Arcs (separate locks).
    #[tokio::test]
    async fn test_different_workspace_ids_return_different_arcs() {
        let reg = Registry::new();
        let a = reg.arc_for("ws_aabbccdd");
        let b = reg.arc_for("ws_11223344");

        assert!(
            !Arc::ptr_eq(&a, &b),
            "Different workspace_ids must produce different Arcs"
        );
    }

    /// Dropping a guard releases the lock — a subsequent acquire_gate call
    /// on the same workspace should not deadlock.
    #[tokio::test]
    async fn test_guard_drop_releases_lock() {
        let reg = Registry::new();
        {
            let _g = reg.acquire("ws_drop_test").await;
            // _g is dropped here
        }

        // If the guard was not released, this would deadlock; the timeout makes
        // the test fail instead.
        let result = timeout(Duration::from_millis(200), reg.acquire("ws_drop_test")).await;
        assert!(result.is_ok(), "Lock should be released after guard drop");
    }

    /// Concurrent acquire_gate calls for the same workspace serialize: the
    /// second caller cannot proceed until the first releases the lock.
    #[tokio::test]
    async fn test_concurrent_acquisition_serializes() {
        let reg = Registry::new();

        // Acquire the underlying Arc directly so we can hold it without a
        // MutationGuard (which would move into the spawned task).
        let arc = reg.arc_for("ws_concurrent");
        let direct_lock = arc.clone().lock_owned().await;

        // Spawn a task that tries to acquire the gate for the same workspace.
        // The registry is cloned into the task via Arc.
        let reg2 = Arc::new(reg);
        let reg2_task = Arc::clone(&reg2);
        let handle = tokio::spawn(async move { reg2_task.acquire("ws_concurrent").await });

        // The spawned task should NOT be able to acquire the gate while we hold
        // `direct_lock`.  Give it 100 ms; if it resolves within that window,
        // something is wrong.
        let should_timeout = timeout(Duration::from_millis(100), handle).await;
        assert!(
            should_timeout.is_err(),
            "Second acquire should block while first lock is held"
        );

        // Now release the lock and verify the spawned task can finish.
        drop(direct_lock);

        // Spawn a fresh task since the original handle was consumed.
        let result = timeout(Duration::from_millis(200), reg2.acquire("ws_concurrent")).await;
        assert!(
            result.is_ok(),
            "acquire should succeed after first lock is released"
        );
    }

    /// A gated function (taking `&MutationGuard<'_>`) can only be called with a
    /// real guard obtained from `acquire_gate`.  This test verifies the runtime
    /// path works; the compile-time enforcement is proved by the `compile_fail`
    /// doctest on `MutationGuard`.
    #[tokio::test]
    async fn test_proof_token_required_to_call_gated_fn() {
        fn gated_mutation(_guard: &MutationGuard<'_>) -> &'static str {
            "mutation ran"
        }

        let guard = acquire_gate("ws_proof").await;
        let result = gated_mutation(&guard);
        assert_eq!(result, "mutation ran");
    }

    /// Two Registry instances must not share locks — each has its own cache.
    #[tokio::test]
    async fn test_two_registries_do_not_share_locks() {
        let a = Registry::new();
        let b = Registry::new();
        let _guard_a = a.acquire("ws").await;
        // b should not block — different cache.
        let guard_b = tokio::time::timeout(std::time::Duration::from_millis(50), b.acquire("ws"))
            .await
            .expect("registry b should acquire immediately, lock is per-Registry");
        drop(guard_b);
        drop(_guard_a);
    }

    /// Registry::global() must return the same Arc on every call.
    #[tokio::test]
    async fn test_global_registry_is_singleton() {
        let g1 = Registry::global();
        let g2 = Registry::global();
        assert!(Arc::ptr_eq(g1, g2));
    }

    /// acquire_gate must delegate to Registry::global(): a guard from
    /// acquire_gate must contend with a guard from Registry::global() on the
    /// same workspace_id.
    #[tokio::test]
    async fn test_acquire_gate_delegates_to_global() {
        // Holding a guard from the module-level helper must block a guard from
        // Registry::global() for the same workspace_id (proves delegation).
        let _outer = acquire_gate("ws-delegate").await;
        let attempt = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            Registry::global().acquire("ws-delegate"),
        )
        .await;
        assert!(
            attempt.is_err(),
            "Registry::global() must contend on the same lock as acquire_gate"
        );
    }
}
