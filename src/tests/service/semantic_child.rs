use super::http_api::Running;
use serde_json::json;

#[cfg(unix)]
#[tokio::test]
async fn two_checkouts_share_one_embedding_child() {
    let fake_dir = tempfile::tempdir().unwrap();
    let fake = crate::tests::helpers::fake_sidecar::write(fake_dir.path());
    let mut env = crate::tests::helpers::env::EnvVarGuard::new();
    env.set("JULIE_NATIVE_SIDECAR_PROGRAM", fake.as_os_str());
    env.set("JULIE_NATIVE_SIDECAR_MODEL", "fake");
    env.set("JULIE_EMBEDDING_PROVIDER", "native");
    let running = Running::start(None).await;
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    for dir in [&a, &b] {
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join(".git"), "gitdir: nowhere\n").unwrap();
        std::fs::write(root.join("lib.rs"), "pub fn shared_child_probe() {}\n").unwrap();
        let response = running
            .api(
                "manage_workspace",
                json!({"operation": "index", "path": root.to_string_lossy()}),
            )
            .await;
        assert_eq!(response.status(), 200);
    }
    let status = running.status().await;
    let child = &status["embedding_child"];
    assert_eq!(child["state"], "ready", "got {status}");
    let pid = child["pid"].as_u64().unwrap();
    let after = running.status().await;
    assert_eq!(
        after["embedding_child"]["pid"].as_u64().unwrap(),
        pid,
        "one child, one pid"
    );
}

#[tokio::test]
async fn lexical_only_mode_never_spawns_the_child() {
    let mut env = crate::tests::helpers::env::EnvVarGuard::new();
    env.set("JULIE_EMBEDDING_PROVIDER", "none");
    let running = Running::start(None).await;
    let root_dir = tempfile::tempdir().unwrap();
    let root = root_dir.path().canonicalize().unwrap();
    std::fs::write(root.join(".git"), "gitdir: nowhere\n").unwrap();
    std::fs::write(root.join("lib.rs"), "pub fn lexical_probe() {}\n").unwrap();
    let response = running
        .api(
            "manage_workspace",
            json!({"operation": "index", "path": root.to_string_lossy()}),
        )
        .await;
    assert_eq!(response.status(), 200);
    let search = running
        .api(
            "fast_search",
            json!({"query": "lexical_probe", "workspace": root.to_string_lossy()}),
        )
        .await;
    assert_eq!(search.status(), 200);
    assert_eq!(running.status().await["embedding_child"]["state"], "absent");
}

#[cfg(unix)]
#[test]
fn child_pid_returns_without_blocking_while_batch_is_in_flight() {
    use julie_core::embeddings_contract::{EmbeddingProvider, EmbeddingRequestBudget};
    use julie_pipeline::embeddings::EmbeddingConfig;
    use julie_pipeline::embeddings::native::NativeEmbeddingProvider;
    use std::sync::Arc;
    use std::time::Duration;

    let fake_dir = tempfile::tempdir().unwrap();
    let fake_exe = crate::tests::helpers::fake_sidecar::write_with_batch_delay(fake_dir.path(), 2);
    let provider = Arc::new(
        NativeEmbeddingProvider::try_new(&EmbeddingConfig {
            provider: "native".into(),
            cache_dir: Some(fake_dir.path().to_path_buf()),
            native_program: Some(fake_exe),
            native_model: Some("fake".into()),
        })
        .unwrap(),
    );

    let p_clone = Arc::clone(&provider);
    let thread = std::thread::spawn(move || {
        let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(5));
        p_clone.embed_batch(&["hello".to_string()], &budget)
    });

    std::thread::sleep(Duration::from_millis(150));

    let start = std::time::Instant::now();
    let pid = provider.child_pid();
    let elapsed = start.elapsed();

    assert!(
        pid.is_some(),
        "child_pid should return Some(pid) while batch is in flight"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "child_pid blocked for {elapsed:?} waiting on in-flight batch"
    );

    let batch_result = thread.join().unwrap();
    assert!(batch_result.is_ok());
}
