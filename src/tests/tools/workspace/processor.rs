use crate::tools::workspace::ManageWorkspaceTool;
use std::fs;
use tempfile::TempDir;

fn workspace_tool() -> ManageWorkspaceTool {
    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    }
}

#[tokio::test]
async fn test_queue_failed_parser_file_for_cleanup_tracks_path_when_file_info_fails() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let missing_file = workspace_root.join("missing.rs");

    let tool = workspace_tool();
    let mut files_to_clean = Vec::new();
    let mut file_infos = Vec::new();

    tool.queue_failed_parser_file_for_cleanup(
        &missing_file,
        "rust",
        &workspace_root,
        &mut files_to_clean,
        &mut file_infos,
    )
    .await;

    assert_eq!(
        files_to_clean,
        vec!["missing.rs".to_string()],
        "cleanup list must include parser-failed file path"
    );
    assert!(
        file_infos.is_empty(),
        "file info refresh should fail for missing file"
    );
}

#[tokio::test]
async fn test_queue_failed_parser_file_for_cleanup_refreshes_file_info_when_available() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let file_path = workspace_root.join("failed.rs");
    fs::write(&file_path, "fn main() {}\n").unwrap();

    let tool = workspace_tool();
    let mut files_to_clean = Vec::new();
    let mut file_infos = Vec::new();

    tool.queue_failed_parser_file_for_cleanup(
        &file_path,
        "rust",
        &workspace_root,
        &mut files_to_clean,
        &mut file_infos,
    )
    .await;

    assert_eq!(
        files_to_clean,
        vec!["failed.rs".to_string()],
        "cleanup list must include parser-failed file path"
    );
    assert_eq!(
        file_infos.len(),
        1,
        "file info should be refreshed when file is readable"
    );
    assert_eq!(
        file_infos[0].path, "failed.rs",
        "refreshed file info path should be workspace-relative"
    );
}
