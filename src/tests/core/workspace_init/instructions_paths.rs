use super::*;

/// Test: agent instructions are always available (embedded at compile time)
///
/// JULIE_AGENT_INSTRUCTIONS.md is product metadata embedded via include_str!,
/// so instructions are available regardless of the workspace being indexed.
#[tokio::test]
#[serial]
async fn test_agent_instructions_always_available() {
    use crate::handler::JulieServerHandler;

    // Use an empty temp dir as workspace — no JULIE_AGENT_INSTRUCTIONS.md present
    let empty_workspace = setup_test_workspace();

    let handler = JulieServerHandler::new(empty_workspace.path().to_path_buf())
        .await
        .expect("Failed to create handler");

    use rmcp::ServerHandler;
    let info = handler.get_info();

    // Instructions should always be present (embedded at compile time)
    assert!(
        info.instructions.is_some(),
        "get_info().instructions should always be Some (embedded at compile time)"
    );

    let instructions = info.instructions.unwrap();
    assert!(
        instructions.contains("Rules"),
        "Embedded instructions should contain expected content"
    );
    assert!(
        instructions.contains("fast_search"),
        "Embedded instructions should reference Julie tools"
    );
}

/// Regression test: workspace_db_path and workspace_tantivy_path must return
/// different paths for different workspace IDs, even when index_root_override
/// is set (daemon mode). Previously, the override branch ignored workspace_id
/// entirely, causing all reference workspace data to be written to the primary
/// workspace's database.
#[test]
fn test_workspace_paths_differ_per_workspace_id_with_override() {
    let tmp = TempDir::new().unwrap();
    let julie_dir = tmp.path().join(".julie");
    fs::create_dir_all(&julie_dir).unwrap();

    let primary_id = "julie_528d4264";
    let ref_id = "zod_4e845d39";

    // Simulate daemon mode: override points to primary workspace's index dir
    let shared_indexes = tmp.path().join("indexes");
    let override_path = shared_indexes.join(primary_id);
    fs::create_dir_all(&override_path).unwrap();

    let workspace = crate::workspace::JulieWorkspace {
        root: tmp.path().to_path_buf(),
        julie_dir: julie_dir.clone(),
        db: None,
        search_index: None,
        watcher: None,
        embedding_provider: None,
        embedding_runtime_status: None,
        config: Default::default(),
        index_root_override: Some(override_path.clone()),
        indexing_runtime: crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
    };

    let primary_db = workspace.workspace_db_path(primary_id);
    let ref_db = workspace.workspace_db_path(ref_id);

    assert_ne!(
        primary_db,
        ref_db,
        "workspace_db_path must return different paths for different workspace IDs. \
         Got same path for both: {}",
        primary_db.display()
    );
    assert!(
        primary_db.to_string_lossy().contains(primary_id),
        "Primary DB path should contain primary workspace ID: {}",
        primary_db.display()
    );
    assert!(
        ref_db.to_string_lossy().contains(ref_id),
        "Reference DB path should contain reference workspace ID: {}",
        ref_db.display()
    );

    // Same check for Tantivy paths
    let primary_tantivy = workspace.workspace_tantivy_path(primary_id);
    let ref_tantivy = workspace.workspace_tantivy_path(ref_id);

    assert_ne!(
        primary_tantivy, ref_tantivy,
        "workspace_tantivy_path must return different paths for different workspace IDs"
    );

    // Same check for index path
    let primary_index = workspace.workspace_index_path(primary_id);
    let ref_index = workspace.workspace_index_path(ref_id);

    assert_ne!(
        primary_index, ref_index,
        "workspace_index_path must return different paths for different workspace IDs"
    );

    // Verify both paths share the same parent (shared indexes dir)
    let db_parent = ref_db.parent().unwrap().parent().unwrap().parent().unwrap();
    let expected_parent = primary_db
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    assert_eq!(
        db_parent, expected_parent,
        "Both workspace paths should share the same indexes parent directory"
    );
}
