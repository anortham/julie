// Markdown Extractor Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod markdown_extractor_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::extractors::markdown::MarkdownExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_md::LANGUAGE.into())
            .expect("Error loading Markdown grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<Symbol> {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor = MarkdownExtractor::new(
            "markdown".to_string(),
            "test.md".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    #[test]
    fn test_extract_markdown_sections() {
        let markdown = r#"# Main Title

This is the introduction.

## Section One

Content for section one.

### Subsection 1.1

Detailed content here.

## Section Two

More content in section two.
"#;

        let symbols = extract_symbols(markdown);

        // We should extract sections as symbols
        assert!(symbols.len() >= 4, "Expected at least 4 sections, got {}", symbols.len());

        // Check main title
        let main_title = symbols.iter().find(|s| s.name == "Main Title");
        assert!(main_title.is_some(), "Should find 'Main Title' section");
        assert_eq!(main_title.unwrap().kind, SymbolKind::Module); // Treating sections as modules

        // Check Section One
        let section_one = symbols.iter().find(|s| s.name == "Section One");
        assert!(section_one.is_some(), "Should find 'Section One' section");

        // Check Subsection
        let subsection = symbols.iter().find(|s| s.name == "Subsection 1.1");
        assert!(subsection.is_some(), "Should find 'Subsection 1.1' section");

        // Check Section Two
        let section_two = symbols.iter().find(|s| s.name == "Section Two");
        assert!(section_two.is_some(), "Should find 'Section Two' section");
    }
}
