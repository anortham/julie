//! End-to-end federation integration tests.
//!
//! These tests verify the **full MCP tool call stack** for federated search:
//! construct tool structs with `workspace: Some("all")`, call `call_tool()`
//! on a handler backed by a `DaemonState` with multiple Ready workspaces,
//! and verify the output string contains results from multiple projects.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
    use crate::database::types::FileInfo;
    use crate::database::SymbolDatabase;
    use crate::extractors::base::SymbolKind;
    use crate::extractors::Symbol;
    use crate::handler::JulieServerHandler;
    use crate::mcp_compat::CallToolResult;
    use crate::search::index::{SearchIndex, SymbolDocument};
    use crate::workspace::JulieWorkspace;

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Extract text content from a CallToolResult for assertions.
    ///
    /// Uses serde_json to extract the "text" field from Content blocks,
    /// since Content is an rmcp type without a direct text accessor.
    fn extract_text(result: &CallToolResult) -> String {
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

    /// Create a test symbol for insertion into the database.
    fn make_symbol(
        name: &str,
        file_path: &str,
        line: u32,
        kind: SymbolKind,
        sig: Option<&str>,
    ) -> Symbol {
        Symbol {
            id: format!("sym_{}_{}", file_path, line),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 200,
            parent_id: None,
            signature: sig.map(|s| s.to_string()),
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: Some(format!("fn {}() {{}}", name)),
            content_type: None,
        }
    }

    /// Insert a file record into the database so symbols can be stored.
    fn store_test_file(db: &SymbolDatabase, path: &str) {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: "rust".to_string(),
            hash: format!("hash_{}", path),
            size: 500,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 1,
            content: None,
        })
        .unwrap();
    }

    /// Create a fully populated test workspace with both DB and SearchIndex
    /// containing the given symbols. Returns (JulieWorkspace, TempDir).
    ///
    /// Each symbol is inserted into both SQLite and Tantivy so that the
    /// full tool pipeline (search -> DB lookup) works end to end.
    fn create_indexed_workspace(
        symbols: Vec<(&str, &str, u32, SymbolKind, &str)>, // (name, file_path, line, kind, signature)
    ) -> (JulieWorkspace, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("symbols.db");
        let tantivy_path = dir.path().join("tantivy");
        std::fs::create_dir_all(&tantivy_path).unwrap();

        // Create and populate DB
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        // Collect unique file paths for FK insertion
        let unique_files: std::collections::HashSet<&str> =
            symbols.iter().map(|(_, fp, _, _, _)| *fp).collect();
        for fp in &unique_files {
            store_test_file(&db, fp);
        }

        let db_symbols: Vec<Symbol> = symbols
            .iter()
            .map(|(name, fp, line, kind, sig)| {
                make_symbol(name, fp, *line, kind.clone(), Some(sig))
            })
            .collect();
        db.store_symbols(&db_symbols).unwrap();

        // Create and populate SearchIndex
        let search_index = SearchIndex::create(&tantivy_path).unwrap();
        for (name, file_path, line, kind, sig) in &symbols {
            let doc = SymbolDocument {
                id: format!("sym_{}_{}", file_path, line),
                name: name.to_string(),
                kind: format!("{:?}", kind).to_lowercase(),
                file_path: file_path.to_string(),
                language: "rust".to_string(),
                signature: sig.to_string(),
                doc_comment: String::new(),
                code_body: format!("fn {}() {{}}", name),
                start_line: *line,
            };
            search_index.add_symbol(&doc).unwrap();
        }
        search_index.commit().unwrap();

        let mut workspace = JulieWorkspace::empty_shell(dir.path().to_path_buf());
        workspace.db = Some(Arc::new(std::sync::Mutex::new(db)));
        workspace.search_index = Some(Arc::new(std::sync::Mutex::new(search_index)));

        (workspace, dir)
    }

    /// Build a DaemonState with the given named workspaces and return a
    /// JulieServerHandler wired to that state.
    async fn build_handler_with_workspaces(
        workspaces: Vec<(String, JulieWorkspace, PathBuf)>,
    ) -> JulieServerHandler {
        let ds = Arc::new(RwLock::new(DaemonState::new()));
        {
            let mut state = ds.write().await;
            for (ws_id, workspace, path) in workspaces {
                state.workspaces.insert(
                    ws_id,
                    LoadedWorkspace {
                        workspace,
                        status: WorkspaceLoadStatus::Ready,
                        path,
                    },
                );
            }
        }
        JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/federation-test"), ds)
            .unwrap()
    }

    // =========================================================================
    // Test 1: Federated fast_search finds symbols from two projects
    // =========================================================================

    #[tokio::test]
    async fn test_federated_fast_search_finds_symbols_from_two_projects() {
        use crate::tools::FastSearchTool;

        let (ws_a, tmp_a) = create_indexed_workspace(vec![(
            "DatabaseEngine",
            "src/engine.rs",
            10,
            SymbolKind::Struct,
            "pub struct DatabaseEngine",
        )]);

        let (ws_b, tmp_b) = create_indexed_workspace(vec![(
            "SearchEngine",
            "src/search.rs",
            20,
            SymbolKind::Struct,
            "pub struct SearchEngine",
        )]);

        let handler = build_handler_with_workspaces(vec![
            ("project_alpha".to_string(), ws_a, tmp_a.path().to_path_buf()),
            ("project_beta".to_string(), ws_b, tmp_b.path().to_path_buf()),
        ])
        .await;

        let tool = FastSearchTool {
            query: "Engine".to_string(),
            search_target: "definitions".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            context_lines: Some(0),
            workspace: Some("all".to_string()),
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_ok(), "call_tool should succeed: {:?}", result);

        let text = extract_text(&result.unwrap());

        // Verify results contain file paths from both workspaces
        assert!(
            text.contains("src/engine.rs"),
            "Should find result from project_alpha (src/engine.rs). Got:\n{}",
            text
        );
        assert!(
            text.contains("src/search.rs"),
            "Should find result from project_beta (src/search.rs). Got:\n{}",
            text
        );

        // Verify project attribution is present
        assert!(
            text.contains("[project:"),
            "Should have project attribution tags. Got:\n{}",
            text
        );
    }

    // =========================================================================
    // Test 2: Federated fast_refs finds cross-project references
    // =========================================================================

    #[tokio::test]
    async fn test_federated_fast_refs_finds_cross_project_references() {
        use crate::tools::FastRefsTool;

        let (ws_a, tmp_a) = create_indexed_workspace(vec![(
            "Tokenizer",
            "src/tokenizer.rs",
            5,
            SymbolKind::Struct,
            "pub struct Tokenizer",
        )]);

        let (ws_b, tmp_b) = create_indexed_workspace(vec![(
            "Tokenizer",
            "lib/tokenizer.py",
            15,
            SymbolKind::Class,
            "class Tokenizer",
        )]);

        let handler = build_handler_with_workspaces(vec![
            ("rust_project".to_string(), ws_a, tmp_a.path().to_path_buf()),
            ("python_project".to_string(), ws_b, tmp_b.path().to_path_buf()),
        ])
        .await;

        let tool = FastRefsTool {
            symbol: "Tokenizer".to_string(),
            include_definition: true,
            limit: 10,
            workspace: Some("all".to_string()),
            reference_kind: None,
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_ok(), "call_tool should succeed: {:?}", result);

        let text = extract_text(&result.unwrap());

        // Should find definitions from both projects
        assert!(
            text.contains("src/tokenizer.rs") || text.contains("lib/tokenizer.py"),
            "Should find Tokenizer in at least one project. Got:\n{}",
            text
        );
    }

    // =========================================================================
    // Test 3: Federated deep_dive across projects
    // =========================================================================

    #[tokio::test]
    async fn test_federated_deep_dive_across_projects() {
        use crate::tools::DeepDiveTool;

        let (ws_a, tmp_a) = create_indexed_workspace(vec![(
            "QueryBuilder",
            "src/query.rs",
            25,
            SymbolKind::Struct,
            "pub struct QueryBuilder",
        )]);

        let (ws_b, tmp_b) = create_indexed_workspace(vec![(
            "QueryBuilder",
            "src/builder.rs",
            50,
            SymbolKind::Class,
            "class QueryBuilder",
        )]);

        let handler = build_handler_with_workspaces(vec![
            ("backend".to_string(), ws_a, tmp_a.path().to_path_buf()),
            ("frontend".to_string(), ws_b, tmp_b.path().to_path_buf()),
        ])
        .await;

        let tool = DeepDiveTool {
            symbol: "QueryBuilder".to_string(),
            depth: "overview".to_string(),
            context_file: None,
            workspace: Some("all".to_string()),
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_ok(), "call_tool should succeed: {:?}", result);

        let text = extract_text(&result.unwrap());

        // Should find the symbol and include project attribution
        assert!(
            text.contains("QueryBuilder"),
            "Should find QueryBuilder. Got:\n{}",
            text
        );
        assert!(
            text.contains("[project:"),
            "Should have project tag in output. Got:\n{}",
            text
        );
    }

    // =========================================================================
    // Test 4: Federated get_context returns multi-project pivots
    // =========================================================================

    #[tokio::test]
    async fn test_federated_get_context_returns_multi_project_pivots() {
        use crate::tools::GetContextTool;

        let (ws_a, tmp_a) = create_indexed_workspace(vec![(
            "IndexWriter",
            "src/index.rs",
            30,
            SymbolKind::Struct,
            "pub struct IndexWriter",
        )]);

        let (ws_b, tmp_b) = create_indexed_workspace(vec![(
            "IndexReader",
            "src/reader.rs",
            40,
            SymbolKind::Struct,
            "pub struct IndexReader",
        )]);

        let handler = build_handler_with_workspaces(vec![
            ("writer_project".to_string(), ws_a, tmp_a.path().to_path_buf()),
            ("reader_project".to_string(), ws_b, tmp_b.path().to_path_buf()),
        ])
        .await;

        let tool = GetContextTool {
            query: "Index".to_string(),
            max_tokens: Some(2000),
            workspace: Some("all".to_string()),
            language: None,
            file_pattern: None,
            format: None,
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_ok(), "call_tool should succeed: {:?}", result);

        let text = extract_text(&result.unwrap());

        // Should contain project headers
        assert!(
            text.contains("[project:"),
            "Should have project headers. Got:\n{}",
            text
        );
        // The query "Index" should match IndexWriter and/or IndexReader
        let has_writer = text.contains("IndexWriter");
        let has_reader = text.contains("IndexReader");
        assert!(
            has_writer || has_reader,
            "Should find at least one Index* symbol. Got:\n{}",
            text
        );
    }
}
