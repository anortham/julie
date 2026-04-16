use std::fs;
use std::sync::atomic::Ordering;

use anyhow::Result;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::pipeline::run_indexing_pipeline;
use crate::tools::workspace::indexing::route::IndexRoute;
use crate::tools::workspace::indexing::state::{
    IndexedFileDisposition, IndexingOperation, IndexingStage,
};
use crate::workspace::JulieWorkspace;

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

async fn test_handler_and_route(
    temp_dir: &TempDir,
) -> Result<(JulieServerHandler, std::path::PathBuf, IndexRoute)> {
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf()).await?;
    let workspace_root = workspace.root.clone();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_root.to_string_lossy())?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        std::sync::Arc::new(workspace),
        workspace_root.clone(),
        None,
        Some(workspace_id),
        None,
        None,
        None,
        None,
        None,
    )
    .await?;

    let route = IndexRoute::for_workspace_path(&handler, &workspace_root)
        .await
        .map_err(anyhow::Error::new)?;

    Ok((handler, workspace_root, route))
}

async fn latest_canonical_revision(
    handler: &JulieServerHandler,
    route: &IndexRoute,
) -> Result<Option<i64>> {
    let db = route
        .database_for_read(handler)
        .await?
        .expect("database should exist for indexing pipeline tests");
    let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    db.get_current_canonical_revision(&route.workspace_id)
}

async fn symbol_count(handler: &JulieServerHandler, route: &IndexRoute) -> Result<i64> {
    let db = route
        .database_for_read(handler)
        .await?
        .expect("database should exist for indexing pipeline tests");
    let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    db.conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .map_err(anyhow::Error::from)
}

fn expected_stage_history() -> Vec<IndexingStage> {
    vec![
        IndexingStage::Queued,
        IndexingStage::Grouped,
        IndexingStage::Extracting,
        IndexingStage::Persisting,
        IndexingStage::Projecting,
        IndexingStage::Resolving,
        IndexingStage::Analyzing,
        IndexingStage::Completed,
    ]
}

#[tokio::test]
async fn test_indexing_pipeline_reports_stage_history_for_parser_backed_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    fs::write(temp_dir.path().join("lib.rs"), "fn parser_backed() {}\n")?;

    let (handler, workspace_root, route) = test_handler_and_route(&temp_dir).await?;
    let result = run_indexing_pipeline(
        &workspace_tool(),
        &handler,
        vec![workspace_root.join("lib.rs")],
        &route,
        IndexingOperation::Incremental,
    )
    .await?;

    assert_eq!(
        result.state.stage_history,
        expected_stage_history(),
        "parser-backed files should traverse the full indexing pipeline"
    );
    assert_eq!(result.files_processed, 1, "one file should be processed");
    assert_eq!(
        result.canonical_revision,
        Some(1),
        "successful pipeline runs should surface the committed canonical revision"
    );
    assert_eq!(result.state.parsed_file_count(), 1);
    assert_eq!(result.state.text_only_file_count(), 0);
    assert_eq!(result.state.repair_file_count(), 0);
    assert!(
        handler.indexing_status.search_ready.load(Ordering::Acquire),
        "successful pipeline runs should publish search readiness"
    );
    assert_eq!(
        latest_canonical_revision(&handler, &route).await?,
        Some(1),
        "database revision should match the surfaced canonical revision"
    );

    Ok(())
}

#[tokio::test]
async fn test_indexing_pipeline_reports_stage_history_for_text_only_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    fs::write(temp_dir.path().join("notes.txt"), "plain text fallback\n")?;

    let (handler, workspace_root, route) = test_handler_and_route(&temp_dir).await?;
    let result = run_indexing_pipeline(
        &workspace_tool(),
        &handler,
        vec![workspace_root.join("notes.txt")],
        &route,
        IndexingOperation::Incremental,
    )
    .await?;

    assert_eq!(
        result.state.stage_history,
        expected_stage_history(),
        "text-only files should still traverse the full indexing pipeline"
    );
    assert_eq!(result.files_processed, 1, "one file should be processed");
    assert_eq!(result.state.parsed_file_count(), 0);
    assert_eq!(result.state.text_only_file_count(), 1);
    assert_eq!(
        result.state.file_states[0].disposition,
        IndexedFileDisposition::TextOnly
    );
    assert_eq!(result.state.repair_file_count(), 0);
    assert!(
        handler.indexing_status.search_ready.load(Ordering::Acquire),
        "successful pipeline runs should publish search readiness"
    );

    Ok(())
}

#[tokio::test]
async fn test_indexing_pipeline_marks_missing_parser_files_as_repair_needed() -> Result<()> {
    let temp_dir = TempDir::new()?;

    let (_handler, workspace_root, route) = test_handler_and_route(&temp_dir).await?;
    let result = run_indexing_pipeline(
        &workspace_tool(),
        &_handler,
        vec![workspace_root.join("missing.rs")],
        &route,
        IndexingOperation::Incremental,
    )
    .await?;

    assert_eq!(
        result.state.stage_history,
        expected_stage_history(),
        "repair-needed files should still report stage history through completion"
    );
    assert_eq!(
        result.files_processed, 0,
        "missing files should not count as processed"
    );
    assert!(result.state.repair_needed());
    assert_eq!(result.state.repair_file_count(), 1);
    assert_eq!(
        result.state.file_states[0].disposition,
        IndexedFileDisposition::RepairNeeded
    );

    Ok(())
}

#[tokio::test]
async fn test_indexing_pipeline_keeps_search_unready_when_projection_fails() -> Result<()> {
    let temp_dir = TempDir::new()?;
    fs::write(temp_dir.path().join("lib.rs"), "fn parser_backed() {}\n")?;

    let (handler, workspace_root, mut route) = test_handler_and_route(&temp_dir).await?;
    let search_index = route
        .search_index_for_write()
        .await?
        .expect("search index should open for projection failure test");
    {
        let idx = search_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        idx.shutdown()?;
    }
    route.search_index = Some(search_index);

    let result = run_indexing_pipeline(
        &workspace_tool(),
        &handler,
        vec![workspace_root.join("lib.rs")],
        &route,
        IndexingOperation::Incremental,
    )
    .await?;

    assert!(
        result.state.repair_needed(),
        "projection failures should surface repair-needed state"
    );
    assert_eq!(
        result.canonical_revision,
        Some(1),
        "projection failures must still report the committed canonical revision"
    );
    assert!(
        !handler.indexing_status.search_ready.load(Ordering::Acquire),
        "failed Tantivy projection must not publish search readiness"
    );
    assert_eq!(
        latest_canonical_revision(&handler, &route).await?,
        Some(1),
        "canonical revision must commit even when Tantivy projection fails"
    );
    assert_eq!(
        symbol_count(&handler, &route).await?,
        1,
        "SQLite canonical state must survive a failed projection pass"
    );

    Ok(())
}
