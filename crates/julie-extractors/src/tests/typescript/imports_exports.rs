//! Inline tests extracted from extractors/typescript/imports_exports.rs
//!
//! These tests validate import and export statement extraction functionality.

#[cfg(test)]
mod tests {
    use crate::base::SymbolKind;
    use crate::typescript::TypeScriptExtractor;
    use std::path::PathBuf;

    #[test]
    fn test_extract_import_named() {
        let code = "import { foo } from './bar';";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            "test.ts".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        assert!(symbols.iter().any(|s| s.kind == SymbolKind::Import));
    }

    #[test]
    fn test_extract_export_named() {
        let code = "export { myVar };";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            "test.ts".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        assert!(symbols.iter().any(|s| s.kind == SymbolKind::Export));
    }

    #[test]
    fn test_extract_export_multiple_specifiers() {
        let code = "export { foo, bar, baz };";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            "test.ts".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let export_symbols: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Export)
            .collect();

        assert_eq!(
            export_symbols.len(),
            3,
            "Expected 3 export symbols for 'export {{ foo, bar, baz }}', got {}: {:?}",
            export_symbols.len(),
            export_symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        let export_names: Vec<&str> = export_symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(export_names.contains(&"foo"), "Missing export 'foo'");
        assert!(export_names.contains(&"bar"), "Missing export 'bar'");
        assert!(export_names.contains(&"baz"), "Missing export 'baz'");
    }
}
