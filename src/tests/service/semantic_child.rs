use super::http_api::Running;
use serde_json::json;

#[cfg(unix)]
#[tokio::test]
async fn two_checkouts_share_one_embedding_child() {
    let fake_dir = tempfile::tempdir().unwrap();
    let fake = crate::tests::helpers::fake_sidecar::write(fake_dir.path());
    let mut env = crate::tests::helpers::env::EnvVarGuard::new();
    env.set("JULIE_NATIVE_SIDECAR_PROGRAM", fake.as_os_str());
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
    assert_eq!(after["embedding_child"]["pid"].as_u64().unwrap(), pid, "one child, one pid");
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
