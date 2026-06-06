use std::path::PathBuf;

use crate::daemon::session::SessionLifecyclePhase;
use crate::handler::JulieServerHandler;

fn temp_workspace_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    std::fs::create_dir_all(dir.path().join(".julie")).expect("Failed to create .julie dir");
    dir
}

#[tokio::test]
async fn handler_construction_uses_startup_hint_for_current_root() {
    let workspace_root = temp_workspace_root();

    let handler = JulieServerHandler::new(workspace_root.path().to_path_buf())
        .await
        .expect("new should succeed");

    assert_eq!(
        handler.workspace_startup_hint().path,
        workspace_root.path().to_path_buf()
    );
    assert_eq!(handler.workspace_startup_hint().source, None);
    assert_eq!(handler.current_workspace_root(), workspace_root.path());
    assert_eq!(handler.current_workspace_id(), None);
}

#[tokio::test]
async fn test_handler_primary_binding_paths_update_session_lifecycle_phase() {
    let workspace_root = temp_workspace_root();
    let handler = JulieServerHandler::new(workspace_root.path().to_path_buf())
        .await
        .expect("new should succeed");

    assert_eq!(
        handler.session_lifecycle_phase_for_test(),
        SessionLifecyclePhase::Connecting
    );

    handler.set_current_primary_binding(
        "primary_ws",
        PathBuf::from(workspace_root.path()).join("primary"),
    );
    assert_eq!(
        handler.session_lifecycle_phase_for_test(),
        SessionLifecyclePhase::Bound
    );

    handler.mark_session_serving_for_test();
    assert_eq!(
        handler.session_lifecycle_phase_for_test(),
        SessionLifecyclePhase::Serving
    );

    handler.publish_loaded_workspace_swap_intent_for_test();
    assert_eq!(
        handler.session_lifecycle_phase_for_test(),
        SessionLifecyclePhase::Connecting
    );
}
