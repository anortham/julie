use super::*;

#[tokio::test]
async fn test_status_live_exposes_nested_health_snapshot() {
    let (state, _temp_dir) = test_state_with_db();
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/status/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["health"]["overall"], "ready");
    assert_eq!(json["health"]["control_plane"]["active_sessions"], 0);
    assert_eq!(json["health"]["control_plane"]["daemon_phase"], "ready");
    assert_eq!(
        json["health"]["control_plane"]["session_phases"]["connecting"],
        0
    );
    assert_eq!(json["health"]["data_plane"]["workspace_count"], 1);
    assert_eq!(json["health"]["data_plane"]["ready_workspace_count"], 1);
    assert_eq!(json["health"]["runtime_plane"]["configured"], false);
    assert_eq!(
        json["health"]["runtime_plane"]["embeddings"]["query_fallback"],
        "keyword-only"
    );
}

#[ignore = "dashboard live-data dark after Phase 3d.2b pool de-type; standalone registry-reader dashboard rebuilt in 3d.3"]
#[tokio::test]
async fn test_status_live_exposes_indexing_health_snapshot() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace dir");
    let workspace_id =
        generate_workspace_id(&workspace_root.to_string_lossy()).expect("workspace id");

    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&workspace_id, 10, 1, None, None, None)
        .unwrap();

    let workspace = Arc::new(
        crate::workspace::JulieWorkspace::initialize(workspace_root.clone())
            .await
            .expect("workspace init"),
    );
    {
        let mut indexing = workspace.indexing_runtime.write().unwrap();
        indexing.begin_operation(IndexingOperation::Incremental);
        indexing.transition_stage(IndexingStage::Projecting);
        indexing.set_catchup_active(true);
        indexing.set_watcher_paused(true);
        indexing.set_dirty_projection_count(2);
        indexing.record_repair_reason(IndexingRepairReason::ProjectionFailure);
    }

    let state = DashboardState::new(
        Arc::new(SessionTracker::new()),
        Some(daemon_db),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None,
        50,
    );
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/status/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["health"]["data_plane"]["indexing"]["active_operation"],
        "catch_up"
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["stage"],
        "projecting"
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["catchup_active"],
        true
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["watcher_paused"],
        true
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["dirty_projection_count"],
        2
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["repair_needed"],
        true
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["repair_reasons"][0],
        "projection_failure"
    );
}

#[tokio::test]
async fn test_status_live_exposes_projection_list_contract() {
    let (state, _temp_dir) = test_state_with_db();
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/status/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        json["health"]["data_plane"]
            .as_object()
            .is_some_and(|data_plane| !data_plane.contains_key("search_projection"))
    );
    let projections = json["health"]["data_plane"]["projections"]
        .as_array()
        .expect("projection list");
    assert_eq!(projections.len(), 2);
    assert_eq!(projections[0]["name"], "tantivy");
    assert_eq!(projections[1]["name"], "web_edges");
    for projection in projections {
        assert_eq!(projection["level"], "unavailable");
        assert_eq!(projection["state"], "missing");
        assert_eq!(projection["freshness"], "unavailable");
        assert_eq!(projection["workspace_id"], serde_json::Value::Null);
        assert_eq!(projection["canonical_revision"], serde_json::Value::Null);
        assert_eq!(projection["projected_revision"], serde_json::Value::Null);
        assert_eq!(projection["revision_lag"], serde_json::Value::Null);
        assert_eq!(projection["repair_needed"], false);
        assert!(
            projection["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("workspace pool is detached"))
        );
    }
}

#[tokio::test]
async fn test_status_page_renders_health_sections() {
    let (state, _temp_dir, _workspace_id) = state_with_projection_lag().await;
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("Control Plane"));
    assert!(html.contains("Data Plane"));
    assert!(html.contains("Runtime Plane"));
    assert!(html.contains("Overall Health"));
    assert!(html.contains("Daemon Phase"));
    assert!(html.contains("Session Phases"));
    assert_eq!(
        html.matches("id=\"data-plane-projection-tantivy-status\"")
            .count(),
        1
    );
    assert_eq!(
        html.matches("id=\"data-plane-projection-web_edges-status\"")
            .count(),
        1
    );
    assert!(html.contains("data-plane-projection-${projection.name}"));
    assert!(html.contains("d.health.data_plane.projections || []"));
    assert!(html.contains("Indexing Operation"));
    assert!(html.contains("Dirty Projection Entries"));
    assert!(html.contains("Repair Reasons"));
    assert!(html.contains("formatUpperValue(d.health.runtime_plane.embeddings.state)"));
    assert!(
        !html.contains("formatUpper(d.health.runtime_plane.embeddings.state)"),
        "runtime state poller should call the defined formatter helper"
    );
}
