//! Tests for the opt-in embedding-host coexistence wiring (Phase 3b, Task 7).
//!
//! Verifies that `spawn_embedding_init` takes the host path when
//! `JULIE_EMBEDDING_USE_HOST` is truthy, and falls through to the existing
//! `create_embedding_provider` path when the env var is absent.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use julie_pipeline::embeddings::host_transport::{HostAddress, HostListener};

    use crate::daemon::app::spawn_embedding_init;
    use crate::daemon::embedding_service::{EmbeddingService, EmbeddingServiceSettled};
    use crate::daemon::watcher_pool::WatcherPool;
    use crate::paths::DaemonPaths;

    // Serialize env-var mutation so the two tests cannot clobber each other
    // when the test runner executes them on the same thread pool.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn temp_paths() -> (tempfile::TempDir, DaemonPaths) {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        (dir, paths)
    }

    // -----------------------------------------------------------------------
    // Test 1: JULIE_EMBEDDING_USE_HOST=1  →  publish_ready (host path)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn host_path_taken_when_env_set() {
        let (_dir, paths) = temp_paths();
        let addr = HostAddress::from_paths(&paths);

        // Bind the listener so is_host_live() inside connect_or_spawn_host
        // succeeds: it does a plain connect() which only needs the socket to
        // be accepting connections.
        let _listener = HostListener::bind(&addr).await.expect("bind listener");

        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { std::env::set_var("JULIE_EMBEDDING_USE_HOST", "1") };

        let svc = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WatcherPool::new(Duration::from_secs(30)));

        let _handle =
            spawn_embedding_init(Arc::clone(&svc), None, Arc::clone(&pool), paths);

        let outcome = svc.wait_until_settled(Duration::from_secs(5)).await;

        // Clean up before the assertion so the env var is always removed.
        unsafe { std::env::remove_var("JULIE_EMBEDDING_USE_HOST") };

        assert!(
            matches!(outcome, EmbeddingServiceSettled::Ready { .. }),
            "expected Ready when JULIE_EMBEDDING_USE_HOST=1",
        );
    }

    // -----------------------------------------------------------------------
    // Test 2: JULIE_EMBEDDING_USE_HOST unset  →  existing create_embedding_provider path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn existing_path_taken_when_env_unset() {
        let (_dir, paths) = temp_paths();

        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            // Ensure the host opt-in is NOT set.
            std::env::remove_var("JULIE_EMBEDDING_USE_HOST");
            // Disable the sidecar so create_embedding_provider returns quickly
            // with (None, _) instead of trying to spawn a real Python process.
            std::env::set_var("JULIE_EMBEDDING_PROVIDER", "none");
        }

        let svc = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WatcherPool::new(Duration::from_secs(30)));

        let _handle =
            spawn_embedding_init(Arc::clone(&svc), None, Arc::clone(&pool), paths);

        let outcome = svc.wait_until_settled(Duration::from_secs(5)).await;

        unsafe { std::env::remove_var("JULIE_EMBEDDING_PROVIDER") };

        // create_embedding_provider with JULIE_EMBEDDING_PROVIDER=none publishes
        // Unavailable, confirming the host branch was NOT entered.
        assert!(
            matches!(outcome, EmbeddingServiceSettled::Unavailable { .. }),
            "expected Unavailable when JULIE_EMBEDDING_USE_HOST is unset and provider=none",
        );
    }
}
