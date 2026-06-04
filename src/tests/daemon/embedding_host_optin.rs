//! Tests for the opt-in embedding-host coexistence wiring (Phase 3b, Task 7).
//!
//! Gate invariant: env unset → `create_embedding_provider` path (incl.
//! model-sync); env truthy → host path, `Ready` only after a real health
//! handshake (because the existing `(Some,Some)` arm calls `device_info()`
//! which triggers the RPC client's lazy health round-trip).

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serial_test::serial;

    use crate::daemon::app::spawn_embedding_init;
    use crate::daemon::embedding_service::{EmbeddingService, EmbeddingServiceSettled};
    use crate::daemon::watcher_pool::WatcherPool;
    use crate::paths::DaemonPaths;

    // -----------------------------------------------------------------------
    // EnvGuard — sets/restores an env var on drop.
    // SAFETY: serialised by #[serial] so no concurrent thread touches the var.
    // -----------------------------------------------------------------------

    fn with_env(key: &str, value: &str) -> EnvGuard {
        let previous = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        EnvGuard { key: key.to_owned(), previous }
    }

    fn without_env(key: &str) -> EnvGuard {
        let previous = std::env::var(key).ok();
        unsafe { std::env::remove_var(key) };
        EnvGuard { key: key.to_owned(), previous }
    }

    struct EnvGuard {
        key: String,
        previous: Option<String>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(v) => unsafe { std::env::set_var(&self.key, v) },
                None => unsafe { std::env::remove_var(&self.key) },
            }
        }
    }

    fn temp_paths() -> (tempfile::TempDir, DaemonPaths) {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        (dir, paths)
    }

    // -----------------------------------------------------------------------
    // Test 1: env truthy → host path → Ready (after real health handshake)
    // The fake server answers the health request so device_info() can
    // populate the RPC client's OnceLock — proving the host responds before
    // Ready is published.
    // -----------------------------------------------------------------------

    #[cfg(unix)]
    #[tokio::test]
    #[serial(embedding_host_optin_env)]
    async fn host_path_taken_when_env_set() {
        use julie_pipeline::embeddings::host_transport::{HostAddress, HostListener};
        use julie_pipeline::embeddings::{
            HealthResult, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
        };

        let (_dir, paths) = temp_paths();
        let addr = HostAddress::from_paths(&paths);
        let listener = HostListener::bind(&addr).await.expect("bind listener");

        // Fake host: serve the is_host_live probe (connect+drop) then answer
        // the real health handshake that RpcEmbeddingProvider issues on first
        // use (triggered by device_info() in the (Some,Some) match arm).
        let server = tokio::spawn(async move {
            // Connection 1: is_host_live probe — client connects then drops.
            let _ = listener.accept().await.expect("accept probe");

            // Connection 2: RpcEmbeddingProvider health round-trip.
            let mut conn = listener.accept().await.expect("accept health conn");
            let line = conn
                .read_line()
                .await
                .expect("read health req")
                .expect("health req line");
            let req: serde_json::Value =
                serde_json::from_str(&line).expect("parse health request");
            let request_id = req["request_id"].as_str().unwrap_or("1").to_string();

            let resp = serde_json::to_string(&ResponseEnvelope {
                schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
                version: SIDECAR_PROTOCOL_VERSION,
                request_id,
                result: Some(HealthResult {
                    ready: true,
                    dims: Some(4),
                    device: Some("cpu".to_string()),
                    runtime: Some("fake".to_string()),
                    model_id: Some("fake-model".to_string()),
                    resolved_backend: None,
                    accelerated: Some(false),
                    degraded_reason: None,
                    capabilities: None,
                    load_policy: None,
                }),
                error: None,
            })
            .expect("serialize health response");
            conn.write_line(&resp).await.expect("write health response");
        });

        let _guard_host = with_env("JULIE_EMBEDDING_USE_HOST", "1");

        let svc = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WatcherPool::new(Duration::from_secs(30)));
        let _handle = spawn_embedding_init(Arc::clone(&svc), None, Arc::clone(&pool), paths);

        let outcome = svc.wait_until_settled(Duration::from_secs(5)).await;

        assert!(
            matches!(outcome, EmbeddingServiceSettled::Ready { .. }),
            "expected Ready when JULIE_EMBEDDING_USE_HOST=1 and host answers health",
        );
        server.await.expect("fake host server task");
    }

    // -----------------------------------------------------------------------
    // Test 2: env unset → existing create_embedding_provider path
    // Asserts Unavailable AND the reason is from create_embedding_provider,
    // not the host path (i.e. reason does NOT contain "embedding-host").
    // -----------------------------------------------------------------------

    #[tokio::test]
    #[serial(embedding_host_optin_env)]
    async fn existing_path_taken_when_env_unset() {
        let (_dir, paths) = temp_paths();

        let _guard_host = without_env("JULIE_EMBEDDING_USE_HOST");
        // Disable the sidecar so create_embedding_provider returns quickly
        // with (None, _) instead of spawning a real Python process.
        let _guard_provider = with_env("JULIE_EMBEDDING_PROVIDER", "none");

        let svc = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WatcherPool::new(Duration::from_secs(30)));
        let _handle = spawn_embedding_init(Arc::clone(&svc), None, Arc::clone(&pool), paths);

        let outcome = svc.wait_until_settled(Duration::from_secs(5)).await;

        // Must be Unavailable (provider=none disables embeddings).
        // Reason must NOT mention "embedding-host" — that's the host path's
        // reason string, so its absence positively distinguishes the branch.
        match outcome {
            EmbeddingServiceSettled::Unavailable { reason, .. } => {
                assert!(
                    !reason.contains("embedding-host"),
                    "reason should come from create_embedding_provider, not the host path; \
                     got: {reason:?}",
                );
            }
            _ => panic!(
                "expected Unavailable when JULIE_EMBEDDING_USE_HOST is unset \
                 and JULIE_EMBEDDING_PROVIDER=none, got a different outcome"
            ),
        }
    }
}
