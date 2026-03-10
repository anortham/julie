//! Tests for federated fast_refs: cross-project reference search.
//!
//! Tests cover:
//! - Federated formatting (format_federated_refs_results)
//! - Stdio mode error (no daemon_state)
//! - Empty daemon state (no Ready workspaces)
//! - Real DB integration with Ready workspaces

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use tokio_util::sync::CancellationToken;

    use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
    use crate::database::FileInfo;
    use crate::extractors::base::{RelationshipKind, SymbolKind};
    use crate::extractors::{Relationship, Symbol};
    use crate::handler::JulieServerHandler;
    use crate::registry::GlobalRegistry;

    /// Create a test DaemonState wrapped in Arc<RwLock<>>.
    fn test_daemon_state() -> Arc<RwLock<DaemonState>> {
        let registry = Arc::new(RwLock::new(GlobalRegistry::new()));
        let ct = CancellationToken::new();
        let julie_home = PathBuf::from("/tmp/test-julie-home");
        Arc::new(RwLock::new(DaemonState::new(registry, julie_home, ct)))
    }
    use crate::mcp_compat::CallToolResult;
    use crate::tools::navigation::formatting::{
        format_federated_refs_results, ProjectTaggedResult,
    };

    /// Insert a file record into the database so symbols can be stored.
    fn store_test_file(db: &crate::database::SymbolDatabase, path: &str) {
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

    /// Extract text from a CallToolResult for assertions.
    fn extract_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // =========================================================================
    // Helper: create test symbols and relationships
    // =========================================================================

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
            end_line: line + 5,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: None,
            signature: sig.map(|s| s.to_string()),
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    fn make_relationship(file_path: &str, line: u32, kind: RelationshipKind) -> Relationship {
        make_relationship_with_confidence(file_path, line, kind, 1.0)
    }

    fn make_relationship_with_confidence(
        file_path: &str,
        line: u32,
        kind: RelationshipKind,
        confidence: f32,
    ) -> Relationship {
        Relationship {
            id: format!("rel_{}_{}", file_path, line),
            from_symbol_id: "caller".to_string(),
            to_symbol_id: "target".to_string(),
            kind,
            file_path: file_path.to_string(),
            line_number: line,
            confidence,
            metadata: None,
        }
    }

    // =========================================================================
    // Formatting tests: format_federated_refs_results
    // =========================================================================

    #[test]
    fn test_federated_format_multiple_projects() {
        let backend_defs = vec![make_symbol(
            "UserService",
            "src/services/user.rs",
            15,
            SymbolKind::Struct,
            Some("pub struct UserService"),
        )];
        let backend_refs = vec![
            make_relationship("src/api/auth.rs", 42, RelationshipKind::Calls),
            make_relationship("src/handlers/login.rs", 55, RelationshipKind::Calls),
        ];

        let frontend_defs = vec![];
        let frontend_refs = vec![
            make_relationship("src/api/client.ts", 28, RelationshipKind::Calls),
            make_relationship("src/hooks/useUser.ts", 5, RelationshipKind::Imports),
        ];

        let tagged = vec![
            ProjectTaggedResult {
                project_name: "backend",
                definitions: &backend_defs,
                references: &backend_refs,
            },
            ProjectTaggedResult {
                project_name: "frontend",
                definitions: &frontend_defs,
                references: &frontend_refs,
            },
        ];

        let output = format_federated_refs_results("UserService", &tagged);

        // Header with project count
        assert!(
            output.contains("5 references to \"UserService\" across 2 projects:"),
            "Should show total count and project count. Got:\n{}",
            output
        );
        // Project tags
        assert!(
            output.contains("[project: backend]"),
            "Should tag backend project. Got:\n{}",
            output
        );
        assert!(
            output.contains("[project: frontend]"),
            "Should tag frontend project. Got:\n{}",
            output
        );
        // Backend definition
        assert!(
            output.contains("src/services/user.rs:15 (struct)"),
            "Should show backend definition"
        );
        // Backend references
        assert!(output.contains("src/api/auth.rs:42 (Calls)"));
        // Frontend references
        assert!(output.contains("src/api/client.ts:28 (Calls)"));
        assert!(output.contains("src/hooks/useUser.ts:5 (Imports)"));
    }

    #[test]
    fn test_federated_format_single_project() {
        let defs = vec![make_symbol(
            "Config",
            "src/config.rs",
            10,
            SymbolKind::Struct,
            Some("pub struct Config"),
        )];
        let refs = vec![make_relationship(
            "src/main.rs",
            5,
            RelationshipKind::References,
        )];

        let tagged = vec![ProjectTaggedResult {
            project_name: "my-project",
            definitions: &defs,
            references: &refs,
        }];

        let output = format_federated_refs_results("Config", &tagged);

        assert!(
            output.contains("2 references to \"Config\" across 1 project:"),
            "Single project should use singular. Got:\n{}",
            output
        );
        assert!(output.contains("[project: my-project]"));
    }

    #[test]
    fn test_federated_format_no_results() {
        let output = format_federated_refs_results("Unknown", &[]);
        assert_eq!(output, "No references found for \"Unknown\"");
    }

    #[test]
    fn test_federated_format_skips_empty_projects() {
        let empty_defs: Vec<Symbol> = vec![];
        let empty_refs: Vec<Relationship> = vec![];
        let nonempty_defs = vec![make_symbol(
            "Foo",
            "src/foo.rs",
            1,
            SymbolKind::Function,
            None,
        )];

        let tagged = vec![
            ProjectTaggedResult {
                project_name: "empty-project",
                definitions: &empty_defs,
                references: &empty_refs,
            },
            ProjectTaggedResult {
                project_name: "real-project",
                definitions: &nonempty_defs,
                references: &empty_refs,
            },
        ];

        let output = format_federated_refs_results("Foo", &tagged);

        // Empty project should be skipped
        assert!(
            !output.contains("[project: empty-project]"),
            "Empty project should not appear. Got:\n{}",
            output
        );
        assert!(output.contains("[project: real-project]"));
    }

    #[test]
    fn test_federated_format_with_imports() {
        let defs = vec![
            make_symbol(
                "Logger",
                "src/logger.rs",
                10,
                SymbolKind::Struct,
                Some("pub struct Logger"),
            ),
            make_symbol("Logger", "src/main.rs", 3, SymbolKind::Import, None),
        ];
        let refs = vec![];

        let tagged = vec![ProjectTaggedResult {
            project_name: "app",
            definitions: &defs,
            references: &refs,
        }];

        let output = format_federated_refs_results("Logger", &tagged);

        assert!(
            output.contains("Definition:"),
            "Should show definition section. Got:\n{}",
            output
        );
        assert!(
            output.contains("Import:"),
            "Should show import section. Got:\n{}",
            output
        );
    }

    // =========================================================================
    // Federated fast_refs: stdio mode error
    // =========================================================================

    #[tokio::test]
    async fn test_federated_refs_errors_in_stdio_mode() {
        // In stdio mode, handler.daemon_state is None.
        let handler = JulieServerHandler::new_for_test().await.unwrap();
        assert!(
            handler.daemon_state.is_none(),
            "Test handler should have no daemon_state"
        );

        let result = crate::tools::navigation::federated_refs::find_refs_federated(
            &handler,
            "SomeSymbol",
            true,
            None,
            10,
        )
        .await;

        assert!(result.is_err(), "Should error in stdio mode");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("daemon mode"),
            "Error should mention daemon mode. Got: {}",
            err_msg
        );
    }

    // =========================================================================
    // Federated fast_refs: empty daemon state
    // =========================================================================

    #[tokio::test]
    async fn test_federated_refs_empty_daemon_returns_no_results() {
        // Create a handler with an empty daemon state (no workspaces).
        let ds = test_daemon_state();
        let handler =
            JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/test"), ds).unwrap();

        let result = crate::tools::navigation::federated_refs::find_refs_federated(
            &handler,
            "SomeSymbol",
            true,
            None,
            10,
        )
        .await;

        assert!(result.is_ok(), "Should succeed but return empty: {:?}", result);
        let text = extract_text(&result.unwrap());
        assert!(
            text.contains("No references found") && text.contains("no Ready projects"),
            "Should indicate no Ready projects. Got: {}",
            text
        );
    }

    // =========================================================================
    // Federated fast_refs: skips non-Ready workspaces
    // =========================================================================

    #[tokio::test]
    async fn test_federated_refs_skips_non_ready_workspaces() {
        use crate::workspace::JulieWorkspace;

        let ds = test_daemon_state();

        // Add a Registered workspace (no DB, not Ready)
        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "not_ready_ws".to_string(),
                LoadedWorkspace {
                    workspace: JulieWorkspace::empty_shell(PathBuf::from("/tmp/not-ready")),
                    status: WorkspaceLoadStatus::Registered,
                    path: PathBuf::from("/tmp/not-ready"),
                },
            );
        }

        let handler =
            JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/test"), ds).unwrap();

        let result = crate::tools::navigation::federated_refs::find_refs_federated(
            &handler,
            "SomeSymbol",
            true,
            None,
            10,
        )
        .await;

        assert!(result.is_ok());
        let text = extract_text(&result.unwrap());
        assert!(
            text.contains("no Ready projects"),
            "Should skip non-Ready workspace. Got: {}",
            text
        );
    }

    // =========================================================================
    // Federated fast_refs: Ready workspace with real DB
    // =========================================================================

    #[tokio::test]
    async fn test_federated_refs_queries_ready_workspace() {
        use crate::database::SymbolDatabase;
        use crate::workspace::JulieWorkspace;
        use std::sync::Mutex;
        use tempfile::TempDir;

        // Create a temp database and insert a symbol
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("symbols.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        store_test_file(&db, "src/handler.rs");

        // Insert a test symbol
        let test_symbol = make_symbol(
            "MyHandler",
            "src/handler.rs",
            10,
            SymbolKind::Struct,
            Some("pub struct MyHandler"),
        );
        db.store_symbols(&[test_symbol.clone()]).unwrap();

        // Create workspace with the DB
        let mut workspace = JulieWorkspace::empty_shell(tmp.path().to_path_buf());
        workspace.db = Some(Arc::new(Mutex::new(db)));

        // Set up daemon state with a Ready workspace
        let ds = test_daemon_state();
        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "test_project".to_string(),
                LoadedWorkspace {
                    workspace,
                    status: WorkspaceLoadStatus::Ready,
                    path: tmp.path().to_path_buf(),
                },
            );
        }

        let handler = JulieServerHandler::new_with_daemon_state(
            PathBuf::from("/tmp/test"),
            ds,
        )
        .unwrap();

        let result = crate::tools::navigation::federated_refs::find_refs_federated(
            &handler,
            "MyHandler",
            true,
            None,
            10,
        )
        .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result);
        let text = extract_text(&result.unwrap());

        // Should find the definition in the test workspace
        assert!(
            text.contains("src/handler.rs:10"),
            "Should find MyHandler definition. Got:\n{}",
            text
        );
        assert!(
            text.contains("[project:"),
            "Should have project tag. Got:\n{}",
            text
        );
        assert!(
            text.contains("MyHandler"),
            "Should mention the symbol. Got:\n{}",
            text
        );
    }

    // =========================================================================
    // Federated fast_refs: Multiple Ready workspaces
    // =========================================================================

    #[tokio::test]
    async fn test_federated_refs_multiple_ready_workspaces() {
        use crate::database::SymbolDatabase;
        use crate::workspace::JulieWorkspace;
        use std::sync::Mutex;
        use tempfile::TempDir;

        // Create two workspaces with different symbols

        // Workspace A: has "Parser" definition
        let tmp_a = TempDir::new().unwrap();
        let db_path_a = tmp_a.path().join("symbols.db");
        let mut db_a = SymbolDatabase::new(&db_path_a).unwrap();
        store_test_file(&db_a, "src/parser.rs");
        db_a.store_symbols(&[make_symbol(
            "Parser",
            "src/parser.rs",
            20,
            SymbolKind::Struct,
            Some("pub struct Parser"),
        )])
        .unwrap();

        let mut ws_a = JulieWorkspace::empty_shell(tmp_a.path().to_path_buf());
        ws_a.db = Some(Arc::new(Mutex::new(db_a)));

        // Workspace B: also has "Parser" definition (different file)
        let tmp_b = TempDir::new().unwrap();
        let db_path_b = tmp_b.path().join("symbols.db");
        let mut db_b = SymbolDatabase::new(&db_path_b).unwrap();
        store_test_file(&db_b, "lib/parser.py");
        db_b.store_symbols(&[make_symbol(
            "Parser",
            "lib/parser.py",
            5,
            SymbolKind::Class,
            Some("class Parser"),
        )])
        .unwrap();

        let mut ws_b = JulieWorkspace::empty_shell(tmp_b.path().to_path_buf());
        ws_b.db = Some(Arc::new(Mutex::new(db_b)));

        // Set up daemon state with both workspaces
        let ds = test_daemon_state();
        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "project_alpha".to_string(),
                LoadedWorkspace {
                    workspace: ws_a,
                    status: WorkspaceLoadStatus::Ready,
                    path: tmp_a.path().to_path_buf(),
                },
            );
            state.workspaces.insert(
                "project_beta".to_string(),
                LoadedWorkspace {
                    workspace: ws_b,
                    status: WorkspaceLoadStatus::Ready,
                    path: tmp_b.path().to_path_buf(),
                },
            );
        }

        let handler = JulieServerHandler::new_with_daemon_state(
            PathBuf::from("/tmp/test"),
            ds,
        )
        .unwrap();

        let result = crate::tools::navigation::federated_refs::find_refs_federated(
            &handler,
            "Parser",
            true,
            None,
            10,
        )
        .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result);
        let text = extract_text(&result.unwrap());

        // Should find definitions from both workspaces
        assert!(
            text.contains("src/parser.rs:20"),
            "Should find Parser in workspace A. Got:\n{}",
            text
        );
        assert!(
            text.contains("lib/parser.py:5"),
            "Should find Parser in workspace B. Got:\n{}",
            text
        );
        // Should mention "across 2 projects"
        assert!(
            text.contains("across 2 projects"),
            "Should show 2 projects. Got:\n{}",
            text
        );
    }

    // =========================================================================
    // Federated fast_refs: include_definition=false hides definitions
    // =========================================================================

    #[tokio::test]
    async fn test_federated_refs_exclude_definitions() {
        use crate::database::SymbolDatabase;
        use crate::workspace::JulieWorkspace;
        use std::sync::Mutex;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("symbols.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        store_test_file(&db, "src/config.rs");

        db.store_symbols(&[make_symbol(
            "Config",
            "src/config.rs",
            1,
            SymbolKind::Struct,
            Some("pub struct Config"),
        )])
        .unwrap();

        let mut workspace = JulieWorkspace::empty_shell(tmp.path().to_path_buf());
        workspace.db = Some(Arc::new(Mutex::new(db)));

        let ds = test_daemon_state();
        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "test_ws".to_string(),
                LoadedWorkspace {
                    workspace,
                    status: WorkspaceLoadStatus::Ready,
                    path: tmp.path().to_path_buf(),
                },
            );
        }

        let handler =
            JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/test"), ds).unwrap();

        // include_definition = false
        let result = crate::tools::navigation::federated_refs::find_refs_federated(
            &handler,
            "Config",
            false,  // Do NOT include definitions
            None,
            10,
        )
        .await;

        assert!(result.is_ok());
        let text = extract_text(&result.unwrap());

        // With include_definition=false and no references, the result set for
        // this project is empty (0 defs shown + 0 refs). The project gets skipped.
        // So we should see "No references found" since the only thing we had was
        // a definition that we chose to hide.
        assert!(
            text.contains("No references found") || !text.contains("Definition:"),
            "Should not show definitions when include_definition=false. Got:\n{}",
            text
        );
    }

    // =========================================================================
    // apply_global_ref_limit: unit tests for confidence-based truncation
    // =========================================================================

    #[test]
    fn test_global_limit_prefers_high_confidence_over_alphabetical_order() {
        use crate::tools::navigation::federated_refs::apply_global_ref_limit;

        // "a-project" has 5 LOW-confidence refs (0.3)
        // "z-project" has 3 HIGH-confidence refs (0.95)
        // With a limit of 4, the old alphabetical approach would keep all 4 from
        // "a-project" and starve "z-project" entirely. The fix should keep the
        // 3 high-confidence z-project refs + 1 from a-project.

        let a_refs: Vec<Relationship> = (0..5)
            .map(|i| {
                make_relationship_with_confidence(
                    &format!("src/a_file_{}.rs", i),
                    10 + i,
                    RelationshipKind::Calls,
                    0.3,
                )
            })
            .collect();

        let z_refs: Vec<Relationship> = (0..3)
            .map(|i| {
                make_relationship_with_confidence(
                    &format!("src/z_file_{}.rs", i),
                    20 + i,
                    RelationshipKind::Calls,
                    0.95,
                )
            })
            .collect();

        let mut per_project = vec![
            ("a-project".to_string(), vec![], a_refs),
            ("z-project".to_string(), vec![], z_refs),
        ];

        apply_global_ref_limit(&mut per_project, 4);

        // z-project should still have all 3 of its high-confidence refs
        let z_entry = per_project
            .iter()
            .find(|(name, _, _)| name == "z-project");
        assert!(
            z_entry.is_some(),
            "z-project should survive truncation (has high-confidence refs)"
        );
        let z_refs = &z_entry.unwrap().2;
        assert_eq!(
            z_refs.len(),
            3,
            "z-project should keep all 3 high-confidence refs, got {}",
            z_refs.len()
        );

        // a-project should have exactly 1 ref (4 total - 3 from z = 1)
        let a_entry = per_project
            .iter()
            .find(|(name, _, _)| name == "a-project");
        assert!(
            a_entry.is_some(),
            "a-project should survive with 1 ref"
        );
        let a_refs = &a_entry.unwrap().2;
        assert_eq!(
            a_refs.len(),
            1,
            "a-project should keep only 1 ref (budget remainder), got {}",
            a_refs.len()
        );

        // Total should be exactly the limit
        let total: usize = per_project.iter().map(|(_, _, r)| r.len()).sum();
        assert_eq!(total, 4, "Total refs should equal limit of 4");
    }

    #[test]
    fn test_global_limit_definitions_not_truncated() {
        use crate::tools::navigation::federated_refs::apply_global_ref_limit;

        // Both projects have definitions + refs. Limit is 2 refs.
        // Definitions should never be dropped.

        let a_defs = vec![make_symbol(
            "Widget",
            "src/widget.rs",
            1,
            SymbolKind::Struct,
            Some("pub struct Widget"),
        )];
        let a_refs = vec![
            make_relationship_with_confidence("src/use_a.rs", 10, RelationshipKind::Calls, 0.5),
            make_relationship_with_confidence("src/use_a2.rs", 20, RelationshipKind::Calls, 0.5),
        ];

        let z_defs = vec![make_symbol(
            "Widget",
            "lib/widget.py",
            1,
            SymbolKind::Class,
            Some("class Widget"),
        )];
        let z_refs = vec![
            make_relationship_with_confidence("lib/use_z.py", 10, RelationshipKind::Calls, 0.9),
        ];

        let mut per_project = vec![
            ("a-project".to_string(), a_defs, a_refs),
            ("z-project".to_string(), z_defs, z_refs),
        ];

        apply_global_ref_limit(&mut per_project, 2);

        // Both projects should still exist (they have definitions)
        assert_eq!(per_project.len(), 2, "Both projects should survive (have definitions)");

        // Definitions should be untouched
        for (name, defs, _) in &per_project {
            assert_eq!(
                defs.len(),
                1,
                "Project {} should still have its definition",
                name
            );
        }

        // Total refs should be 2 (the limit)
        let total_refs: usize = per_project.iter().map(|(_, _, r)| r.len()).sum();
        assert_eq!(total_refs, 2, "Total refs should equal limit of 2");

        // The high-confidence z-project ref (0.9) should survive
        let z_entry = per_project
            .iter()
            .find(|(name, _, _)| name == "z-project")
            .unwrap();
        assert_eq!(z_entry.2.len(), 1, "z-project's high-confidence ref should survive");
    }

    #[test]
    fn test_global_limit_removes_empty_projects_without_defs() {
        use crate::tools::navigation::federated_refs::apply_global_ref_limit;

        // "loser-project" has only low-confidence refs and no defs.
        // With a tight limit, it gets zero refs and should be removed entirely.

        let winner_refs = vec![
            make_relationship_with_confidence("src/win.rs", 1, RelationshipKind::Calls, 1.0),
            make_relationship_with_confidence("src/win.rs", 2, RelationshipKind::Calls, 0.99),
        ];
        let loser_refs = vec![
            make_relationship_with_confidence("src/lose.rs", 1, RelationshipKind::Calls, 0.1),
            make_relationship_with_confidence("src/lose.rs", 2, RelationshipKind::Calls, 0.1),
        ];

        let mut per_project = vec![
            ("winner".to_string(), vec![], winner_refs),
            ("loser".to_string(), vec![], loser_refs),
        ];

        apply_global_ref_limit(&mut per_project, 2);

        // Only winner should survive
        assert_eq!(per_project.len(), 1);
        assert_eq!(per_project[0].0, "winner");
        assert_eq!(per_project[0].2.len(), 2);
    }

    #[test]
    fn test_global_limit_no_truncation_when_under_limit() {
        use crate::tools::navigation::federated_refs::apply_global_ref_limit;

        // Total refs (3) < limit (10). Nothing should be dropped.
        let a_refs = vec![
            make_relationship_with_confidence("src/a.rs", 1, RelationshipKind::Calls, 0.5),
        ];
        let b_refs = vec![
            make_relationship_with_confidence("src/b.rs", 1, RelationshipKind::Calls, 0.7),
            make_relationship_with_confidence("src/b.rs", 2, RelationshipKind::Calls, 0.8),
        ];

        let mut per_project = vec![
            ("alpha".to_string(), vec![], a_refs),
            ("beta".to_string(), vec![], b_refs),
        ];

        apply_global_ref_limit(&mut per_project, 10);

        let total: usize = per_project.iter().map(|(_, _, r)| r.len()).sum();
        assert_eq!(total, 3, "All 3 refs should survive when under limit");
        assert_eq!(per_project.len(), 2, "Both projects should survive");
    }

    #[test]
    fn test_global_limit_output_sorted_by_project_name() {
        use crate::tools::navigation::federated_refs::apply_global_ref_limit;

        // Projects should end up sorted by name regardless of input order.
        let mut per_project = vec![
            (
                "zebra".to_string(),
                vec![],
                vec![make_relationship_with_confidence(
                    "z.rs",
                    1,
                    RelationshipKind::Calls,
                    1.0,
                )],
            ),
            (
                "apple".to_string(),
                vec![],
                vec![make_relationship_with_confidence(
                    "a.rs",
                    1,
                    RelationshipKind::Calls,
                    1.0,
                )],
            ),
            (
                "mango".to_string(),
                vec![],
                vec![make_relationship_with_confidence(
                    "m.rs",
                    1,
                    RelationshipKind::Calls,
                    1.0,
                )],
            ),
        ];

        apply_global_ref_limit(&mut per_project, 100);

        let names: Vec<&str> = per_project.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(names, vec!["apple", "mango", "zebra"]);
    }
}
