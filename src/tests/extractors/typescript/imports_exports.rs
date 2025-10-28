//! Inline tests extracted from extractors/typescript/imports_exports.rs
//!
//! These tests validate import and export statement extraction functionality.

#[cfg(test)]
mod tests {
    use crate::extractors::base::SymbolKind;
    use crate::extractors::typescript::TypeScriptExtractor;
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
}
