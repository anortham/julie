//! Test helpers for julie-runtime test suites.

use julie_core::workspace::leader_lock::DaemonLockGuard;
use julie_core::workspace::mutation_gate::Registry as MutationGateRegistry;
use julie_core::workspace::ownership::OwnerEpoch;
pub use julie_core::workspace::ownership::WriterPermit;
use julie_core::workspace::registry::generate_workspace_id;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_PERMIT_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Scope wrapper providing an authentic `OwnerEpoch` and `WriterPermit`s for test execution.
pub struct TestWriterScope {
    pub epoch: Arc<OwnerEpoch>,
}

impl TestWriterScope {
    /// Create a new test writer scope for the given workspace root.
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = workspace_root.as_ref();
        let _ = std::fs::create_dir_all(root);
        let workspace_key = root.to_string_lossy();
        let workspace_id =
            generate_workspace_id(&workspace_key).unwrap_or_else(|_| workspace_key.into_owned());

        let counter = TEST_PERMIT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let lock_path = root.join(format!(
            ".test_owner_epoch_{}_{}.lock",
            std::process::id(),
            counter
        ));

        let lock_guard = DaemonLockGuard::try_acquire(&lock_path).unwrap_or_else(|_| {
            let fallback = std::env::temp_dir().join(format!(
                "julie_test_epoch_fallback_{}_{}_{}.lock",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                counter
            ));
            DaemonLockGuard::try_acquire(&fallback)
                .expect("failed to acquire fallback test daemon lock guard")
        });

        let epoch = Arc::new(OwnerEpoch::new(1, workspace_id, lock_guard));
        Self { epoch }
    }

    /// Acquire an authentic `WriterPermit` from this scope's epoch.
    pub async fn permit(&self) -> WriterPermit<'static> {
        self.epoch
            .acquire_writer_shared(MutationGateRegistry::global())
            .await
            .expect("test owner epoch must acquire writer permit")
    }
}

/// Acquire an authentic `WriterPermit` for test execution.
///
/// Sets up an isolated test `OwnerEpoch` backed by a real `DaemonLockGuard`
/// located within the test's `workspace_root`, and acquires a permit against
/// the global mutation gate registry.
pub async fn test_writer_permit(workspace_root: impl AsRef<Path>) -> WriterPermit<'static> {
    TestWriterScope::new(workspace_root).permit().await
}
