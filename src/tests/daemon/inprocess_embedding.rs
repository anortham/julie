//! Tests for `server_in_process::acquire_in_process_embedding_provider`.
//!
//! ## Gate invariant
//!
//! - In-process embedding acquisition routes through the shared 3b resident
//!   host via `connect_or_spawn_host`.
//! - Readiness is gated on the fallible `ensure_ready()` (NOT silent getters),
//!   so an unhealthy host degrades to `None` rather than false-Ready.
//! - `JULIE_EMBEDDING_PROVIDER=none` force-disables without any I/O.
//! - Host-down / unhealthy does not hang startup; the function returns `None`.
//!
//! ## Coverage note
//!
//! The full "three concurrent sessions share exactly ONE resident model"
//! assertion (multi-session HARD GATE) requires a real Python sidecar and the
//! `embeddings-sidecar` feature; it lives in
//! `src/tests/daemon/embedding_host_multi_session.rs`.  The tests here cover
//! the lighter invariants that can be exercised with a fake `HostListener`:
//!
//! (a) `ensure_ready()` gates Ready-vs-None — an unhealthy host (ready=false)
//!     produces `None`, not a silent provider.
//! (b) A healthy host (ready=true) produces `Some(provider)`.
//! (c) `JULIE_EMBEDDING_PROVIDER=none` returns `None` without any I/O.

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::paths::DaemonPaths;

    // -----------------------------------------------------------------------
    // RAII env guard — restores the previous value on drop.
    // SAFETY: serialised by #[serial] so no concurrent thread mutates the var.
    // -----------------------------------------------------------------------

    struct EnvGuard {
        key: String,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: serialised by #[serial]
            unsafe { std::env::set_var(key, value) };
            Self { key: key.to_owned(), previous }
        }

        fn remove(key: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: serialised by #[serial]
            unsafe { std::env::remove_var(key) };
            Self { key: key.to_owned(), previous }
        }
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
    // Test (c): JULIE_EMBEDDING_PROVIDER=none → None without any I/O.
    //
    // This is the cheapest test — no fake server, no blocking, just an env
    // guard and a direct call.  Verifies the force-disable short-circuit path
    // that mirrors create_embedding_provider()'s "none" check.
    // -----------------------------------------------------------------------

    #[tokio::test]
    #[serial(inprocess_embedding_env)]
    async fn test_provider_none_env_returns_none() {
        let (_dir, paths) = temp_paths();
        let _guard = EnvGuard::set("JULIE_EMBEDDING_PROVIDER", "none");

        let result =
            crate::server_in_process::acquire_in_process_embedding_provider(&paths).await;

        assert!(
            result.is_none(),
            "JULIE_EMBEDDING_PROVIDER=none must return None without I/O"
        );
    }

    // -----------------------------------------------------------------------
    // Test (a) + (b) below are Unix-only because HostListener uses Unix sockets.
    // No `embeddings-sidecar` feature needed — we drive a fake server that
    // speaks the host protocol, not a real Python sidecar.
    // -----------------------------------------------------------------------

    /// Test (a): Fake host reports ready=false → ensure_ready() fails → None.
    ///
    /// This is the ensure_ready HARD GATE test.  Without the gate, a host that
    /// accepts connections but isn't ready would be promoted to
    /// `Arc<dyn EmbeddingProvider>`, silently returning wrong results.
    #[cfg(unix)]
    #[tokio::test]
    #[serial(inprocess_embedding_env)]
    async fn test_unhealthy_host_returns_none() {
        use julie_pipeline::embeddings::host_transport::{HostAddress, HostListener};
        use julie_pipeline::embeddings::{
            HealthResult, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
        };

        let (_dir, paths) = temp_paths();
        let addr = HostAddress::from_paths(&paths);
        let listener = HostListener::bind(&addr).await.expect("bind fake host");

        // Ensure JULIE_EMBEDDING_PROVIDER is not set to "none"
        let _guard = EnvGuard::remove("JULIE_EMBEDDING_PROVIDER");

        // Fake host task: handle is_host_live probe + health with ready=false.
        let server = tokio::spawn(async move {
            // Connection 1: is_host_live probe — client connects then drops.
            let _ = listener.accept().await.expect("accept is_host_live probe");

            // Connection 2: RpcEmbeddingProvider health round-trip (ensure_ready).
            let mut conn = listener.accept().await.expect("accept health conn");
            let line = conn
                .read_line()
                .await
                .expect("read health request")
                .expect("health request line present");
            let req: serde_json::Value =
                serde_json::from_str(&line).expect("parse health request");
            let request_id = req["request_id"].as_str().unwrap_or("1").to_string();

            // Reply with ready=false — host still warming up.
            let resp = serde_json::to_string(&ResponseEnvelope {
                schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
                version: SIDECAR_PROTOCOL_VERSION,
                request_id,
                result: Some(HealthResult {
                    ready: false,
                    dims: None,
                    device: None,
                    runtime: None,
                    model_id: None,
                    resolved_backend: None,
                    accelerated: None,
                    degraded_reason: Some("model still loading".to_string()),
                    capabilities: None,
                    load_policy: None,
                }),
                error: None,
            })
            .expect("serialize health response");
            conn.write_line(&resp).await.expect("write health response");
        });

        let result =
            crate::server_in_process::acquire_in_process_embedding_provider(&paths).await;

        assert!(
            result.is_none(),
            "ensure_ready() failure (ready=false) must degrade to None, \
             not silently produce a provider"
        );
        server.await.expect("fake host server task completed");
    }

    /// Test (b): Fake host reports ready=true → ensure_ready() succeeds → Some.
    ///
    /// Proves the happy path: a responsive, healthy host produces a usable
    /// `Arc<dyn EmbeddingProvider>`.
    #[cfg(unix)]
    #[tokio::test]
    #[serial(inprocess_embedding_env)]
    async fn test_healthy_host_returns_some() {
        use julie_pipeline::embeddings::host_transport::{HostAddress, HostListener};
        use julie_pipeline::embeddings::{
            HealthResult, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
        };

        let (_dir, paths) = temp_paths();
        let addr = HostAddress::from_paths(&paths);
        let listener = HostListener::bind(&addr).await.expect("bind fake host");

        let _guard = EnvGuard::remove("JULIE_EMBEDDING_PROVIDER");

        // Fake host task: is_host_live probe + health with ready=true.
        let server = tokio::spawn(async move {
            // Connection 1: is_host_live probe.
            let _ = listener.accept().await.expect("accept is_host_live probe");

            // Connection 2: health round-trip.
            let mut conn = listener.accept().await.expect("accept health conn");
            let line = conn
                .read_line()
                .await
                .expect("read health request")
                .expect("health request line present");
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

        let result =
            crate::server_in_process::acquire_in_process_embedding_provider(&paths).await;

        assert!(
            result.is_some(),
            "healthy host (ready=true) must produce Some(provider)"
        );
        server.await.expect("fake host server task completed");
    }
}
