#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use anyhow::Result;
    use serial_test::serial;
    use tempfile::TempDir;

    use crate::database::ProjectionStatus;
    use crate::database::types::FileInfo;
    use crate::extractors::{Symbol, SymbolKind};
    use crate::handler::JulieServerHandler;
    use crate::health::{
        HealthChecker, HealthLevel, ProjectionFreshness, ProjectionState, SystemStatus,
    };
    use crate::mcp_compat::CallToolResult;
    use crate::tests::test_helpers::create_test_file;
    use crate::tools::search::FastSearchTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use crate::workspace::registry::generate_workspace_id;

    struct SkipEmbeddingsGuard;

    impl SkipEmbeddingsGuard {
        fn new() -> Self {
            unsafe {
                std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
            }
            Self
        }
    }

    impl Drop for SkipEmbeddingsGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("JULIE_SKIP_EMBEDDINGS");
            }
        }
    }

    fn extract_text_from_result(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content| {
                serde_json::to_value(content).ok().and_then(|value| {
                    value
                        .get("text")
                        .and_then(|text| text.as_str())
                        .map(|text| text.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn make_file(path: &str, content: &str) -> FileInfo {
        FileInfo {
            path: path.to_string(),
            language: "rust".to_string(),
            hash: format!("hash_{path}"),
            size: content.len() as i64,
            last_modified: 1000,
            last_indexed: 0,
            symbol_count: 1,
            line_count: content.lines().count() as i32,
            content: Some(content.to_string()),
        }
    }

    fn make_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 24,
            start_byte: 0,
            end_byte: 24,
            signature: Some(format!("fn {}()", name)),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some(format!("fn {}() {{}}", name)),
            content_type: None,
        }
    }

    async fn prepare_indexed_workspace() -> Result<(
        SkipEmbeddingsGuard,
        TempDir,
        JulieServerHandler,
        PathBuf,
        String,
    )> {
        let guard = SkipEmbeddingsGuard::new();
        let temp_dir = tempfile::tempdir()?;
        let workspace_path = temp_dir.path().to_path_buf();

        std::fs::create_dir_all(workspace_path.join("src"))?;
        create_test_file(
            &workspace_path.join("src"),
            "lib.rs",
            "pub fn repair_target() {}\n",
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        }
        .call_tool(&handler)
        .await?;

        let workspace_id =
            generate_workspace_id(&workspace_path.to_string_lossy()).expect("workspace id");

        Ok((guard, temp_dir, handler, workspace_path, workspace_id))
    }

    #[serial(embedding_env)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_system_health_reports_sqlite_only_when_projection_is_removed() -> Result<()> {
        let (_guard, _temp_dir, handler, workspace_path, workspace_id) =
            prepare_indexed_workspace().await?;

        let tantivy_dir = handler.workspace_tantivy_dir_for(&workspace_id).await?;
        let meta_path = tantivy_dir.join("meta.json");
        if meta_path.exists() {
            fs::remove_file(&meta_path)?;
        }

        {
            let mut ws_guard = handler.workspace.write().await;
            let ws = ws_guard.as_mut().expect("workspace should be initialized");
            ws.search_index = None;
        }

        let snapshot = HealthChecker::system_snapshot(&handler).await?;
        match snapshot.readiness {
            SystemStatus::SqliteOnly { symbol_count } => {
                assert!(
                    symbol_count >= 1,
                    "symbol count should survive projection loss"
                );
            }
            other => panic!("expected sqlite-only readiness, got {other:?}"),
        }
        assert_eq!(
            snapshot.data_plane.search_projection.state,
            ProjectionState::Missing
        );
        assert_eq!(
            snapshot.data_plane.search_projection.level,
            HealthLevel::Degraded
        );

        let status = HealthChecker::get_status_message(&handler).await?;
        assert!(status.contains("Partially ready"), "{status}");
        assert!(status.contains("Tantivy projection missing"), "{status}");
        assert!(workspace_path.exists(), "workspace should remain on disk");

        Ok(())
    }

    #[serial(embedding_env)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_system_health_repair_rebuilds_projection_and_restores_search() -> Result<()> {
        let (_guard, _temp_dir, handler, _workspace_path, workspace_id) =
            prepare_indexed_workspace().await?;

        let tantivy_dir = handler.workspace_tantivy_dir_for(&workspace_id).await?;
        let meta_path = tantivy_dir.join("meta.json");
        if meta_path.exists() {
            fs::remove_file(&meta_path)?;
        }

        {
            let mut ws_guard = handler.workspace.write().await;
            let ws = ws_guard.as_mut().expect("workspace should be initialized");
            ws.search_index = None;
        }

        let degraded = HealthChecker::system_snapshot(&handler).await?;
        assert_eq!(
            degraded.data_plane.search_projection.state,
            ProjectionState::Missing
        );

        {
            let mut ws_guard = handler.workspace.write().await;
            let ws = ws_guard.as_mut().expect("workspace should be initialized");
            ws.initialize_search_index()?;
        }

        let repaired = HealthChecker::system_snapshot(&handler).await?;
        assert_eq!(
            repaired.data_plane.search_projection.level,
            HealthLevel::Ready
        );
        assert_eq!(
            repaired.data_plane.search_projection.state,
            ProjectionState::Ready
        );
        match repaired.readiness {
            SystemStatus::FullyReady { symbol_count } => {
                assert!(symbol_count >= 1, "symbol count should survive repair");
            }
            other => panic!("expected full readiness after repair, got {other:?}"),
        }

        let status = HealthChecker::get_status_message(&handler).await?;
        assert!(
            status.contains("Search-ready") || status.contains("Fully operational"),
            "{status}"
        );

        let search = FastSearchTool {
            query: "repair_target".to_string(),
            language: None,
            file_pattern: None,
            limit: 5,
            workspace: Some("primary".to_string()),
            search_target: "definitions".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };
        let result = search.call_tool(&handler).await?;
        let text = extract_text_from_result(&result);
        assert!(text.contains("repair_target"), "{text}");

        Ok(())
    }

    #[serial(embedding_env)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_system_health_reports_projection_revision_lag() -> Result<()> {
        let (_guard, _temp_dir, handler, _workspace_path, workspace_id) =
            prepare_indexed_workspace().await?;

        let db = handler.get_database_for_workspace(&workspace_id).await?;
        {
            let db = db.lock().unwrap();
            assert_eq!(db.get_current_canonical_revision(&workspace_id)?, Some(1));
            let projection = db
                .get_projection_state("tantivy", &workspace_id)?
                .expect("indexed workspace should record projection state");
            assert_eq!(projection.status, ProjectionStatus::Ready);
            assert_eq!(projection.canonical_revision, Some(1));
        }

        {
            let mut db = db.lock().unwrap();
            db.incremental_update_atomic(
                &["src/lib.rs".to_string()],
                &[make_file("src/lib.rs", "pub fn lagging_target() {}\n")],
                &[make_symbol("sym_2", "lagging_target", "src/lib.rs")],
                &[],
                &[],
                &[],
                &workspace_id,
            )?;
        }

        let snapshot = HealthChecker::system_snapshot(&handler).await?;
        assert_eq!(
            snapshot.data_plane.search_projection.state,
            ProjectionState::Ready
        );
        assert_eq!(
            snapshot.data_plane.search_projection.freshness,
            ProjectionFreshness::Lagging
        );
        assert_eq!(
            snapshot.data_plane.search_projection.canonical_revision,
            Some(2)
        );
        assert_eq!(
            snapshot.data_plane.search_projection.projected_revision,
            Some(1)
        );
        assert_eq!(snapshot.data_plane.search_projection.revision_lag, Some(1));
        assert!(snapshot.data_plane.search_projection.repair_needed);
        match snapshot.readiness {
            SystemStatus::FullyReady { symbol_count } => {
                assert!(
                    symbol_count >= 1,
                    "lagging projection should stay search-ready"
                );
            }
            other => panic!("expected fully ready despite lagging projection, got {other:?}"),
        }

        let status = HealthChecker::get_status_message(&handler).await?;
        assert!(status.contains("lagging"), "{status}");
        assert!(status.contains("revision 1/2"), "{status}");

        Ok(())
    }

    #[serial(embedding_env)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_system_health_ignores_unconfigured_runtime_plane_for_overall_level() -> Result<()>
    {
        let (_guard, _temp_dir, handler, _workspace_path, _workspace_id) =
            prepare_indexed_workspace().await?;

        {
            let mut workspace = handler.workspace.write().await;
            let workspace = workspace.as_mut().expect("workspace should be initialized");
            workspace.embedding_provider = None;
            workspace.embedding_runtime_status = None;
        }

        let snapshot = HealthChecker::system_snapshot(&handler).await?;

        assert_eq!(
            snapshot.data_plane.level,
            HealthLevel::Ready,
            "indexed workspace should report a healthy data plane"
        );
        assert_eq!(
            snapshot.runtime_plane.level,
            HealthLevel::Unavailable,
            "embedding runtime is intentionally unconfigured in this harness"
        );
        assert_eq!(
            snapshot.overall,
            HealthLevel::Ready,
            "unconfigured runtime should not downgrade an otherwise healthy system"
        );

        Ok(())
    }
}
