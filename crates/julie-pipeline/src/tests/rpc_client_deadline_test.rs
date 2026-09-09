//! Trickling health handshake deadline tests for `RpcEmbeddingProvider`.

#[cfg(all(test, unix))]
mod unix {
    use std::time::{Duration, Instant};

    use julie_core::embeddings_contract::{EmbeddingProvider, EmbeddingRequestBudget};
    use julie_core::paths::RegistryPaths;

    use crate::embeddings::host_transport::{HostAddress, HostListener};
    use crate::embeddings::rpc_client::RpcEmbeddingProvider;

    fn temp_address() -> (tempfile::TempDir, HostAddress) {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = RegistryPaths::with_home(dir.path().to_path_buf());
        (dir, HostAddress::from_paths(&paths))
    }

    #[tokio::test]
    async fn challenge_health_handshake_exceeding_deadline_with_trickling_response_fails() {
        let (_dir, addr) = temp_address();
        let listener = HostListener::bind(&addr).await.expect("bind");

        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept");
            let line = conn
                .read_line()
                .await
                .expect("read line")
                .expect("has line");
            assert!(line.contains("health"));

            let health_json = "{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"rpc-1\",\"result\":{\"ready\":true,\"dims\":3,\"device\":\"cpu\",\"runtime\":\"fake\",\"model_id\":\"m1\",\"llama_cpp_build\":\"b1\"},\"error\":null}\n";

            // Trickle bytes slowly: 1 byte every 20ms.
            // A socket read timeout would only trigger if an individual read exceeds the socket timeout.
            // An overall deadline check must abort because total response time exceeds the budget deadline (50ms).
            for b in health_json.as_bytes() {
                tokio::time::sleep(Duration::from_millis(20)).await;
                if conn.write_raw(&[*b]).await.is_err() {
                    break;
                }
            }
        });

        let client_addr = addr.clone();
        let client_res = tokio::task::spawn_blocking(move || {
            let provider = RpcEmbeddingProvider::new(client_addr);
            // Tight deadline of 50ms
            let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(50));
            let start = Instant::now();
            let res = provider.embed_query("test query", &budget);
            let elapsed = start.elapsed();
            (res, elapsed)
        })
        .await
        .expect("join client");

        let (result, elapsed) = client_res;

        assert!(
            result.is_err(),
            "Health handshake trickling past the deadline must fail"
        );
        let err_msg = result.unwrap_err().to_string();
        let err_lower = err_msg.to_lowercase();
        assert!(
            err_lower.contains("deadline exceeded")
                || err_lower.contains("timedout")
                || err_lower.contains("timed out")
                || err_lower.contains("resource temporarily unavailable")
                || err_lower.contains("operation would block"),
            "Error message must indicate deadline/timeout, got: {err_msg}"
        );
        assert!(
            elapsed < Duration::from_millis(1500),
            "Handshake should fail close to deadline rather than waiting for full trickling response, elapsed: {elapsed:?}"
        );

        server.abort();
    }
}
