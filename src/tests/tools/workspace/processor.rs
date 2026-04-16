use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::state::{
    IndexedFileDisposition, IndexingBatchState, IndexingStage,
};
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

#[test]
fn test_indexing_batch_state_tracks_stage_history_without_duplicates() {
    let mut state = IndexingBatchState::new("workspace-123");

    state.transition_to(IndexingStage::Grouped);
    state.transition_to(IndexingStage::Grouped);
    state.transition_to(IndexingStage::Extracting);
    state.transition_to(IndexingStage::Completed);

    assert_eq!(
        state.stage_history,
        vec![
            IndexingStage::Queued,
            IndexingStage::Grouped,
            IndexingStage::Extracting,
            IndexingStage::Completed,
        ],
        "duplicate stage transitions should not pollute history"
    );
}

#[test]
fn test_indexing_batch_state_marks_repair_needed_files() {
    let mut state = IndexingBatchState::new("workspace-123");

    state.record_file(
        "missing.rs",
        "rust",
        IndexedFileDisposition::RepairNeeded,
        Some("failed to read file".to_string()),
    );

    assert!(
        state.repair_needed(),
        "repair-needed files must be surfaced"
    );
    assert_eq!(
        state.repair_file_count(),
        1,
        "repair-needed files should be counted explicitly"
    );
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
