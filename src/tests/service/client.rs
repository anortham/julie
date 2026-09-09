use super::http_api::Running;
use crate::service::client::{ConnectError, connect_or_start};
use crate::service::discovery::{self, ServiceRecord};
use julie_core::paths::RegistryPaths;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn connects_to_a_running_service_without_spawning() {
    let running = Running::start(None).await;
    let spawns = Arc::new(AtomicUsize::new(0));
    let s = Arc::clone(&spawns);
    let client = connect_or_start(&running.paths, move || {
        s.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(client.token, running.token);
    assert_eq!(spawns.load(Ordering::SeqCst), 0);
    assert!(client.status().await.unwrap()["version"].is_string());
}

#[tokio::test]
async fn stale_record_is_removed_and_the_spawn_hook_runs_once() {
    let home = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(home.path().to_path_buf());
    let dead_port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    discovery::write_record(
        &paths,
        &ServiceRecord {
            port: dead_port,
            token: "0".repeat(64),
            pid: 0,
            version: env!("CARGO_PKG_VERSION").into(),
            started_at: "0".into(),
        },
    )
    .unwrap();
    let spawns = Arc::new(AtomicUsize::new(0));
    let s = Arc::clone(&spawns);
    let result = connect_or_start(&paths, move || {
        s.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(ConnectError::Unavailable(_))));
    assert_eq!(spawns.load(Ordering::SeqCst), 1);
    assert!(discovery::read_record(&paths).unwrap().is_none());
}

#[tokio::test]
async fn spawn_hook_that_writes_a_record_lets_the_client_connect() {
    let running = Running::start(None).await;
    let live = discovery::read_record(&running.paths).unwrap().unwrap();
    let home = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(home.path().to_path_buf());
    let target = paths.clone();
    let client = connect_or_start(&paths, move || discovery::write_record(&target, &live))
        .await
        .unwrap();
    assert_eq!(client.token, running.token);
}

#[tokio::test]
async fn version_mismatch_is_reported_with_both_versions() {
    let running = Running::start(None).await;
    let mut record = discovery::read_record(&running.paths).unwrap().unwrap();
    record.version = "0.0.0-other".into();
    discovery::write_record(&running.paths, &record).unwrap();
    let err = connect_or_start(&running.paths, || Ok(()))
        .await
        .err()
        .unwrap();
    match err {
        ConnectError::VersionMismatch { service, client } => {
            assert_eq!(service, "0.0.0-other");
            assert_eq!(client, env!("CARGO_PKG_VERSION"));
        }
        other => panic!("expected mismatch, got {other:?}"),
    }
}
