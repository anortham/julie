//! Tests for federated deep_dive (workspace="all") — cross-project symbol investigation.
//!
//! Test layers:
//! 1. Formatting: project attribution in header, cross-project callers section
//! 2. Cross-project caller formatting
//! 3. Stdio mode error (daemon_state is None)
//! 4. Integration: multiple workspace DBs with shared symbols

#[cfg(test)]
mod formatting_tests {
    use crate::extractors::base::{SymbolKind, Visibility};
    use crate::extractors::Symbol;
    use crate::tools::deep_dive::data::SymbolContext;
    use crate::tools::deep_dive::formatting::{
        format_symbol_context, format_symbol_context_with_project,
    };

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file_path: &str,
        line: u32,
        signature: Option<&str>,
        visibility: Option<Visibility>,
    ) -> Symbol {
        Symbol {
            id: format!("test_{}_{}", name, line),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: None,
            signature: signature.map(|s| s.to_string()),
            doc_comment: None,
            visibility,
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: None,
            content_type: None,
        }
    }

    fn empty_context(symbol: Symbol) -> SymbolContext {
        SymbolContext {
            symbol,
            incoming: vec![],
            incoming_total: 0,
            outgoing: vec![],
            outgoing_total: 0,
            children: vec![],
            implementations: vec![],
            test_refs: vec![],
            similar: vec![],
        }
    }

    #[test]
    fn test_format_without_project_has_no_project_tag() {
        let sym = make_symbol(
            "SearchIndex",
            SymbolKind::Class,
            "src/search/index.rs",
            130,
            None,
            Some(Visibility::Public),
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context(&ctx, "overview");

        assert!(output.contains("src/search/index.rs:130"));
        assert!(output.contains("(class, public)"));
        assert!(!output.contains("[project:"));
    }

    #[test]
    fn test_format_with_project_shows_project_tag_in_header() {
        let sym = make_symbol(
            "SearchIndex",
            SymbolKind::Class,
            "src/search/index.rs",
            130,
            None,
            Some(Visibility::Public),
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context_with_project(&ctx, "overview", "julie");

        assert!(output.contains("src/search/index.rs:130"));
        assert!(output.contains("(class, public)"));
        assert!(output.contains("[project: julie]"));
    }

    #[test]
    fn test_project_tag_appears_after_kind_visibility() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            Some("pub fn process(data: &[u8]) -> Result<()>"),
            Some(Visibility::Public),
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context_with_project(&ctx, "overview", "my-project");

        // The header line should be: file:line (kind, vis) [project: name]
        let first_line = output.lines().next().unwrap();
        assert!(
            first_line.contains("(function, public) [project: my-project]"),
            "Expected project tag after kind/vis, got: {}",
            first_line
        );
    }

    #[test]
    fn test_format_with_project_preserves_signature() {
        let sym = make_symbol(
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            42,
            Some("pub fn process(data: &[u8]) -> Result<()>"),
            Some(Visibility::Public),
        );
        let ctx = empty_context(sym);
        let output = format_symbol_context_with_project(&ctx, "overview", "julie");

        assert!(output.contains("pub fn process(data: &[u8]) -> Result<()>"));
    }
}

#[cfg(test)]
mod cross_project_callers_tests {
    use crate::tools::deep_dive::{format_cross_project_callers, CrossProjectCaller};

    #[test]
    fn test_empty_callers_returns_empty_string() {
        let result = format_cross_project_callers(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_caller_with_name() {
        let callers = vec![CrossProjectCaller {
            project_name: "frontend".to_string(),
            file_path: "src/api/client.ts".to_string(),
            line_number: 55,
            caller_name: Some("fetchData".to_string()),
        }];
        let output = format_cross_project_callers(&callers);

        assert!(output.contains("Cross-project callers (1):"));
        assert!(output.contains("src/api/client.ts:55  fetchData  [project: frontend]"));
    }

    #[test]
    fn test_single_caller_without_name() {
        let callers = vec![CrossProjectCaller {
            project_name: "backend".to_string(),
            file_path: "lib/handler.rb".to_string(),
            line_number: 12,
            caller_name: None,
        }];
        let output = format_cross_project_callers(&callers);

        assert!(output.contains("Cross-project callers (1):"));
        assert!(output.contains("lib/handler.rb:12  [project: backend]"));
    }

    #[test]
    fn test_multiple_callers_from_different_projects() {
        let callers = vec![
            CrossProjectCaller {
                project_name: "web-app".to_string(),
                file_path: "src/hooks/useSearch.ts".to_string(),
                line_number: 30,
                caller_name: Some("useSearch".to_string()),
            },
            CrossProjectCaller {
                project_name: "cli-tool".to_string(),
                file_path: "cmd/search.go".to_string(),
                line_number: 88,
                caller_name: Some("runSearch".to_string()),
            },
            CrossProjectCaller {
                project_name: "web-app".to_string(),
                file_path: "src/components/SearchBar.tsx".to_string(),
                line_number: 15,
                caller_name: None,
            },
        ];
        let output = format_cross_project_callers(&callers);

        assert!(output.contains("Cross-project callers (3):"));
        assert!(output.contains("[project: web-app]"));
        assert!(output.contains("[project: cli-tool]"));
        assert!(output.contains("useSearch"));
        assert!(output.contains("runSearch"));
    }

    #[test]
    fn test_caller_count_is_accurate() {
        let callers: Vec<CrossProjectCaller> = (0..7)
            .map(|i| CrossProjectCaller {
                project_name: format!("proj-{}", i),
                file_path: format!("src/file{}.rs", i),
                line_number: i as u32 * 10,
                caller_name: Some(format!("func_{}", i)),
            })
            .collect();
        let output = format_cross_project_callers(&callers);
        assert!(output.contains("Cross-project callers (7):"));
    }
}

#[cfg(test)]
mod query_with_project_tests {
    use crate::database::SymbolDatabase;
    use crate::database::types::FileInfo;
    use crate::extractors::base::{SymbolKind, Visibility};
    use crate::tools::deep_dive::{deep_dive_query, deep_dive_query_with_project};
    use tempfile::TempDir;

    fn setup_db_with_symbol() -> (TempDir, SymbolDatabase) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut db = SymbolDatabase::new(db_path).unwrap();

        // Store file info first (FK constraint)
        db.store_file_info(&FileInfo {
            path: "src/search/index.rs".to_string(),
            language: "rust".to_string(),
            hash: "hash_search_index".to_string(),
            size: 500,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 1,
            content: None,
        })
        .unwrap();

        // Store a symbol
        db.store_symbols(&[crate::extractors::Symbol {
            id: "sym_search_index_1".to_string(),
            name: "SearchIndex".to_string(),
            kind: SymbolKind::Class,
            language: "rust".to_string(),
            file_path: "src/search/index.rs".to_string(),
            start_line: 130,
            end_line: 200,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 500,
            parent_id: None,
            signature: Some("pub struct SearchIndex".to_string()),
            doc_comment: None,
            visibility: Some(Visibility::Public),
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: None,
            content_type: None,
        }])
        .unwrap();

        (tmp, db)
    }

    #[test]
    fn test_deep_dive_query_no_project_tag() {
        let (_tmp, db) = setup_db_with_symbol();
        let result = deep_dive_query(&db, "SearchIndex", None, "overview", 10, 10).unwrap();

        assert!(result.contains("src/search/index.rs:130"));
        assert!(!result.contains("[project:"));
    }

    #[test]
    fn test_deep_dive_query_with_project_tag() {
        let (_tmp, db) = setup_db_with_symbol();
        let result =
            deep_dive_query_with_project(&db, "SearchIndex", None, "overview", 10, 10, "julie")
                .unwrap();

        assert!(result.contains("src/search/index.rs:130"));
        assert!(result.contains("[project: julie]"));
    }

    #[test]
    fn test_deep_dive_query_not_found_no_project() {
        let (_tmp, db) = setup_db_with_symbol();
        let result = deep_dive_query(&db, "NonExistent", None, "overview", 10, 10).unwrap();
        assert!(result.contains("No symbol found: 'NonExistent'"));
    }

    #[test]
    fn test_deep_dive_query_with_project_preserves_signature() {
        let (_tmp, db) = setup_db_with_symbol();
        let result = deep_dive_query_with_project(
            &db,
            "SearchIndex",
            None,
            "overview",
            10,
            10,
            "my-project",
        )
        .unwrap();

        assert!(result.contains("pub struct SearchIndex"));
        assert!(result.contains("[project: my-project]"));
    }
}

#[cfg(test)]
mod stdio_mode_tests {
    use crate::handler::JulieServerHandler;
    use crate::tools::DeepDiveTool;

    #[tokio::test]
    async fn test_workspace_all_errors_in_stdio_mode() {
        // In stdio mode, daemon_state is None — workspace="all" must error
        let handler = JulieServerHandler::new_for_test().await.unwrap();
        assert!(
            handler.daemon_state.is_none(),
            "Test handler should have no daemon_state (stdio mode)"
        );

        let tool = DeepDiveTool {
            symbol: "SearchIndex".to_string(),
            depth: "overview".to_string(),
            context_file: None,
            workspace: Some("all".to_string()),
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_err(), "workspace='all' should error in stdio mode");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("daemon mode"),
            "Error should mention daemon mode, got: {}",
            err_msg
        );
    }
}
