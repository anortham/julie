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
    use std::time::Duration;

    use julie_core::paths::RegistryPaths;

    use crate::embeddings::host_transport::{HostAddress, HostListener, HostServerConn};
    use crate::embeddings::rpc_client::RpcEmbeddingProvider;
    use crate::embeddings::{
        EmbedBatchResult, EmbedQueryResult, EmbeddingProvider, EmbeddingRequestBudget,
        EncoderIdentity, HealthResult, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA,
        SIDECAR_PROTOCOL_VERSION,
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
            ..Default::default()
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
            let budget = EmbeddingRequestBudget::default();
            let query_vec = p.embed_query("hello", &budget)?;
            let batch_vecs = p.embed_batch(&["a".to_string(), "b".to_string()], &budget)?;
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
            p.embed_query("reconnect-test", &EmbeddingRequestBudget::default())
        })
        .await
        .expect("blocking join")
        .expect("embed_query after reconnect");

        assert_eq!(vector, VEC.to_vec(), "vector after reconnect");

        server.await.expect("server task");
    }

    // -----------------------------------------------------------------------
    // Empirical Challenge Tests: Finding 4 (Provenance & Reconnect Integrity)
    // -----------------------------------------------------------------------

    fn make_test_health(
        model_id: &str,
        dims: usize,
        sha256: Option<&str>,
        pooling: Option<&str>,
        norm: Option<&str>,
        policy: Option<u64>,
    ) -> HealthResult {
        HealthResult {
            ready: true,
            dims: Some(dims),
            device: Some("cpu".to_string()),
            runtime: Some("fake-host".to_string()),
            model_id: Some(model_id.to_string()),
            model_sha256: sha256.map(|s| s.to_string()),
            pooling: pooling.map(|s| s.to_string()),
            normalization: norm.map(|s| s.to_string()),
            instruction_policy_version: policy,
            ..Default::default()
        }
    }

    async fn handle_one_custom(conn: &mut HostServerConn, health: HealthResult) -> Option<String> {
        let line = conn.read_line().await.expect("server read")?;
        let req: serde_json::Value = serde_json::from_str(&line).expect("parse request");
        let method = req["method"].as_str().expect("method field").to_string();
        let request_id = req["request_id"].as_str().unwrap_or("1").to_string();

        let resp = match method.as_str() {
            "health" => serde_json::to_string(&ResponseEnvelope {
                schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
                version: SIDECAR_PROTOCOL_VERSION,
                request_id,
                result: Some(health),
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

            other => panic!("unexpected method from client: {other}"),
        };

        conn.write_line(&resp).await.expect("server write");
        Some(method)
    }

    async fn test_reconnect_health_swap(
        initial: HealthResult,
        replacement: HealthResult,
    ) -> anyhow::Result<Vec<f32>> {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            // Connection 1: serve initial health, then close connection
            {
                let mut conn = listener.accept().await.expect("accept conn1");
                assert_eq!(
                    handle_one_custom(&mut conn, initial).await.as_deref(),
                    Some("health")
                );
            }

            // Connection 2: serve replacement health; if accepted, client requests embed_query
            {
                let mut conn = listener.accept().await.expect("accept conn2");
                assert_eq!(
                    handle_one_custom(&mut conn, replacement).await.as_deref(),
                    Some("health")
                );
                let _ = handle_one_custom(&mut conn, fake_health()).await;
            }
        });

        let res = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            p.embed_query("reconnect-test", &EmbeddingRequestBudget::default())
        })
        .await
        .expect("blocking join");

        let _ = server.await;
        res
    }

    const VALID_SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const ALT_SHA: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    #[tokio::test]
    async fn challenge_reconnect_rejects_altered_model_id() {
        let initial = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let altered = make_test_health(
            "model-v2",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let err = test_reconnect_health_swap(initial, altered)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("reconnect health mismatch"),
            "expected reconnect health mismatch error, got: {err}"
        );
        assert!(
            err.contains("model="),
            "expected model mismatch detail, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_reconnect_rejects_altered_dims() {
        let initial = make_test_health(
            "model-v1",
            3,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let altered = make_test_health(
            "model-v1",
            4,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let err = test_reconnect_health_swap(initial, altered)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("reconnect health mismatch"),
            "expected reconnect health mismatch error, got: {err}"
        );
        assert!(
            err.contains("dims="),
            "expected dims mismatch detail, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_reconnect_rejects_altered_sha256() {
        let initial = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let altered = make_test_health(
            "model-v1",
            DIMS,
            Some(ALT_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let err = test_reconnect_health_swap(initial, altered)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("reconnect health mismatch"),
            "expected reconnect health mismatch error, got: {err}"
        );
        assert!(
            err.contains("sha256="),
            "expected sha256 mismatch detail, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_reconnect_rejects_altered_pooling() {
        let initial = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let altered = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("cls"),
            Some("l2"),
            Some(1),
        );
        let err = test_reconnect_health_swap(initial, altered)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("reconnect health mismatch"),
            "expected reconnect health mismatch error, got: {err}"
        );
        assert!(
            err.contains("pooling="),
            "expected pooling mismatch detail, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_reconnect_rejects_altered_normalization() {
        let initial = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let altered = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("none"),
            Some(1),
        );
        let err = test_reconnect_health_swap(initial, altered)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("reconnect health mismatch"),
            "expected reconnect health mismatch error, got: {err}"
        );
        assert!(
            err.contains("norm="),
            "expected normalization mismatch detail, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_reconnect_rejects_altered_instruction_policy() {
        let initial = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let altered = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(2),
        );
        let err = test_reconnect_health_swap(initial, altered)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("reconnect health mismatch"),
            "expected reconnect health mismatch error, got: {err}"
        );
        assert!(
            err.contains("policy="),
            "expected policy mismatch detail, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_reconnect_succeeds_when_health_matches_exactly() {
        let initial = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let identical = make_test_health(
            "model-v1",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let vec = test_reconnect_health_swap(initial, identical)
            .await
            .expect("reconnect should succeed");
        assert_eq!(vec, VEC.to_vec());
    }

    // -----------------------------------------------------------------------
    // Provenance Integrity Challenges (no defaults fabricated)
    // -----------------------------------------------------------------------

    async fn test_rpc_encoder_identity(health: HealthResult) -> anyhow::Result<EncoderIdentity> {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept");
            assert_eq!(
                handle_one_custom(&mut conn, health).await.as_deref(),
                Some("health")
            );
        });

        let res = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            p.encoder_identity()
        })
        .await
        .expect("blocking join");

        let _ = server.await;
        res
    }

    #[tokio::test]
    async fn challenge_provenance_missing_pooling_fails_without_fabrication() {
        let h = make_test_health(
            "test-model",
            DIMS,
            Some(VALID_SHA),
            None,
            Some("l2"),
            Some(1),
        );
        let err = test_rpc_encoder_identity(h).await.unwrap_err().to_string();
        assert!(
            err.contains("IdentityUnavailable: missing pooling"),
            "expected missing pooling error, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_provenance_empty_pooling_fails_without_fabrication() {
        let h = make_test_health(
            "test-model",
            DIMS,
            Some(VALID_SHA),
            Some("   "),
            Some("l2"),
            Some(1),
        );
        let err = test_rpc_encoder_identity(h).await.unwrap_err().to_string();
        assert!(
            err.contains("IdentityUnavailable: missing pooling"),
            "expected missing pooling error, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_provenance_missing_normalization_fails_without_fabrication() {
        let h = make_test_health(
            "test-model",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            None,
            Some(1),
        );
        let err = test_rpc_encoder_identity(h).await.unwrap_err().to_string();
        assert!(
            err.contains("IdentityUnavailable: missing normalization"),
            "expected missing normalization error, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_provenance_empty_normalization_fails_without_fabrication() {
        let h = make_test_health(
            "test-model",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some(""),
            Some(1),
        );
        let err = test_rpc_encoder_identity(h).await.unwrap_err().to_string();
        assert!(
            err.contains("IdentityUnavailable: missing normalization"),
            "expected missing normalization error, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_provenance_missing_instruction_policy_fails_without_fabrication() {
        let h = make_test_health(
            "test-model",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            None,
        );
        let err = test_rpc_encoder_identity(h).await.unwrap_err().to_string();
        assert!(
            err.contains("IdentityUnavailable: missing instruction_policy_version"),
            "expected missing instruction_policy_version error, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_provenance_missing_sha256_fails_without_fabrication() {
        let h = make_test_health("test-model", DIMS, None, Some("mean"), Some("l2"), Some(1));
        let err = test_rpc_encoder_identity(h).await.unwrap_err().to_string();
        assert!(
            err.contains("IdentityUnavailable"),
            "expected IdentityUnavailable error, got: {err}"
        );
    }

    #[tokio::test]
    async fn challenge_provenance_valid_identity_succeeds() {
        let h = make_test_health(
            "test-model",
            DIMS,
            Some(VALID_SHA),
            Some("mean"),
            Some("l2"),
            Some(1),
        );
        let id = test_rpc_encoder_identity(h)
            .await
            .expect("should produce valid EncoderIdentity");
        assert_eq!(id.model_id, "test-model");
        assert_eq!(id.dimensions, DIMS);
        assert_eq!(id.pooling, "mean");
        assert_eq!(id.normalization, "l2");
        assert_eq!(id.instruction_policy, "v1");
        assert_eq!(id.weights_sha256, VALID_SHA);
    }

    // -----------------------------------------------------------------------
    // Empirical Challenge Tests: Finding 7 (Timeout Clamping & Deadlines)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn challenge_request_timeout_clamped_to_short_budget() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept");
            assert_eq!(handle_one(&mut conn).await.as_deref(), Some("health"));
            let line = conn.read_line().await.expect("server read").expect("line");
            let req: serde_json::Value = serde_json::from_str(&line).expect("parse request");
            assert_eq!(req["method"], "embed_query");
            // Deliberately do not reply; sleep for 2000ms to simulate a hanging host
            tokio::time::sleep(Duration::from_millis(2000)).await;
        });

        let start = std::time::Instant::now();
        let res = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            // Budget is only 60ms, which must clamp the 120-second default timeout
            let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(60));
            p.embed_query("timeout-test", &budget)
        })
        .await
        .expect("blocking join");

        let elapsed = start.elapsed();
        let err = res.unwrap_err().to_string();

        assert!(
            err.contains("embed_query io error") || err.contains("deadline exceeded"),
            "expected io error or deadline exceeded, got: {err}"
        );
        // Clamping ensures it timed out within ~60ms, far below the 2000ms server sleep (and 120s default)
        assert!(
            elapsed < Duration::from_millis(500),
            "timeout took {elapsed:?}, expected < 500ms (budget was 60ms, default is 120s)"
        );
        assert!(
            elapsed >= Duration::from_millis(40),
            "timeout returned prematurely in {elapsed:?}"
        );

        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn challenge_reconnect_deadline_clamped_on_hanging_reconnect() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            // Connection 1: serve health, then drop
            {
                let mut conn = listener.accept().await.expect("accept conn1");
                assert_eq!(handle_one(&mut conn).await.as_deref(), Some("health"));
            }

            // Connection 2: accept reconnect, but hang on health handshake
            {
                let mut conn = listener.accept().await.expect("accept conn2");
                let _line = conn.read_line().await.expect("server read").expect("line");
                // Do NOT reply; sleep for 2000ms
                tokio::time::sleep(Duration::from_millis(2000)).await;
            }
        });

        let start = std::time::Instant::now();
        let res = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(120));
            p.embed_query("reconnect-timeout-test", &budget)
        })
        .await
        .expect("blocking join");

        let elapsed = start.elapsed();
        let err = res.unwrap_err().to_string();

        assert!(
            err.contains("embedding host reconnect") || err.contains("deadline exceeded"),
            "expected reconnect timeout or deadline exceeded, got: {err}"
        );
        assert!(
            elapsed < Duration::from_millis(700),
            "reconnect timeout took {elapsed:?}, expected < 700ms (budget was 120ms, default is 120s)"
        );
        assert!(
            elapsed >= Duration::from_millis(80),
            "reconnect timeout returned too early in {elapsed:?}"
        );

        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn challenge_reconnect_deadline_clamped_on_hanging_request_after_reconnect() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            // Connection 1: serve health, then drop
            {
                let mut conn = listener.accept().await.expect("accept conn1");
                assert_eq!(handle_one(&mut conn).await.as_deref(), Some("health"));
            }

            // Connection 2: accept reconnect, answer health promptly, but hang on embed_query
            {
                let mut conn = listener.accept().await.expect("accept conn2");
                assert_eq!(handle_one(&mut conn).await.as_deref(), Some("health"));
                let line = conn.read_line().await.expect("server read").expect("line");
                let req: serde_json::Value = serde_json::from_str(&line).expect("parse request");
                assert_eq!(req["method"], "embed_query");
                tokio::time::sleep(Duration::from_millis(2000)).await;
            }
        });

        let start = std::time::Instant::now();
        let res = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(150));
            p.embed_query("reconnect-request-hang-test", &budget)
        })
        .await
        .expect("blocking join");

        let elapsed = start.elapsed();
        let err = res.unwrap_err().to_string();

        assert!(
            err.contains("embed_query failed after reconnect")
                || err.contains("embed_query io error")
                || err.contains("deadline exceeded"),
            "expected error after reconnect, got: {err}"
        );
        assert!(
            elapsed < Duration::from_millis(800),
            "reconnect request timeout took {elapsed:?}, expected < 800ms (budget was 150ms, default is 120s)"
        );

        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn challenge_reconnect_bails_immediately_when_budget_cancelled_before_reconnect() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let cancelled_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag_clone = cancelled_flag.clone();

        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept conn1");
            assert_eq!(handle_one(&mut conn).await.as_deref(), Some("health"));
            // Read embed_query request, cancel budget, then drop connection
            let line = conn.read_line().await.expect("server read").expect("line");
            let req: serde_json::Value = serde_json::from_str(&line).expect("parse request");
            assert_eq!(req["method"], "embed_query");
            flag_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            drop(conn);
        });

        let res = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            let budget = EmbeddingRequestBudget::new(
                std::time::Instant::now() + Duration::from_secs(10),
                cancelled_flag,
            );
            p.embed_query("cancelled-test", &budget)
        })
        .await
        .expect("blocking join");

        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("embedding request cancelled"),
            "expected 'embedding request cancelled', got: {err}"
        );

        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn challenge_request_bails_when_budget_expires_during_connect() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept conn1");
            let line = conn
                .read_line()
                .await
                .expect("read health req")
                .expect("line");
            let req: serde_json::Value = serde_json::from_str(&line).expect("parse req");
            assert_eq!(req["method"], "health");
            // Delay 40ms before answering health to consume client's 30ms budget
            tokio::time::sleep(Duration::from_millis(40)).await;
            let resp = serde_json::to_string(&ResponseEnvelope {
                schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
                version: SIDECAR_PROTOCOL_VERSION,
                request_id: req["request_id"].as_str().unwrap_or("1").to_string(),
                result: Some(fake_health()),
                error: None,
            })
            .expect("serialize health");
            conn.write_line(&resp).await.expect("write health");
        });

        let res = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            // Budget 30ms, health handshake takes 40ms so remaining is zero after connect
            let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(30));
            p.embed_query("budget-expired-during-connect", &budget)
        })
        .await
        .expect("blocking join");

        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("deadline exceeded")
                || err.contains("Resource temporarily unavailable")
                || err.contains("os error 11"),
            "expected deadline or connect timeout error, got: {err}"
        );

        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn challenge_request_bails_immediately_on_expired_budget_without_io() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            // Should never accept any connection
            let _ = listener.accept().await;
        });

        let res = tokio::task::spawn_blocking(move || {
            let p = RpcEmbeddingProvider::new(addr);
            // Already expired budget
            let budget = EmbeddingRequestBudget::with_timeout(Duration::ZERO);
            p.embed_query("zero-budget-test", &budget)
        })
        .await
        .expect("blocking join");

        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("deadline exceeded"),
            "expected deadline exceeded error, got: {err}"
        );

        server.abort();
        let _ = server.await;
    }
}
