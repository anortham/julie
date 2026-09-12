use crate::registry::database::DaemonDatabase;
use crate::registry::workspace_registry_store::WorkspaceRegistryStore;
use crate::service::{ServiceApp, ServiceConfig, discovery};
use crate::tools::workspace::commands::registry::registry_store_for;
use julie_core::paths::RegistryPaths;
use std::time::Duration;

pub(crate) struct Running {
    pub paths: RegistryPaths,
    pub base: String,
    pub token: String,
    pub engine: std::sync::Arc<crate::request_engine::RequestEngine>,
    _home: tempfile::TempDir,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Running {
    pub(crate) async fn start(idle: Option<Duration>) -> Running {
        Running::start_in(tempfile::tempdir().unwrap(), idle).await
    }

    pub(crate) async fn start_in(home: tempfile::TempDir, idle: Option<Duration>) -> Running {
        let paths = RegistryPaths::with_home(home.path().to_path_buf());
        let app = ServiceApp::new(ServiceConfig {
            idle,
            registry_paths: paths.clone(),
        })
        .unwrap();
        let engine = app.engine().clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(app.serve(listener));
        let record = wait_for_record(&paths).await;
        assert_eq!(record.port, port);
        Running {
            paths,
            base: format!("http://127.0.0.1:{port}"),
            token: record.token,
            engine,
            _home: home,
            task,
        }
    }
    pub(crate) fn client(&self) -> reqwest::Client {
        reqwest::Client::new()
    }
    pub(crate) fn engine(&self) -> &std::sync::Arc<crate::request_engine::RequestEngine> {
        &self.engine
    }
    pub(crate) async fn api(&self, tool: &str, params: serde_json::Value) -> reqwest::Response {
        self.client()
            .post(format!("{}/api/{}", self.base, tool))
            .bearer_auth(&self.token)
            .json(&params)
            .send()
            .await
            .unwrap()
    }
    pub(crate) async fn status(&self) -> serde_json::Value {
        self.client()
            .get(format!("{}/status", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    pub(crate) async fn finished(self) -> anyhow::Result<()> {
        self.task.await.unwrap()
    }
}

pub(crate) async fn wait_for_record(paths: &RegistryPaths) -> discovery::ServiceRecord {
    for _ in 0..100 {
        if let Ok(Some(r)) = discovery::read_record(paths) {
            return r;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("service.json not written");
}

#[tokio::test]
async fn service_json_names_the_bound_port_and_a_64_hex_token() {
    let running = Running::start(None).await;
    assert_eq!(running.token.len(), 64);
    assert!(running.token.chars().all(|c| c.is_ascii_hexdigit()));
    let record = discovery::read_record(&running.paths).unwrap().unwrap();
    assert_eq!(record.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(record.pid, std::process::id());
}

#[cfg(unix)]
#[tokio::test]
async fn service_json_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let running = Running::start(None).await;
    let mode = std::fs::metadata(running.paths.service_json())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn status_without_token_is_401() {
    let running = Running::start(None).await;
    let res = running
        .client()
        .get(format!("{}/status", running.base))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    assert_eq!(res.text().await.unwrap(), r#"{"error":"unauthorized"}"#);
}

#[tokio::test]
async fn status_with_header_or_query_token_reports_version_and_uptime() {
    let running = Running::start(None).await;
    let by_header = running
        .client()
        .get(format!("{}/status", running.base))
        .bearer_auth(&running.token)
        .send()
        .await
        .unwrap();
    assert_eq!(by_header.status(), 200);
    let body: serde_json::Value = by_header.json().await.unwrap();
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert!(body["uptime_seconds"].is_number());
    assert!(body["requests"].is_array());
    assert_eq!(body["embedding_child"]["state"], "absent");
    let by_query = running
        .client()
        .get(format!("{}/status?token={}", running.base, running.token))
        .send()
        .await
        .unwrap();
    assert_eq!(by_query.status(), 200);
}

#[tokio::test]
async fn api_workspace_list_returns_a_tool_reply_envelope() {
    let running = Running::start(None).await;
    let res = running
        .client()
        .post(format!("{}/api/manage_workspace", running.base))
        .bearer_auth(&running.token)
        .json(&serde_json::json!({"operation": "list"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["tool"], "manage_workspace");
    assert_eq!(body["schema_version"], 1);
    assert!(body["result"].is_object());
}

#[tokio::test]
async fn api_unknown_tool_returns_request_failure_as_400() {
    let running = Running::start(None).await;
    let res = running
        .client()
        .post(format!("{}/api/no_such_tool", running.base))
        .bearer_auth(&running.token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["code"].is_string());
    assert!(body["message"].is_string());
}

#[tokio::test]
async fn api_semantics_mode_reaches_request_readiness_and_required_failure() {
    let running = Running::start(None).await;
    let root_dir = tempfile::tempdir().unwrap();
    let root = root_dir.path().join("semantic-api");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn api_probe() {}\n").unwrap();
    let indexed = running
        .api(
            "manage_workspace",
            serde_json::json!({ "operation": "index", "path": root }),
        )
        .await;
    assert_eq!(indexed.status(), reqwest::StatusCode::OK);

    let required = running
        .api(
            "get_context",
            serde_json::json!({
                "query": "api probe",
                "workspace": root,
                "semantics": "required"
            }),
        )
        .await;
    assert_eq!(required.status(), reqwest::StatusCode::BAD_REQUEST);
    let failure: serde_json::Value = required.json().await.unwrap();
    assert_eq!(failure["code"], "SEMANTICS_NOT_READY");

    let off = running
        .api(
            "fast_search",
            serde_json::json!({
                "query": "api_probe",
                "backend": "lexical",
                "workspace": root,
                "semantics": "off"
            }),
        )
        .await;
    assert_eq!(off.status(), reqwest::StatusCode::OK);
    let reply: serde_json::Value = off.json().await.unwrap();
    assert_eq!(reply["readiness"]["mode"], "off");
    assert_eq!(reply["readiness"]["status"], "disabled");

    let invalid = running
        .api(
            "fast_search",
            serde_json::json!({
                "query": "api_probe",
                "workspace": root,
                "semantics": "sometimes"
            }),
        )
        .await;
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);
    let failure: serde_json::Value = invalid.json().await.unwrap();
    assert_eq!(failure["code"], "INVALID_ARGUMENTS");

    let mut nullable = serde_json::json!({ "semantics": null })
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(
        crate::request_engine::types::take_semantic_mode(&mut nullable).unwrap(),
        crate::request_engine::SemanticMode::Auto
    );
}

#[tokio::test]
async fn status_lists_the_last_requests_newest_first() {
    let running = Running::start(None).await;
    for _ in 0..3 {
        running
            .client()
            .post(format!("{}/api/manage_workspace", running.base))
            .bearer_auth(&running.token)
            .json(&serde_json::json!({"operation": "list"}))
            .send()
            .await
            .unwrap();
    }
    let body: serde_json::Value = running
        .client()
        .get(format!("{}/status", running.base))
        .bearer_auth(&running.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let requests = body["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0]["tool"], "manage_workspace");
    assert!(requests[0]["latency_ms"].is_number());
    assert_eq!(requests[0]["outcome"], "ok");
}

#[tokio::test]
async fn status_carries_every_checkout() {
    let running = Running::start(None).await;
    let root_dir = tempfile::tempdir().unwrap();
    let root = root_dir.path().canonicalize().unwrap();
    std::fs::write(root.join(".git"), "gitdir: nowhere\n").unwrap();
    std::fs::write(root.join("lib.rs"), "pub fn status_probe() {}\n").unwrap();
    let indexed = running
        .client()
        .post(format!("{}/api/manage_workspace", running.base))
        .bearer_auth(&running.token)
        .json(&serde_json::json!({"operation": "index", "path": root.to_string_lossy()}))
        .send()
        .await
        .unwrap();
    assert_eq!(indexed.status(), 200);

    let body: serde_json::Value = running
        .client()
        .get(format!("{}/status", running.base))
        .bearer_auth(&running.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let checkouts = body["checkouts"].as_array().unwrap();
    let expected_id =
        crate::workspace::registry::generate_workspace_id(&root.to_string_lossy()).unwrap();
    let checkout = checkouts
        .iter()
        .find(|c| c["workspace_id"] == expected_id)
        .unwrap_or_else(|| panic!("checkout missing: {checkouts:?}"));
    assert_eq!(checkout["root"], root.to_string_lossy().as_ref());
    assert_eq!(checkout["root_exists"], true);
    assert_eq!(checkout["file_count"], 1);
    assert_eq!(checkout["symbol_count"], 1);
    assert_eq!(checkout["tantivy"], "present");
    assert!(checkout["facts_bytes"].as_u64().unwrap() > 0);
    assert_eq!(checkout["vector_count"], 0);
    assert!(checkout.get("vector_scan_millis").is_some());
    assert!(checkout["last_write_at"].is_string());
}

#[tokio::test]
async fn idle_exit_removes_service_json() {
    let running = Running::start(Some(Duration::from_millis(300))).await;
    let paths = running.paths.clone();
    tokio::time::timeout(Duration::from_secs(5), running.finished())
        .await
        .expect("service did not exit when idle")
        .unwrap();
    assert!(discovery::read_record(&paths).unwrap().is_none());
}

#[tokio::test]
async fn status_reports_a_service_log_under_the_julie_home() {
    let running = Running::start(None).await;
    running
        .api("manage_workspace", serde_json::json!({"operation": "list"}))
        .await;
    let body = running.status().await;
    let log = body["log"].as_str().expect("status has no log path");
    let expected_prefix = running
        .paths
        .logs_dir()
        .join("julie-service.log.")
        .to_string_lossy()
        .into_owned();
    assert!(log.starts_with(&expected_prefix), "{log}");
    let mut contents = String::new();
    for _ in 0..100 {
        contents = std::fs::read_to_string(log).unwrap_or_default();
        if contents.contains("Cleanup sweep finished at service start") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        contents.contains("tool_call: manage_workspace"),
        "{contents}"
    );
    assert!(
        contents.contains("Cleanup sweep finished at service start"),
        "{contents}"
    );
}

fn registry_store_at(paths: &RegistryPaths) -> WorkspaceRegistryStore {
    let db = std::sync::Arc::new(DaemonDatabase::open(&paths.registry_db()).unwrap());
    registry_store_for(&db).unwrap()
}

#[tokio::test]
async fn service_start_prunes_dead_registry_rows_in_the_background() {
    let home = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(home.path().to_path_buf());
    let live_root = tempfile::tempdir().unwrap();
    let dead_path = home.path().join("gone");

    let store = registry_store_at(&paths);
    store
        .upsert_workspace("dead_00000000", dead_path.to_str().unwrap(), "ready")
        .unwrap();
    store
        .upsert_workspace("live_00000000", live_root.path().to_str().unwrap(), "ready")
        .unwrap();
    std::fs::create_dir_all(store.index_dir_for("live_00000000")).unwrap();
    std::fs::create_dir_all(paths.indexes_dir().join("orphan_00000000")).unwrap();
    drop(store);

    let running = Running::start_in(home, None).await;

    let store = registry_store_at(&running.paths);
    let orphan_dir = running.paths.indexes_dir().join("orphan_00000000");
    let mut swept = false;
    for _ in 0..100 {
        if store.get_workspace("dead_00000000").unwrap().is_none() && !orphan_dir.exists() {
            swept = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(swept, "the start sweep left the dead row or the orphan dir");
    assert!(store.get_workspace("live_00000000").unwrap().is_some());
    assert!(running.paths.indexes_dir().join("live_00000000").is_dir());
}
