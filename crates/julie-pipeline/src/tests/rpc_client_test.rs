//! Tests for `RpcEmbeddingProvider` (Phase 3b, Task 3).
//!
//! Drive the client against an in-test tokio fake host that speaks the
//! envelope protocol. The fake host reads `RequestEnvelope` lines and replies
//! with deterministic `EmbedQueryResult` / `EmbedBatchResult` / `HealthResult`
//! responses serialised as `ResponseEnvelope` JSON.
//!
//! Invariant proven: `RpcEmbeddingProvider`, talking only over the transport,
//! returns correct embedding vectors from a host and recovers from a dropped
//! connection with exactly one reconnect.

#[cfg(all(test, unix))]
mod unix {
    use julie_core::paths::RegistryPaths;

    use crate::embeddings::host_transport::{HostAddress, HostListener, HostServerConn};
    use crate::embeddings::rpc_client::RpcEmbeddingProvider;
    use crate::embeddings::{
        EmbedBatchResult, EmbedQueryResult, EmbeddingProvider, HealthResult, ResponseEnvelope,
        SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    };

    // -----------------------------------------------------------------------
    // Shared constants
    // -----------------------------------------------------------------------

    const DIMS: usize = 3;
    const VEC: [f32; 3] = [0.1, 0.2, 0.3];

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn temp_address() -> (tempfile::TempDir, HostAddress) {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = RegistryPaths::with_home(dir.path().to_path_buf());
        (dir, HostAddress::from_paths(&paths))
    }

    /// A deterministic health response the fake host returns.
    fn fake_health() -> HealthResult {
        HealthResult {
            ready: true,
            dims: Some(DIMS),
            device: Some("cpu".to_string()),
            runtime: Some("fake-host".to_string()),
            model_id: Some("test-model".to_string()),
            resolved_backend: None,
            accelerated: Some(false),
            degraded_reason: None,
            capabilities: None,
            load_policy: None,
        }
    }

    /// Read exactly one request from `conn`, dispatch by `method`, and write
    /// the appropriate response back. Returns the method name, or `None` if
    /// the client closed the connection before sending a request.
    async fn handle_one(conn: &mut HostServerConn) -> Option<String> {
        let line = conn.read_line().await.expect("server read")?;
        let req: serde_json::Value = serde_json::from_str(&line).expect("parse request");
        let method = req["method"].as_str().expect("method field").to_string();
        let request_id = req["request_id"].as_str().unwrap_or("1").to_string();

        let resp = match method.as_str() {
            "health" => serde_json::to_string(&ResponseEnvelope {
                schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
                version: SIDECAR_PROTOCOL_VERSION,
                request_id,
                result: Some(fake_health()),
                error: None,
            })
            .expect("serialize health"),

            "embed_query" => serde_json::to_string(&ResponseEnvelope {
                schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
                version: SIDECAR_PROTOCOL_VERSION,
                request_id,
                result: Some(EmbedQueryResult {
                    dims: DIMS,
                    vector: VEC.to_vec(),
                }),
                error: None,
            })
            .expect("serialize embed_query"),

            "embed_batch" => {
                let count = req["params"]["texts"].as_array().map_or(0, |a| a.len());
                serde_json::to_string(&ResponseEnvelope {
                    schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
                    version: SIDECAR_PROTOCOL_VERSION,
                    request_id,
                    result: Some(EmbedBatchResult {
                        dims: DIMS,
                        vectors: vec![VEC.to_vec(); count],
                    }),
                    error: None,
                })
                .expect("serialize embed_batch")
            }

            other => panic!("unexpected method from client: {other}"),
        };

        conn.write_line(&resp).await.expect("server write");
        Some(method)
    }

    // -----------------------------------------------------------------------
    // Test 1: happy path — embed_query, embed_batch, dimensions, device_info
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn embed_query_batch_and_cached_info_reflect_health() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept");
            // health — triggered by the lazy connect inside embed_query
            assert_eq!(handle_one(&mut conn).await.as_deref(), Some("health"));
            // embed_query request
            assert_eq!(handle_one(&mut conn).await.as_deref(), Some("embed_query"));
            // embed_batch request (same connection, no reconnect)
            assert_eq!(handle_one(&mut conn).await.as_deref(), Some("embed_batch"));
        });

        let (query_vec, batch_vecs, dims, dev) = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            let query_vec = p.embed_query("hello")?;
            let batch_vecs = p.embed_batch(&["a".to_string(), "b".to_string()])?;
            let dims = p.dimensions();
            let dev = p.device_info();
            Ok::<_, anyhow::Error>((query_vec, batch_vecs, dims, dev))
        })
        .await
        .expect("blocking join")
        .expect("provider calls");

        assert_eq!(query_vec, VEC.to_vec(), "embed_query vector");
        assert_eq!(
            batch_vecs,
            vec![VEC.to_vec(), VEC.to_vec()],
            "embed_batch vectors"
        );
        assert_eq!(dims, DIMS, "dimensions from health cache");
        assert_eq!(dev.runtime, "fake-host", "device_info.runtime");
        assert_eq!(dev.device, "cpu", "device_info.device");
        assert_eq!(dev.model_name, "test-model", "device_info.model_name");
        assert_eq!(dev.dimensions, DIMS, "device_info.dimensions");

        server.await.expect("server task");
    }

    // -----------------------------------------------------------------------
    // Test 2: broken pipe → exactly one reconnect → success
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn broken_pipe_triggers_exactly_one_reconnect_and_succeeds() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            // Connection 1: serve health, then drop conn.
            // The server-side FIN causes the client's embed_query round_trip to
            // return UnexpectedEof, which is classified as a connection drop.
            {
                let mut conn = listener.accept().await.expect("accept conn1");
                assert_eq!(handle_one(&mut conn).await.as_deref(), Some("health"));
                // conn drops here → FIN sent → client sees EOF on next read
            }

            // Connection 2: the client reconnects; serve health + embed_query.
            {
                let mut conn = listener.accept().await.expect("accept conn2");
                assert_eq!(handle_one(&mut conn).await.as_deref(), Some("health"));
                assert_eq!(handle_one(&mut conn).await.as_deref(), Some("embed_query"));
            }
        });

        let vector = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            // This call must survive the broken connection with one reconnect.
            p.embed_query("reconnect-test")
        })
        .await
        .expect("blocking join")
        .expect("embed_query after reconnect");

        assert_eq!(vector, VEC.to_vec(), "vector after reconnect");

        server.await.expect("server task");
    }
}
