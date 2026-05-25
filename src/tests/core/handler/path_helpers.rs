use super::*;

#[test]
fn metrics_db_path_helper_uses_current_workspace_root_for_local_storage() {
    let current_root = PathBuf::from("/tmp/rebound-primary");
    let db_path = metrics_db_path_for_workspace(None, &current_root, "ref_workspace");

    assert_eq!(
        db_path,
        PathBuf::from("/tmp/rebound-primary/.julie/indexes/ref_workspace/db/symbols.db")
    );
}

#[test]
fn workspace_root_uri_helper_parses_local_file_uri() {
    let path =
        JulieServerHandler::workspace_path_from_root_uri_for_test("file:///tmp/workspace-root")
            .expect("file uri should parse");

    assert_eq!(path, PathBuf::from("/tmp/workspace-root"));
}

#[cfg(windows)]
#[test]
fn workspace_root_uri_helper_parses_unc_file_uri() {
    let path =
        JulieServerHandler::workspace_path_from_root_uri_for_test("file://server/share/project")
            .expect("UNC file uri should parse");

    assert_eq!(path, PathBuf::from(r"\\server\share\project"));
}

#[test]
fn metrics_db_path_helper_uses_shared_index_parent_when_override_exists() {
    let current_root = PathBuf::from("/tmp/rebound-primary");
    let override_root = PathBuf::from("/tmp/shared/indexes/primary_ws");
    let db_path =
        metrics_db_path_for_workspace(Some(&override_root), &current_root, "ref_workspace");

    assert_eq!(
        db_path,
        PathBuf::from("/tmp/shared/indexes/ref_workspace/db/symbols.db")
    );
}
