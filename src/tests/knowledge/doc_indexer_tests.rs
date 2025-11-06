// Documentation Indexer Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod documentation_indexer_tests {
    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::knowledge::doc_indexer::DocumentationIndexer;

    fn create_test_symbol(file_path: &str, name: &str, kind: SymbolKind) -> Symbol {
        Symbol {
            id: format!("test_{}", name),
            name: name.to_string(),
            kind,
            file_path: file_path.to_string(),
            language: "markdown".to_string(),
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        }
    }

    #[test]
    fn test_is_documentation_symbol_markdown_file() {
        let symbol = create_test_symbol("docs/CLAUDE.md", "Project Overview", SymbolKind::Module);

        assert!(
            DocumentationIndexer::is_documentation_symbol(&symbol),
            "Markdown file symbols should be detected as documentation"
        );
    }

    #[test]
    fn test_is_documentation_symbol_code_file() {
        let mut symbol = create_test_symbol("src/main.rs", "main", SymbolKind::Function);
        symbol.language = "rust".to_string();

        assert!(
            !DocumentationIndexer::is_documentation_symbol(&symbol),
            "Rust file symbols should NOT be detected as documentation"
        );
    }

    #[test]
    fn test_is_documentation_symbol_json_config() {
        let mut symbol = create_test_symbol("package.json", "dependencies", SymbolKind::Module);
        symbol.language = "json".to_string();

        // JSON is configuration, not documentation
        assert!(
            !DocumentationIndexer::is_documentation_symbol(&symbol),
            "JSON config symbols should NOT be documentation (they're configuration)"
        );
    }

    #[test]
    fn test_is_documentation_symbol_readme() {
        let symbol = create_test_symbol("README.md", "Installation", SymbolKind::Module);

        assert!(
            DocumentationIndexer::is_documentation_symbol(&symbol),
            "README.md symbols should be detected as documentation"
        );
    }

    #[test]
    fn test_is_documentation_symbol_nested_docs() {
        let symbol = create_test_symbol("docs/architecture/CASCADE.md", "Flow Diagram", SymbolKind::Module);

        assert!(
            DocumentationIndexer::is_documentation_symbol(&symbol),
            "Nested documentation file symbols should be detected"
        );
    }
}
