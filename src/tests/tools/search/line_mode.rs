//! Tests for fast_search line-level output mode
//! TDD: RED → GREEN → REFACTOR

#[cfg(test)]
mod search_line_mode_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::{Symbol, SymbolKind};
    use crate::handler::JulieServerHandler;
    use crate::tools::search::FastSearchTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use anyhow::Result;
    use chrono::Utc;
    use rmcp::{
        ServerHandler,
        model::{CallToolRequestParams, NumberOrString, ServerJsonRpcMessage, ServerRequest},
        service::{RequestContext, serve_directly},
    };
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::time::{Duration, sleep};

    fn extract_text_from_result(result: &crate::mcp_compat::CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[allow(dead_code)]
    fn extract_workspace_id(result: &crate::mcp_compat::CallToolResult) -> Option<String> {
        let text = extract_text_from_result(result);
        text.lines()
            .find(|line| line.contains("Workspace ID:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|id| id.trim().to_string())
    }

    async fn send_json_line(writer: &mut (impl AsyncWriteExt + Unpin), value: &serde_json::Value) {
        writer
            .write_all(serde_json::to_string(value).unwrap().as_bytes())
            .await
            .unwrap();
        writer.write_all(b"\n").await.unwrap();
        writer.flush().await.unwrap();
    }

    async fn read_server_message(
        lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
    ) -> ServerJsonRpcMessage {
        let line = lines
            .next_line()
            .await
            .unwrap()
            .expect("server should emit a JSON-RPC message line");
        serde_json::from_str(&line).unwrap()
    }

    async fn mark_index_ready(handler: &JulieServerHandler) {
        handler
            .indexing_status
            .search_ready
            .store(true, Ordering::Relaxed);
        *handler.is_indexed.write().await = true;
    }

    async fn setup_loaded_primary_without_tantivy() -> Result<(TempDir, JulieServerHandler)> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();
        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;
        fs::write(
            src_dir.join("example.rs"),
            "pub fn loaded_primary_missing_tantivy() {}\n",
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())?;
        let tantivy_dir = handler.workspace_tantivy_dir_for(&workspace_id).await?;
        let meta_path = tantivy_dir.join("meta.json");
        if meta_path.exists() {
            fs::remove_file(meta_path)?;
        }

        mark_index_ready(&handler).await;

        Ok((temp_dir, handler))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_basic() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        // Create test file with known content
        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        let test_file = src_dir.join("example.rs");
        fs::write(
            &test_file,
            r#"// TODO: implement authentication
fn getUserData() {
    // TODO: add validation
    println!("Getting user data");
}

fn processPayment() {
    // This function is complete
    println!("Processing payment");
}
"#,
        )?;

        // Initialize handler and index
        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "TODO".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("TODO: implement authentication"),
            "Should find first TODO comment"
        );
        assert!(
            response_text.contains("TODO: add validation"),
            "Should find second TODO comment"
        );
        assert!(
            response_text.contains("Line 1") || response_text.contains(":1:"),
            "Should include line numbers"
        );
        assert!(
            !response_text.contains("Processing payment"),
            "Should NOT include unrelated lines"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_respects_workspace_filter() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // Create two files with distinct content
        let file1 = src_dir.join("module_a.rs");
        fs::write(
            &file1,
            "fn function_alpha() { println!(\"alpha_marker\"); }\n",
        )?;

        let file2 = src_dir.join("module_b.rs");
        fs::write(
            &file2,
            "fn function_beta() { println!(\"beta_marker\"); }\n",
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        // Test 1: Search primary workspace explicitly - should find results
        let search_primary = FastSearchTool {
            query: "alpha_marker".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_primary.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("alpha_marker"),
            "Primary workspace search should find content: {}",
            response_text
        );
        assert!(
            response_text.contains("module_a.rs"),
            "Primary workspace search should show correct file: {}",
            response_text
        );

        // Test 2: Search with invalid workspace ID - should return error
        let search_invalid = FastSearchTool {
            query: "alpha_marker".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("nonexistent_workspace_id".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_invalid.call_tool(&handler).await;
        assert!(
            result.is_err(),
            "Searching non-existent workspace should return error"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_primary_uses_rebound_session_primary() -> Result<()> {
        use crate::daemon::database::DaemonDatabase;
        use crate::daemon::workspace_pool::WorkspacePool;
        use crate::workspace::registry::generate_workspace_id;
        use std::sync::Arc;

        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let indexes_dir = temp_dir.path().join("indexes");
        fs::create_dir_all(&indexes_dir)?;

        let original_root = temp_dir.path().join("original-primary");
        let rebound_root = temp_dir.path().join("rebound-primary");
        fs::create_dir_all(&original_root)?;
        fs::create_dir_all(&rebound_root)?;
        fs::write(
            original_root.join("main.rs"),
            "fn original_workspace_only() { println!(\"original_only_marker\"); }\n",
        )?;
        fs::write(
            rebound_root.join("lib.rs"),
            "fn rebound_workspace_only() { println!(\"rebound_only_marker\"); }\n",
        )?;

        let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
            None,
            None,
        ));

        let original_path = original_root.canonicalize()?;
        let original_path_str = original_path.to_string_lossy().to_string();
        let original_id = generate_workspace_id(&original_path_str)?;
        let original_ws = pool
            .get_or_init(&original_id, original_path.clone())
            .await?;

        let handler = JulieServerHandler::new_with_shared_workspace(
            original_ws,
            original_path.clone(),
            Some(Arc::clone(&daemon_db)),
            Some(original_id.clone()),
            None,
            None,
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await?;

        daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

        let rebound_path = rebound_root.canonicalize()?;
        let rebound_path_str = rebound_path.to_string_lossy().to_string();
        let rebound_id = generate_workspace_id(&rebound_path_str)?;
        daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

        let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
        let seed_handler = JulieServerHandler::new_with_shared_workspace(
            rebound_ws,
            rebound_path.clone(),
            Some(Arc::clone(&daemon_db)),
            Some(rebound_id.clone()),
            None,
            None,
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await?;

        ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(rebound_path_str.clone()),
            force: Some(true),
            name: None,
            workspace_id: None,
            detailed: None,
        }
        .call_tool(&seed_handler)
        .await?;

        handler.set_current_primary_binding(rebound_id.clone(), rebound_path);
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "rebound_only_marker".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("rebound_only_marker"),
            "primary line-mode search should use rebound session primary: {}",
            response_text
        );
        assert!(
            !response_text.contains("original_only_marker"),
            "primary line-mode search should not read stale loaded primary content: {}",
            response_text
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_reports_index_requirement_for_reference_without_tantivy()
    -> Result<()> {
        use crate::daemon::database::DaemonDatabase;
        use crate::daemon::workspace_pool::WorkspacePool;
        use crate::workspace::registry::generate_workspace_id;
        use std::sync::Arc;

        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let indexes_dir = temp_dir.path().join("indexes");
        fs::create_dir_all(&indexes_dir)?;

        let original_root = temp_dir.path().join("original-primary");
        let reference_root = temp_dir.path().join("reference-workspace");
        fs::create_dir_all(&original_root)?;
        fs::create_dir_all(&reference_root)?;
        fs::write(
            original_root.join("main.rs"),
            "fn original_workspace_only() {}\n",
        )?;
        fs::write(
            reference_root.join("lib.rs"),
            "fn reference_missing_tantivy() { println!(\"reference_missing_tantivy\"); }\n",
        )?;

        let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
            None,
            None,
        ));

        let original_path = original_root.canonicalize()?;
        let original_path_str = original_path.to_string_lossy().to_string();
        let original_id = generate_workspace_id(&original_path_str)?;
        let original_ws = pool
            .get_or_init(&original_id, original_path.clone())
            .await?;

        let handler = JulieServerHandler::new_with_shared_workspace(
            original_ws,
            original_path.clone(),
            Some(Arc::clone(&daemon_db)),
            Some(original_id.clone()),
            None,
            None,
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await?;

        daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

        let reference_path = reference_root.canonicalize()?;
        let reference_path_str = reference_path.to_string_lossy().to_string();
        let reference_id = generate_workspace_id(&reference_path_str)?;
        daemon_db.upsert_workspace(&reference_id, &reference_path_str, "ready")?;

        let reference_db_path = handler
            .get_workspace()
            .await?
            .expect("primary workspace should exist")
            .workspace_db_path(&reference_id);
        fs::create_dir_all(
            reference_db_path
                .parent()
                .expect("reference db parent should exist"),
        )?;
        let _reference_db = crate::database::SymbolDatabase::new(&reference_db_path)?;

        handler.mark_workspace_active(&reference_id).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "reference_missing_tantivy".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id.clone()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains(&format!(
                "Line-level content search requires a Tantivy index for workspace '{}'",
                reference_id
            )),
            "reference line-mode search should return a clear readiness message when Tantivy is missing: {response_text}"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_reference_db_cache_tracks_primary_root_changes() -> Result<()> {
        use crate::database::SymbolDatabase;
        use crate::database::types::FileInfo;

        let temp_dir = TempDir::new()?;
        let first_root = temp_dir.path().join("first-root");
        let second_root = temp_dir.path().join("second-root");
        fs::create_dir_all(first_root.join(".git"))?;
        fs::create_dir_all(second_root.join(".git"))?;

        let handler = JulieServerHandler::new(first_root.clone()).await?;
        handler
            .initialize_workspace_with_force(Some(first_root.to_string_lossy().to_string()), true)
            .await?;

        let ref_id = "shared-ref";
        let first_db_path = first_root
            .join(".julie")
            .join("indexes")
            .join(ref_id)
            .join("db")
            .join("symbols.db");
        fs::create_dir_all(first_db_path.parent().expect("first db parent"))?;
        let mut first_db = SymbolDatabase::new(&first_db_path)?;
        let first_file = FileInfo {
            path: "a.rs".to_string(),
            language: "rust".to_string(),
            hash: "hash-a".to_string(),
            size: 1,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: Some("fn alpha() {}".to_string()),
        };
        let first_symbol = Symbol {
            id: "alpha-id".to_string(),
            name: "alpha".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "a.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 12,
            start_byte: 0,
            end_byte: 12,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        };
        first_db.bulk_store_fresh_atomic(&[first_file], &[first_symbol], &[], &[], &[], ref_id)?;

        let first_db_handle = handler.get_database_for_workspace(ref_id).await?;
        let first_count = first_db_handle
            .lock()
            .unwrap()
            .count_symbols_for_workspace()?;
        assert_eq!(first_count, 1);

        handler
            .initialize_workspace_with_force(Some(second_root.to_string_lossy().to_string()), true)
            .await?;

        let second_db_path = second_root
            .join(".julie")
            .join("indexes")
            .join(ref_id)
            .join("db")
            .join("symbols.db");
        fs::create_dir_all(second_db_path.parent().expect("second db parent"))?;
        let mut second_db = SymbolDatabase::new(&second_db_path)?;
        let second_file = FileInfo {
            path: "b.rs".to_string(),
            language: "rust".to_string(),
            hash: "hash-b".to_string(),
            size: 1,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 2,
            line_count: 2,
            content: Some("fn beta() {}\nfn gamma() {}".to_string()),
        };
        let second_symbols = vec![
            Symbol {
                id: "beta-id".to_string(),
                name: "beta".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "b.rs".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 1,
                end_column: 11,
                start_byte: 0,
                end_byte: 11,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
                annotations: Vec::new(),
            },
            Symbol {
                id: "gamma-id".to_string(),
                name: "gamma".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "b.rs".to_string(),
                start_line: 2,
                start_column: 0,
                end_line: 2,
                end_column: 12,
                start_byte: 13,
                end_byte: 25,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
                annotations: Vec::new(),
            },
        ];
        second_db.bulk_store_fresh_atomic(
            &[second_file],
            &second_symbols,
            &[],
            &[],
            &[],
            ref_id,
        )?;

        let second_db_handle = handler.get_database_for_workspace(ref_id).await?;
        let second_count = second_db_handle
            .lock()
            .unwrap()
            .count_symbols_for_workspace()?;
        assert_eq!(
            second_count, 2,
            "reference db cache should follow the new primary root anchor instead of reusing the old cached handle"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_reference_indexing_uses_rebound_primary_storage_root() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let first_root = temp_dir.path().join("first-root");
        let second_root = temp_dir.path().join("second-root");
        let reference_root = temp_dir.path().join("reference-root");
        fs::create_dir_all(first_root.join(".git"))?;
        fs::create_dir_all(second_root.join(".git"))?;
        fs::create_dir_all(&reference_root)?;
        fs::write(first_root.join("main.rs"), "fn first_root() {}\n")?;
        fs::write(second_root.join("main.rs"), "fn second_root() {}\n")?;
        fs::write(reference_root.join("ref.rs"), "fn reference_symbol() {}\n")?;

        let handler = JulieServerHandler::new(first_root.clone()).await?;
        handler
            .initialize_workspace_with_force(Some(first_root.to_string_lossy().to_string()), true)
            .await?;
        handler
            .initialize_workspace_with_force(Some(second_root.to_string_lossy().to_string()), true)
            .await?;

        let reference_path = reference_root.canonicalize()?;
        let reference_id =
            crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(reference_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        let second_db_path = second_root
            .join(".julie")
            .join("indexes")
            .join(&reference_id)
            .join("db")
            .join("symbols.db");
        let first_db_path = first_root
            .join(".julie")
            .join("indexes")
            .join(&reference_id)
            .join("db")
            .join("symbols.db");

        assert!(
            second_db_path.exists(),
            "reference indexing should land under the rebound primary storage root"
        );
        assert!(
            !first_db_path.exists(),
            "reference indexing should not write under the stale loaded primary storage root"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_definitions_reports_index_requirement_for_reference_without_tantivy()
    -> Result<()> {
        use crate::daemon::database::DaemonDatabase;
        use crate::daemon::workspace_pool::WorkspacePool;
        use crate::workspace::registry::generate_workspace_id;
        use std::sync::Arc;

        let temp_dir = TempDir::new()?;
        let indexes_dir = temp_dir.path().join("indexes");
        fs::create_dir_all(&indexes_dir)?;

        let original_root = temp_dir.path().join("original-primary");
        let reference_root = temp_dir.path().join("reference-workspace");
        fs::create_dir_all(&original_root)?;
        fs::create_dir_all(&reference_root)?;
        fs::write(
            original_root.join("main.rs"),
            "fn original_workspace_only() {}\n",
        )?;
        fs::write(
            reference_root.join("lib.rs"),
            "fn definition_missing_tantivy() {}\n",
        )?;

        let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
            None,
            None,
        ));

        let original_path = original_root.canonicalize()?;
        let original_path_str = original_path.to_string_lossy().to_string();
        let original_id = generate_workspace_id(&original_path_str)?;
        let original_ws = pool
            .get_or_init(&original_id, original_path.clone())
            .await?;

        let handler = JulieServerHandler::new_with_shared_workspace(
            original_ws,
            original_path.clone(),
            Some(Arc::clone(&daemon_db)),
            Some(original_id.clone()),
            None,
            None,
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await?;

        daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

        let reference_path = reference_root.canonicalize()?;
        let reference_path_str = reference_path.to_string_lossy().to_string();
        let reference_id = generate_workspace_id(&reference_path_str)?;
        daemon_db.upsert_workspace(&reference_id, &reference_path_str, "ready")?;

        let reference_db_path = handler.workspace_db_file_path_for(&reference_id).await?;
        fs::create_dir_all(
            reference_db_path
                .parent()
                .expect("reference db parent should exist"),
        )?;
        let mut reference_db = crate::database::SymbolDatabase::new(&reference_db_path)?;
        let reference_file = crate::database::types::FileInfo {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "ref-hash".to_string(),
            size: 1,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: Some("fn definition_missing_tantivy() {}\n".to_string()),
        };
        let reference_symbol = Symbol {
            id: "definition-missing-id".to_string(),
            name: "definition_missing_tantivy".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 31,
            start_byte: 0,
            end_byte: 31,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        };
        reference_db.bulk_store_fresh_atomic(
            &[reference_file],
            &[reference_symbol],
            &[],
            &[],
            &[],
            &reference_id,
        )?;

        handler.mark_workspace_active(&reference_id).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "definition_missing_tantivy".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id.clone()),
            search_target: "definitions".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains(&format!(
                "Definition search requires a Tantivy index for workspace '{}'",
                reference_id
            )),
            "definition search should return a clear readiness message when Tantivy is missing: {response_text}"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_reports_index_requirement_for_loaded_primary_without_tantivy()
    -> Result<()> {
        let (_temp_dir, handler) = setup_loaded_primary_without_tantivy().await?;

        let search_tool = FastSearchTool {
            query: "loaded_primary_missing_tantivy".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("Line-level content search requires a Tantivy index for the current primary workspace"),
            "loaded primary line-mode search should return an explicit Tantivy-required message: {response_text}"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_definitions_reports_index_requirement_for_loaded_primary_without_tantivy()
    -> Result<()> {
        let (_temp_dir, handler) = setup_loaded_primary_without_tantivy().await?;

        let search_tool = FastSearchTool {
            query: "loaded_primary_missing_tantivy".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "definitions".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains(
                "Definition search requires a Tantivy index for the current primary workspace"
            ),
            "loaded primary definition search should return an explicit Tantivy-required message: {response_text}"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_primary_rejects_neutral_gap_without_primary_identity() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();
        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;
        fs::write(
            src_dir.join("example.rs"),
            "pub fn neutral_gap_search_target() {}\n",
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        handler.publish_loaded_workspace_swap_intent_for_test();

        let search_tool = FastSearchTool {
            query: "neutral_gap_search_target".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let err = search_tool
            .call_tool(&handler)
            .await
            .expect_err("neutral gap should reject primary fast_search requests");

        assert!(
            err.to_string()
                .contains("Primary workspace identity unavailable during swap"),
            "unexpected error: {err:#}"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_primary_cold_start_reports_index_first_instead_of_swap_gap()
    -> Result<()> {
        let handler = JulieServerHandler::new_for_test().await?;

        let search_tool = FastSearchTool {
            query: "cold_start_primary_search_target".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains(
                "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first."
            ),
            "cold-start primary search should preserve the normal index-first guidance: {response_text}"
        );
        assert!(
            !response_text.contains("Primary workspace identity unavailable during swap"),
            "cold-start primary search should not be classified as a swap gap: {response_text}"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_primary_rejects_swap_in_progress_after_partial_publish() -> Result<()>
    {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let rebound_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();
        let rebound_path = rebound_dir.path().canonicalize()?;
        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;
        fs::write(
            src_dir.join("example.rs"),
            "pub fn partial_publish_search_target() {}\n",
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        let rebound_id =
            crate::workspace::registry::generate_workspace_id(&rebound_path.to_string_lossy())?;

        handler.publish_loaded_workspace_swap_intent_for_test();
        handler.set_current_primary_binding(rebound_id, rebound_path);

        let search_tool = FastSearchTool {
            query: "partial_publish_search_target".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let err = search_tool
            .call_tool(&handler)
            .await
            .expect_err("swap-in-progress should reject primary fast_search after partial publish");

        assert!(
            err.to_string()
                .contains("Primary workspace identity unavailable during swap"),
            "unexpected error: {err:#}"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_handles_exclusion_queries() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        let test_file = src_dir.join("filters.rs");
        fs::write(
            &test_file,
            r#"// user profile data
// user password secret
// user preferences dashboard
"#,
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "user -password".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        // Verify correct lines are included
        assert!(
            response_text.contains("user profile data"),
            "Should include line without excluded term: {}",
            response_text
        );
        assert!(
            response_text.contains("user preferences dashboard"),
            "Should include other matching line: {}",
            response_text
        );

        // Verify excluded line is NOT in results (check line content, not header)
        assert!(
            !response_text.contains("user password secret"),
            "Should exclude lines containing the forbidden term: {}",
            response_text
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_symbols_mode_default() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        let test_file = src_dir.join("example.rs");
        fs::write(
            &test_file,
            r#"pub fn getUserData() -> User {
    User { name: "test" }
}
"#,
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "getUserData".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("getUserData"),
            "Should find function symbol"
        );
        assert!(
            response_text.contains("getUserData")
                || response_text.contains("Found")
                || response_text.contains("symbol"),
            "Should show basic search result info"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_language_filter() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // Create files in different languages with common search term
        let rust_file = src_dir.join("example.rs");
        fs::write(
            &rust_file,
            r#"// TODO: implement feature
fn rust_function() {}
"#,
        )?;

        let ts_file = src_dir.join("example.ts");
        fs::write(
            &ts_file,
            r#"// TODO: implement feature
function typescriptFunction() {}
"#,
        )?;

        let py_file = src_dir.join("example.py");
        fs::write(
            &py_file,
            r#"# TODO: implement feature
def python_function():
    pass
"#,
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        // Test: Search with rust language filter
        let search_rust = FastSearchTool {
            query: "TODO".to_string(),
            language: Some("rust".to_string()),
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_rust.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("example.rs"),
            "Should find TODO in Rust file"
        );
        assert!(
            !response_text.contains("example.ts"),
            "Should NOT include TypeScript file when filtering for Rust"
        );
        assert!(
            !response_text.contains("example.py"),
            "Should NOT include Python file when filtering for Rust"
        );

        // Test: Search with typescript language filter
        let search_ts = FastSearchTool {
            query: "TODO".to_string(),
            language: Some("typescript".to_string()),
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result_ts = search_ts.call_tool(&handler).await?;
        let response_ts = extract_text_from_result(&result_ts);

        assert!(
            response_ts.contains("example.ts"),
            "Should find TODO in TypeScript file"
        );
        assert!(
            !response_ts.contains("example.rs"),
            "Should NOT include Rust file when filtering for TypeScript"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_file_pattern_filter() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        // Create directory structure
        let src_dir = workspace_path.join("src");
        let tests_dir = workspace_path.join("tests");
        fs::create_dir_all(&src_dir)?;
        fs::create_dir_all(&tests_dir)?;

        // Create files with common search term in different locations
        let src_file = src_dir.join("code.rs");
        fs::write(&src_file, "// FIXME: handle error\n")?;

        let test_file = tests_dir.join("test.rs");
        fs::write(&test_file, "// FIXME: add test case\n")?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_secs(2)).await; // Increased wait for FTS content indexing
        mark_index_ready(&handler).await;

        // Test: Search with src/** file pattern
        let search_src = FastSearchTool {
            query: "FIXME".to_string(),
            language: None,
            file_pattern: Some("src/**".to_string()),
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_src.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("src/code.rs") || response_text.contains("src\\code.rs"),
            "Should find FIXME in src/ directory: {}",
            response_text
        );
        assert!(
            !response_text.contains("tests/test.rs") && !response_text.contains("tests\\test.rs"),
            "Should NOT include tests/ directory when filtering for src/**"
        );

        // Test: Search with tests/** file pattern
        let search_tests = FastSearchTool {
            query: "FIXME".to_string(),
            language: None,
            file_pattern: Some("tests/**".to_string()),
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result_tests = search_tests.call_tool(&handler).await?;
        let response_tests = extract_text_from_result(&result_tests);

        assert!(
            response_tests.contains("tests/test.rs") || response_tests.contains("tests\\test.rs"),
            "Should find FIXME in tests/ directory"
        );
        assert!(
            !response_tests.contains("src/code.rs") && !response_tests.contains("src\\code.rs"),
            "Should NOT include src/ directory when filtering for tests/**"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_exclude_tests() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        // Create production source file
        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        let prod_file = src_dir.join("auth.rs");
        fs::write(
            &prod_file,
            r#"/// Authenticate a user with the given credentials
fn authenticate_user(username: &str, password: &str) -> bool {
    // authenticate logic here
    username.len() > 0 && password.len() > 0
}
"#,
        )?;

        // Create test file in a test directory (is_test_path checks for "tests" segment)
        let test_dir = workspace_path.join("src").join("tests");
        fs::create_dir_all(&test_dir)?;

        let test_file = test_dir.join("auth_test.rs");
        fs::write(
            &test_file,
            r#"#[test]
fn test_authenticate_user() {
    // authenticate test logic
    assert!(authenticate_user("admin", "secret"));
}
"#,
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_secs(2)).await;
        mark_index_ready(&handler).await;

        // Test 1: Search WITHOUT exclude_tests — should find results from BOTH files
        let search_all = FastSearchTool {
            query: "authenticate".to_string(),
            language: None,
            file_pattern: None,
            limit: 20,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result_all = search_all.call_tool(&handler).await?;
        let text_all = extract_text_from_result(&result_all);

        assert!(
            text_all.contains("src/auth.rs"),
            "Without exclude_tests, should find production file. Got: {}",
            text_all
        );
        assert!(
            text_all.contains("src/tests/auth_test.rs"),
            "Without exclude_tests, should find test file. Got: {}",
            text_all
        );

        // Test 2: Search WITH exclude_tests: Some(true) — should ONLY find production file
        let search_no_tests = FastSearchTool {
            query: "authenticate".to_string(),
            language: None,
            file_pattern: None,
            limit: 20,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: Some(true),
            ..Default::default()
        };

        let result_no_tests = search_no_tests.call_tool(&handler).await?;
        let text_no_tests = extract_text_from_result(&result_no_tests);

        assert!(
            text_no_tests.contains("src/auth.rs"),
            "With exclude_tests, should still find production file. Got: {}",
            text_no_tests
        );
        assert!(
            !text_no_tests.contains("src/tests/auth_test.rs"),
            "With exclude_tests, should NOT find test file. Got: {}",
            text_no_tests
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_combined_filters() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // Create multiple files
        let rust_file = src_dir.join("main.rs");
        fs::write(&rust_file, "// TODO: rust implementation\n")?;

        let ts_file = src_dir.join("index.ts");
        fs::write(&ts_file, "// TODO: typescript implementation\n")?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_secs(2)).await; // Increased wait for FTS content indexing
        mark_index_ready(&handler).await;

        // Test: Search with BOTH language AND file_pattern filters
        let search_combined = FastSearchTool {
            query: "TODO".to_string(),
            language: Some("rust".to_string()),
            file_pattern: Some("src/**/*.rs".to_string()),
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_combined.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("main.rs"),
            "Should find TODO in Rust file matching both filters"
        );
        assert!(
            !response_text.contains("index.ts"),
            "Should NOT include TypeScript file when filtering for Rust + src/**/*.rs"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_primary_wrapper_resolves_roots_before_searching() -> Result<()> {
        use crate::daemon::database::DaemonDatabase;
        use crate::daemon::workspace_pool::WorkspacePool;
        use crate::workspace::registry::generate_workspace_id;

        let temp_dir = TempDir::new()?;
        let indexes_dir = temp_dir.path().join("indexes");
        fs::create_dir_all(&indexes_dir)?;

        let startup_root = temp_dir.path().join("startup-primary");
        let roots_root = temp_dir.path().join("roots-primary");
        fs::create_dir_all(startup_root.join("src"))?;
        fs::create_dir_all(roots_root.join("src"))?;
        fs::write(startup_root.join("src/old.rs"), "fn old_root_only() {}\n")?;
        fs::write(
            roots_root.join("src/rebound.rs"),
            "pub fn rebound_search_symbol() {}\n",
        )?;

        let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
            None,
            None,
        ));

        let startup_path = startup_root.canonicalize()?;
        let startup_id = generate_workspace_id(&startup_path.to_string_lossy())?;
        let startup_ws = pool.get_or_init(&startup_id, startup_path.clone()).await?;

        let roots_path = roots_root.canonicalize()?;
        let roots_id = generate_workspace_id(&roots_path.to_string_lossy())?;
        daemon_db.upsert_workspace(&startup_id, &startup_path.to_string_lossy(), "ready")?;
        daemon_db.upsert_workspace(&roots_id, &roots_path.to_string_lossy(), "ready")?;
        let roots_ws = pool.get_or_init(&roots_id, roots_path.clone()).await?;
        {
            let rebound_db = roots_ws.db.as_ref().unwrap().clone();
            let mut rebound_db = rebound_db.lock().unwrap();
            let file_info = crate::database::types::FileInfo {
                path: "src/rebound.rs".to_string(),
                language: "rust".to_string(),
                hash: "roots-search-hash".to_string(),
                size: 1,
                last_modified: 1,
                last_indexed: 1,
                symbol_count: 1,
                line_count: 1,
                content: Some("pub fn rebound_search_symbol() {}\n".to_string()),
            };
            let symbol = Symbol {
                id: "roots-search-symbol-id".to_string(),
                name: "rebound_search_symbol".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/rebound.rs".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 1,
                end_column: 31,
                start_byte: 0,
                end_byte: 31,
                signature: Some("fn rebound_search_symbol()".to_string()),
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: Some("pub fn rebound_search_symbol() {}".to_string()),
                content_type: None,
                annotations: Vec::new(),
            };
            rebound_db.bulk_store_fresh_atomic(
                &[file_info],
                &[symbol],
                &[],
                &[],
                &[],
                &roots_id,
            )?;
        }

        let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
            startup_ws,
            crate::workspace::startup_hint::WorkspaceStartupHint {
                path: startup_path.clone(),
                source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
            },
            Some(Arc::clone(&daemon_db)),
            Some(startup_id.clone()),
            None,
            None,
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await?;
        handler.set_client_supports_workspace_roots_for_test(true);

        let (server_transport, client_transport) = tokio::io::duplex(256);
        let service =
            serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
        let (read_half, mut write_half) = tokio::io::split(client_transport);
        let mut lines = BufReader::new(read_half).lines();

        let roots_reply = async {
            match read_server_message(&mut lines).await {
                ServerJsonRpcMessage::Request(request) => match request.request {
                    ServerRequest::ListRootsRequest(_) => {
                        send_json_line(
                            &mut write_half,
                            &serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": request.id,
                                "result": {
                                    "roots": [{ "uri": format!("file://{}", roots_path.to_string_lossy()) }]
                                }
                            }),
                        )
                        .await;
                    }
                    other => panic!("unexpected server request: {other:?}"),
                },
                other => panic!("unexpected server message: {other:?}"),
            }
        };

        let search = <JulieServerHandler as ServerHandler>::call_tool(
            &handler,
            CallToolRequestParams::new("fast_search").with_arguments(
                serde_json::json!({
                    "query": "rebound_search_symbol",
                    "workspace": "primary",
                    "search_target": "content",
                    "limit": 10
                })
                .as_object()
                .expect("fast_search args")
                .clone(),
            ),
            RequestContext::new(NumberOrString::Number(31), service.peer().clone()),
        );
        let (_, result) = tokio::join!(roots_reply, search);
        let response_text = extract_text_from_result(&result?);

        assert!(
            response_text.contains("No lines found matching")
                || response_text.contains("Line-level content search requires a Tantivy index for the current primary workspace"),
            "fast_search should resolve roots first and produce a normal roots-bound search response: {response_text}"
        );
        assert_eq!(handler.current_workspace_id(), Some(roots_id));

        drop(write_half);
        drop(lines);
        let _ = service.cancel().await;
        Ok(())
    }
}
