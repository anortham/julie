use super::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_remove_workspace_uses_global_index_dir_shape() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let fake_home = tempfile::TempDir::new().unwrap();

    let original_home = std::env::var("HOME").ok();
    #[cfg(windows)]
    let original_userprofile = std::env::var("USERPROFILE").ok();

    unsafe {
        std::env::set_var("HOME", fake_home.path());
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", fake_home.path());
    }

    let daemon_paths = DaemonPaths::new();
    let indexes_dir = daemon_paths.indexes_dir();
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();

    let primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(primary_path.clone())
            .await
            .expect("primary workspace should initialize"),
    );
    let handler = JulieServerHandler::new_with_shared_workspace(
        Arc::clone(&primary_ws),
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let global_index_dir = daemon_paths.workspace_index_dir(&target_id);
    fs::create_dir_all(global_index_dir.join("db")).unwrap();
    fs::write(global_index_dir.join("db").join("symbols.db"), "target-db").unwrap();

    let legacy_nested_dir = primary_ws.indexes_root_path().join(&target_id);
    fs::create_dir_all(legacy_nested_dir.join("db")).unwrap();
    fs::write(legacy_nested_dir.join("db").join("symbols.db"), "legacy-db").unwrap();

    let result = ManageWorkspaceTool {
        operation: "remove".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("remove should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace Removed Successfully"),
        "remove should confirm success: {text}"
    );
    assert!(
        !global_index_dir.exists(),
        "remove should delete the global daemon index directory shape"
    );
    let cleanup_events = daemon_db.list_cleanup_events(10).unwrap();
    assert!(
        cleanup_events
            .iter()
            .any(|event| event.workspace_id == target_id && event.action == "manual_delete"),
        "remove should record a manual-delete cleanup event"
    );
    assert!(
        legacy_nested_dir.exists(),
        "remove should no longer target the old nested-under-primary layout"
    );

    unsafe {
        if let Some(val) = original_home {
            std::env::set_var("HOME", val);
        } else {
            std::env::remove_var("HOME");
        }
        #[cfg(windows)]
        {
            if let Some(val) = original_userprofile {
                std::env::set_var("USERPROFILE", val);
            } else {
                std::env::remove_var("USERPROFILE");
            }
        }
    }
}

#[tokio::test]
#[serial]
async fn test_remove_current_primary_workspace_is_blocked_in_process() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let fake_home = tempfile::TempDir::new().unwrap();

    let original_home = std::env::var("HOME").ok();
    #[cfg(windows)]
    let original_userprofile = std::env::var("USERPROFILE").ok();

    unsafe {
        std::env::set_var("HOME", fake_home.path());
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", fake_home.path());
    }

    let daemon_paths = DaemonPaths::new();
    let indexes_dir = daemon_paths.indexes_dir();
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();

    let primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(primary_path.clone())
            .await
            .expect("primary workspace should initialize"),
    );
    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    let global_index_dir = daemon_paths.workspace_index_dir(&primary_id);
    fs::create_dir_all(global_index_dir.join("db")).unwrap();
    fs::write(global_index_dir.join("db").join("symbols.db"), "primary-db").unwrap();

    let result = ManageWorkspaceTool {
        operation: "remove".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(primary_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("remove should return a blocked tool response");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace Delete Blocked"),
        "remove should block the live primary workspace: {text}"
    );
    assert!(
        text.contains("active in this in-process session"),
        "remove should explain the in-process activity guard: {text}"
    );
    assert!(
        daemon_db.get_workspace(&primary_id).unwrap().is_some(),
        "blocked remove must leave the registry row intact"
    );
    assert!(
        global_index_dir.exists(),
        "blocked remove must leave the live primary index intact"
    );

    unsafe {
        if let Some(val) = original_home {
            std::env::set_var("HOME", val);
        } else {
            std::env::remove_var("HOME");
        }
        #[cfg(windows)]
        {
            if let Some(val) = original_userprofile {
                std::env::set_var("USERPROFILE", val);
            } else {
                std::env::remove_var("USERPROFILE");
            }
        }
    }
}
