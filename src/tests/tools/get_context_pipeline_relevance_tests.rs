//! Integration tests for get_context relevance behavior through run_pipeline.

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Symbol, SymbolKind, Visibility};
    use crate::search::index::{SearchIndex, SymbolDocument};
    use crate::tools::get_context::pipeline::run_pipeline;

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
                content: None,
            })
            .unwrap();
        }

        let docs_blob = "docsdominanttoken docsdominanttoken docsdominanttoken docsdominanttoken";
        let symbols = vec![
            Symbol {
                id: "doc_1".to_string(),
                name: "workspace_routing_overview_docsdominanttoken".to_string(),
                kind: SymbolKind::Function,
                language: "markdown".to_string(),
                file_path: "docs/workspace-routing.md".to_string(),
                start_line: 1,
                end_line: 20,
                start_column: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some("fn docs_overview_docsdominanttoken()".to_string()),
                doc_comment: Some(docs_blob.to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(docs_blob.to_string()),
                content_type: None,
            },
            Symbol {
                id: "doc_2".to_string(),
                name: "workspace_routing_checkpoint_docsdominanttoken".to_string(),
                kind: SymbolKind::Function,
                language: "markdown".to_string(),
                file_path: ".memories/checkpoint.md".to_string(),
                start_line: 1,
                end_line: 20,
                start_column: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some("fn docs_checkpoint_docsdominanttoken()".to_string()),
                doc_comment: Some(docs_blob.to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(docs_blob.to_string()),
                content_type: None,
            },
            Symbol {
                id: "code_1".to_string(),
                name: "resolve_workspace_routing".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/workspace/resolver.rs".to_string(),
                start_line: 10,
                end_line: 30,
                start_column: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some(
                    "fn resolve_workspace_routing(docsdominanttoken: &str)".to_string(),
                ),
                doc_comment: Some("Resolve workspace routing".to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.95),
                code_context: Some(
                    "fn resolve_workspace_routing(input: &str) -> bool { input.len() > 0 }"
                        .to_string(),
                ),
                content_type: None,
            },
            Symbol {
                id: "code_2".to_string(),
                name: "validate_workspace_routing".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/workspace/validator.rs".to_string(),
                start_line: 40,
                end_line: 60,
                start_column: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some(
                    "fn validate_workspace_routing(docsdominanttoken: &str)".to_string(),
                ),
                doc_comment: Some("Validate workspace routing".to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.95),
                code_context: Some(
                    "fn validate_workspace_routing(input: &str) -> bool { !input.is_empty() }"
                        .to_string(),
                ),
                content_type: None,
            },
        ];

        db.store_symbols(&symbols).unwrap();

        for symbol in &symbols {
            index
                .add_symbol(&SymbolDocument::from_symbol(symbol))
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
            output.contains("Pivot: resolve_workspace_routing"),
            "expected resolver code symbol as pivot, got:\n{}",
            output
        );
        assert!(
            output.contains("Pivot: validate_workspace_routing"),
            "expected validator code symbol as pivot, got:\n{}",
            output
        );

        assert!(
            !output.contains("Pivot: workspace_routing_overview_docsdominanttoken"),
            "docs pivot should be dropped after code-first fallback, got:\n{}",
            output
        );
        assert!(
            !output.contains("Pivot: workspace_routing_checkpoint_docsdominanttoken"),
            "memory pivot should be dropped after code-first fallback, got:\n{}",
            output
        );
    }
}
