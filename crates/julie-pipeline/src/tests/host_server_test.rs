//! Tests for the embedding-host server (Phase 3b, Task 4).
//!
//! Uses an injected `FakeProvider` (deterministic, no Python/torch) to verify:
//! - `health` / `embed_query` / `embed_batch` round-trip correctly.
//! - Two concurrent client connections both complete successfully.
//! - `cancel.cancel()` makes `run_embedding_host` return and releases the
//!   socket file and the singleton lock.

#[cfg(all(test, unix))]
mod unix {
    use std::sync::Arc;

    use julie_core::{
        embeddings_contract::{DeviceInfo, EmbeddingProvider},
        paths::RegistryPaths,
    };
    use tokio_util::sync::CancellationToken;

    use crate::embeddings::{
        host_server::run_embedding_host,
        host_transport::{HostAddress, HostClientConn, HostListener},
        sidecar_protocol::{
            EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult,
            RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
        },
    };

    // -----------------------------------------------------------------------
    // Fake provider — deterministic vectors, no real ML
    // -----------------------------------------------------------------------

    const FAKE_DIMS: usize = 4;

    struct FakeProvider;

    impl EmbeddingProvider for FakeProvider {
        fn embed_query(&self, text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![text.len() as f32; FAKE_DIMS])
        }

        fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(texts
                .iter()
                .map(|t| vec![t.len() as f32; FAKE_DIMS])
                .collect())
        }

        fn dimensions(&self) -> usize {
            FAKE_DIMS
        }

        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "fake".to_string(),
                device: "cpu".to_string(),
                model_name: "fake-model".to_string(),
                dimensions: FAKE_DIMS,
            }
        }
        // shutdown / wait_for_exit use default no-op implementations.
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Serialize a `RequestEnvelope` to a single JSON line ready for round-trip.
    fn request_line<T: serde::Serialize>(method: &str, id: &str, params: T) -> String {
        let env = RequestEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: id.to_string(),
            method: method.to_string(),
            params,
        };
        serde_json::to_string(&env).expect("serialize request")
    }

    /// Spawn the host server and wait briefly for the listener to bind.
    ///
    /// Returns `(cancel_token, join_handle, Arc<HostAddress>)`.  The `Arc`
    /// lets multiple blocking-client tasks share the address without cloning
    /// the raw `HostAddress` (which is not `Clone`).
    async fn spawn_host(
        tmp_root: std::path::PathBuf,
        lock_path: std::path::PathBuf,
    ) -> (
        CancellationToken,
        tokio::task::JoinHandle<anyhow::Result<()>>,
        Arc<HostAddress>,
    ) {
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(FakeProvider);
        let cancel = CancellationToken::new();

        // Build the address from the tmp root — we need to do it here so we
        // can return an Arc, and again inside the spawn because HostAddress is
        // not Clone.
        let paths = RegistryPaths::with_home(tmp_root.clone());
        let addr = Arc::new(HostAddress::from_paths(&paths));

        let addr_inner = Arc::clone(&addr);
        let cancel_inner = cancel.clone();
        let handle = tokio::spawn(async move {
            run_embedding_host(&addr_inner, &lock_path, cancel_inner, provider).await
        });

        // Give the listener a moment to bind before clients try to connect.
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        (cancel, handle, addr)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// health / embed_query / embed_batch all return the correct results over
    /// the real IPC transport with an injected FakeProvider.
    #[tokio::test]
    async fn health_embed_query_embed_batch_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("embedding-host.lock");
        let (cancel, handle, addr) = spawn_host(dir.path().to_path_buf(), lock_path).await;

        let addr_c = Arc::clone(&addr);
        let (health_line, query_line, batch_line) = tokio::task::spawn_blocking(move || {
            let mut conn = HostClientConn::connect(&addr_c).expect("connect");

            let h = conn
                .round_trip(&request_line("health", "h1", serde_json::json!({})))
                .expect("health round_trip");

            let q = conn
                .round_trip(&request_line(
                    "embed_query",
                    "q1",
                    EmbedQueryRequest {
                        text: "hello".to_string(),
                    }, // len=5
                ))
                .expect("embed_query round_trip");

            let b = conn
                .round_trip(&request_line(
                    "embed_batch",
                    "b1",
                    EmbedBatchRequest {
                        texts: vec!["hi".to_string(), "there".to_string()], // len 2,5
                    },
                ))
                .expect("embed_batch round_trip");

            (h, q, b)
        })
        .await
        .expect("spawn_blocking");

        // --- health ---
        let hr: ResponseEnvelope<HealthResult> =
            serde_json::from_str(&health_line).expect("parse health");
        assert!(hr.error.is_none(), "health must not carry an error");
        let hr = hr.result.expect("health result present");
        assert!(hr.ready, "health.ready");
        assert_eq!(hr.dims, Some(FAKE_DIMS), "health.dims");
        assert_eq!(hr.device.as_deref(), Some("cpu"), "health.device");
        assert_eq!(hr.runtime.as_deref(), Some("fake"), "health.runtime");
        assert_eq!(
            hr.model_id.as_deref(),
            Some("fake-model"),
            "health.model_id"
        );

        // --- embed_query: "hello" len=5 → [5.0; 4] ---
        let qr: ResponseEnvelope<EmbedQueryResult> =
            serde_json::from_str(&query_line).expect("parse query");
        assert!(qr.error.is_none(), "embed_query must not carry an error");
        let qr = qr.result.expect("query result present");
        assert_eq!(qr.dims, FAKE_DIMS, "query dims");
        assert_eq!(qr.vector, vec![5.0f32; FAKE_DIMS], "query vector");

        // --- embed_batch: ["hi"(2), "there"(5)] ---
        let br: ResponseEnvelope<EmbedBatchResult> =
            serde_json::from_str(&batch_line).expect("parse batch");
        assert!(br.error.is_none(), "embed_batch must not carry an error");
        let br = br.result.expect("batch result present");
        assert_eq!(br.dims, FAKE_DIMS, "batch dims");
        assert_eq!(br.vectors.len(), 2, "batch count");
        assert_eq!(br.vectors[0], vec![2.0f32; FAKE_DIMS], "batch[0] hi");
        assert_eq!(br.vectors[1], vec![5.0f32; FAKE_DIMS], "batch[1] there");

        cancel.cancel();
        handle.await.expect("server join").expect("server ok");
    }

    /// Two concurrent blocking connections both receive correct embed_query
    /// results from the single shared FakeProvider.
    #[tokio::test]
    async fn two_concurrent_connections_both_succeed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("embedding-host.lock");
        let (cancel, handle, addr) = spawn_host(dir.path().to_path_buf(), lock_path).await;

        let addr1 = Arc::clone(&addr);
        let addr2 = Arc::clone(&addr);

        // Each task connects independently and sends a different text.
        let t1 = tokio::task::spawn_blocking(move || {
            let mut conn = HostClientConn::connect(&addr1).expect("connect c1");
            conn.round_trip(&request_line(
                "embed_query",
                "c1",
                EmbedQueryRequest {
                    text: "abcde".to_string(),
                }, // len=5
            ))
            .expect("c1 round_trip")
        });
        let t2 = tokio::task::spawn_blocking(move || {
            let mut conn = HostClientConn::connect(&addr2).expect("connect c2");
            conn.round_trip(&request_line(
                "embed_query",
                "c2",
                EmbedQueryRequest {
                    text: "ab".to_string(),
                }, // len=2
            ))
            .expect("c2 round_trip")
        });

        let (r1, r2) = tokio::join!(t1, t2);
        let r1: ResponseEnvelope<EmbedQueryResult> =
            serde_json::from_str(&r1.expect("t1")).expect("parse c1");
        let r2: ResponseEnvelope<EmbedQueryResult> =
            serde_json::from_str(&r2.expect("t2")).expect("parse c2");

        assert_eq!(
            r1.result.expect("c1 result").vector,
            vec![5.0f32; FAKE_DIMS]
        );
        assert_eq!(
            r2.result.expect("c2 result").vector,
            vec![2.0f32; FAKE_DIMS]
        );

        cancel.cancel();
        handle.await.expect("join").expect("server ok");
    }

    /// After cancellation the server returns `Ok(())`, the socket file is
    /// removed, and the singleton lock is released (can be re-acquired).
    #[tokio::test]
    async fn cancel_releases_socket_and_singleton_lock() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("embedding-host.lock");
        let tmp_root = dir.path().to_path_buf();

        let paths = RegistryPaths::with_home(tmp_root.clone());
        let addr_ref = HostAddress::from_paths(&paths);
        let socket_path = addr_ref.socket_path().to_path_buf();

        let (cancel, handle, _addr) = spawn_host(tmp_root.clone(), lock_path.clone()).await;

        assert!(
            socket_path.exists(),
            "socket should exist while server is running"
        );

        cancel.cancel();
        handle
            .await
            .expect("server join")
            .expect("server ok after cancel");

        // Socket file removed by HostListener's Drop impl.
        assert!(
            !socket_path.exists(),
            "socket file should be removed after shutdown"
        );

        // Singleton lock is released — we can acquire it from this process.
        let lf = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&lock_path)
            .expect("open lock file after shutdown");
        use fs2::FileExt as _;
        assert!(
            lf.try_lock_exclusive().is_ok(),
            "singleton lock must be acquirable after shutdown"
        );

        // Listener can be re-bound on the same address.
        let paths2 = RegistryPaths::with_home(tmp_root);
        let addr2 = HostAddress::from_paths(&paths2);
        HostListener::bind(&addr2)
            .await
            .expect("re-bind after shutdown must succeed");
    }

    /// An unrecognised method returns an error envelope with code = "unknown_method".
    #[tokio::test]
    async fn unknown_method_returns_error_envelope() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock_path = dir.path().join("embedding-host.lock");
        let (cancel, handle, addr) = spawn_host(dir.path().to_path_buf(), lock_path).await;

        let addr_c = Arc::clone(&addr);
        let reply = tokio::task::spawn_blocking(move || {
            let mut conn = HostClientConn::connect(&addr_c).expect("connect");
            conn.round_trip(&request_line(
                "not_a_real_method",
                "e1",
                serde_json::json!({}),
            ))
            .expect("round_trip")
        })
        .await
        .expect("spawn_blocking");

        let env: ResponseEnvelope<serde_json::Value> =
            serde_json::from_str(&reply).expect("parse error envelope");
        assert!(
            env.result.is_none(),
            "result must be None for unknown method"
        );
        let err = env.error.expect("error field must be set");
        assert_eq!(err.code, "unknown_method");

        cancel.cancel();
        handle.await.expect("join").expect("ok");
    }
}
