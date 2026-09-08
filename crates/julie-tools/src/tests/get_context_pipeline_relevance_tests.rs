//! Integration tests for get_context relevance behavior through run_pipeline.

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::get_context::pipeline::run_pipeline;
    use julie_core::Symbol;
    use julie_core::database::{FileInfo, SymbolDatabase};
    use julie_extractors::{SymbolKind, Visibility};
    use julie_index::search::index::{SearchDocument, SearchIndex};

    fn make_test_sym(
        id: &str,
        name: &str,
        file: &str,
        language: &str,
        start_line: u32,
        end_line: u32,
        sig: &str,
        doc: &str,
        code: &str,
    ) -> Symbol {
        Symbol {
            extracted: julie_extractors::Symbol {
                id: id.to_string(),
                name: name.to_string(),
                kind: SymbolKind::Function,
                language: language.to_string(),
                file_path: file.to_string(),
                start_line,
                end_line,
                start_column: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some(sig.to_string()),
                doc_comment: Some(doc.to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.95),
                content_type: None,
                body_span: None,
                body_hash: None,
                annotations: Vec::new(),
            },
            code_context: Some(code.to_string()),
        }
    }

    fn setup_env() -> (TempDir, TempDir, SymbolDatabase, SearchIndex) {
        let db_dir = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();

        let db_path = db_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        let index = SearchIndex::create(index_dir.path()).unwrap();

        for (file, language) in [
            ("docs/workspace-routing.md", "markdown"),
            (".memories/checkpoint.md", "markdown"),
            ("src/workspace/resolver.rs", "rust"),
            ("src/workspace/validator.rs", "rust"),
        ] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: language.to_string(),
                hash: format!("hash_{}", file),
                size: 1000,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 1,
                line_count: 0,
                content: None,
            })
            .unwrap();
        }

        let docs_blob = "docsdominanttoken docsdominanttoken docsdominanttoken docsdominanttoken";
        let symbols = vec![
            make_test_sym(
                "doc_1",
                "workspace_routing_overview_docsdominanttoken",
                "docs/workspace-routing.md",
                "markdown",
                1,
                20,
                "fn docs_overview_docsdominanttoken()",
                docs_blob,
                docs_blob,
            ),
            make_test_sym(
                "doc_2",
                "workspace_routing_checkpoint_docsdominanttoken",
                ".memories/checkpoint.md",
                "markdown",
                1,
                20,
                "fn docs_checkpoint_docsdominanttoken()",
                docs_blob,
                docs_blob,
            ),
            make_test_sym(
                "code_1",
                "resolve_workspace_routing",
                "src/workspace/resolver.rs",
                "rust",
                10,
                30,
                "fn resolve_workspace_routing(docsdominanttoken: &str)",
                "Resolve workspace routing",
                "fn resolve_workspace_routing(input: &str) -> bool { input.len() > 0 }",
            ),
            make_test_sym(
                "code_2",
                "validate_workspace_routing",
                "src/workspace/validator.rs",
                "rust",
                40,
                60,
                "fn validate_workspace_routing(docsdominanttoken: &str)",
                "Validate workspace routing",
                "fn validate_workspace_routing(input: &str) -> bool { input.len() > 0 }",
            ),
        ];

        db.store_symbols(&symbols).unwrap();

        for symbol in &symbols {
            index
                .add_search_doc(&SearchDocument::for_symbol(
                    symbol,
                    vec![],
                    String::new(),
                    String::new(),
                ))
                .unwrap();
        }
        index.commit().unwrap();

        (db_dir, index_dir, db, index)
    }

    #[test]
    fn test_run_pipeline_prefers_code_pivots_under_low_actionable_coverage() {
        let (_db_dir, _index_dir, db, index) = setup_env();

        let output = run_pipeline(
            "docsdominanttoken workspace routing",
            None,
            None,
            None,
            None,
            &db,
            &index,
            None,
        )
        .unwrap();

        assert!(
            output.contains("PIVOT resolve_workspace_routing"),
            "expected resolver code symbol as pivot, got:\n{}",
            output
        );
        assert!(
            output.contains("PIVOT validate_workspace_routing"),
            "expected validator code symbol as pivot, got:\n{}",
            output
        );

        assert!(
            !output.contains("PIVOT workspace_routing_overview_docsdominanttoken"),
            "docs pivot should be dropped after code-first fallback, got:\n{}",
            output
        );
        assert!(
            !output.contains("PIVOT workspace_routing_checkpoint_docsdominanttoken"),
            "memory pivot should be dropped after code-first fallback, got:\n{}",
            output
        );
    }
}
