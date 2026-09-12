use super::http_api::{Running, wait_for_record};
use crate::service::client::ServiceClient;
use crate::service::discovery::{self, ServiceRecord};
use crate::service::{ServiceApp, ServiceConfig};
use julie_core::paths::RegistryPaths;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

struct OwnedService {
    paths: RegistryPaths,
    record: ServiceRecord,
    engine: Arc<crate::request_engine::RequestEngine>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
    _home: tempfile::TempDir,
}

impl OwnedService {
    async fn start() -> OwnedService {
        let home = tempfile::tempdir().unwrap();
        let paths = RegistryPaths::with_home(home.path().to_path_buf());
        let app = ServiceApp::new(ServiceConfig {
            idle: None,
            registry_paths: paths.clone(),
        })
        .unwrap();
        let engine = app.engine().clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let task = tokio::spawn(app.serve(listener));
        let record = wait_for_record(&paths).await;
        OwnedService {
            paths,
            record,
            engine,
            task,
            _home: home,
        }
    }

    async fn shutdown(&mut self) {
        let client = ServiceClient::from_record(&self.record);
        assert_eq!(client.post_shutdown().await.unwrap().status(), 202);
        tokio::time::timeout(Duration::from_secs(5), &mut self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn shutdown_stops_the_service_and_removes_the_record() {
    let mut service = OwnedService::start().await;
    service.shutdown().await;
    assert!(discovery::read_record(&service.paths).unwrap().is_none());
}

#[tokio::test]
async fn shutdown_removes_only_a_record_it_owns() {
    let mut service = OwnedService::start().await;
    let mut foreign = service.record.clone();
    foreign.pid = std::process::id().wrapping_add(1);
    discovery::write_record(&service.paths, &foreign).unwrap();
    service.shutdown().await;
    assert_eq!(
        discovery::read_record(&service.paths).unwrap(),
        Some(foreign)
    );
}

#[tokio::test]
async fn service_shutdown_joins_maintenance_watchers_and_embedding_writers() {
    let mut service = OwnedService::start().await;
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join(".git")).unwrap();
    std::fs::write(
        workspace.path().join("Cargo.toml"),
        "[package]\nname=\"service-drain\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    std::fs::create_dir(workspace.path().join("src")).unwrap();
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn service_drain() {}\n",
    )
    .unwrap();

    let binding = crate::request_engine::BindingResolver::new(
        Some(workspace.path().to_path_buf()),
        false,
        service.paths.clone(),
    )
    .resolve(None, None, false)
    .unwrap()
    .unwrap();
    let runtime = service
        .engine
        .runtimes
        .acquire(
            Some(&binding),
            &crate::request_engine::RequestContext::new(
                crate::request_engine::RequestOrigin::Mcp,
                Some(Duration::from_secs(5)),
                tokio_util::sync::CancellationToken::new(),
            ),
        )
        .await
        .unwrap();
    let handler = Arc::clone(runtime.handler());
    assert!(handler.loaded_workspace_file_watcher_running().await);

    let release_writer = Arc::new(tokio::sync::Notify::new());
    let writer_finished = Arc::new(AtomicBool::new(false));
    let release = Arc::clone(&release_writer);
    let finished = Arc::clone(&writer_finished);
    handler.embedding_tasks.lock().await.insert(
        binding.workspace_id.clone(),
        (
            Arc::new(AtomicBool::new(false)),
            tokio::spawn(async move {
                release.notified().await;
                finished.store(true, Ordering::SeqCst);
            }),
        ),
    );
    drop(runtime);

    let client = ServiceClient::from_record(&service.record);
    assert_eq!(client.post_shutdown().await.unwrap().status(), 202);

    let deadline = Instant::now() + Duration::from_secs(1);
    while handler
        .embedding_tasks
        .lock()
        .await
        .contains_key(&binding.workspace_id)
    {
        assert!(
            Instant::now() < deadline,
            "shutdown did not take the embedding writer"
        );
        tokio::task::yield_now().await;
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut service.task)
            .await
            .is_err(),
        "service completed before the embedding writer joined"
    );

    release_writer.notify_one();
    tokio::time::timeout(Duration::from_secs(5), &mut service.task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(writer_finished.load(Ordering::SeqCst));
    assert!(!handler.loaded_workspace_file_watcher_running().await);
    assert_eq!(service.engine.runtimes.loaded_runtime_count().await, 0);
}

#[tokio::test]
async fn dashboard_root_requires_token_and_renders_html() {
    let running = Running::start(None).await;
    let anon = running
        .client()
        .get(format!("{}/", running.base))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status(), 401);
    let page = running
        .client()
        .get(format!("{}/?token={}", running.base, running.token))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let ct = page
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/html"), "got {ct}");
}

#[tokio::test]
async fn manage_workspace_dashboard_returns_the_service_url() {
    let running = Running::start(None).await;
    let body: serde_json::Value = running
        .client()
        .post(format!("{}/api/manage_workspace", running.base))
        .bearer_auth(&running.token)
        .json(&serde_json::json!({"operation": "dashboard"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let url = body["result"]["url"].as_str().expect("url");
    assert!(url.starts_with(&running.base));
    assert!(url.contains(&running.token));
    let _ = wait_for_record(&running.paths).await;
}

#[test]
fn version_mismatch_error_formats_and_maps_to_exit_code_3() {
    let err = crate::service::client::ConnectError::VersionMismatch {
        service: "1.0.0".into(),
        client: "1.0.1".into(),
    };
    assert_eq!(
        err.to_string(),
        format!(
            "julie: service version 1.0.0 does not match client version 1.0.1; run: \"{}\" service restart; then restart old harness clients",
            std::env::current_exe().unwrap().display()
        )
    );
    assert_eq!(crate::service::client::exit_code(&err), 3);
}

#[test]
fn service_lock_allows_only_one_owner_and_releases_on_drop() {
    let home = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(home.path().to_path_buf());

    let first = crate::service::acquire_service_lock(&paths).unwrap();
    let second = crate::service::acquire_service_lock(&paths);

    let message = second.err().unwrap().to_string();
    assert!(message.contains("could not acquire service lock"));

    drop(first);
    assert!(crate::service::acquire_service_lock(&paths).is_ok());
}
