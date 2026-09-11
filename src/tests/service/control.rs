use super::http_api::{Running, wait_for_record};
use crate::service::client::ServiceClient;
use crate::service::discovery::{self, ServiceRecord};
use crate::service::{ServiceApp, ServiceConfig};
use julie_core::paths::RegistryPaths;
use std::time::Duration;

struct OwnedService {
    paths: RegistryPaths,
    record: ServiceRecord,
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let task = tokio::spawn(app.serve(listener));
        let record = wait_for_record(&paths).await;
        OwnedService {
            paths,
            record,
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
        "julie: service version 1.0.0 does not match client version 1.0.1; run: julie-server service restart"
    );
    assert_eq!(crate::service::client::exit_code(&err), 3);
}
