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
    fn test_typescript_import_named_alias_creates_binding_symbol() {
        let code = r#"
import React, { useState, createElement as h } from 'react';
import * as Utils from './utils';
import type { Foo as Bar } from './types';
import './setup';
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            "imports.ts".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let imports: Vec<_> = symbols
            .iter()
            .filter(|symbol| symbol.kind == SymbolKind::Import)
            .collect();
        let import_names: Vec<_> = imports.iter().map(|symbol| symbol.name.as_str()).collect();

        assert_eq!(
            imports.len(),
            5,
            "side-effect imports should not create binding symbols: {:?}",
            import_names
        );
        for expected in ["React", "useState", "h", "Utils", "Bar"] {
            assert!(
                import_names.contains(&expected),
                "missing import binding {expected}; got {import_names:?}"
            );
        }
        for module_or_raw in [
            "react",
            "./utils",
            "./types",
            "./setup",
            "* as Utils",
            "Foo",
        ] {
            assert!(
                !import_names.contains(&module_or_raw),
                "imports should be named after local bindings, not {module_or_raw}: {import_names:?}"
            );
        }

        let source_for = |name: &str| {
            imports
                .iter()
                .find(|symbol| symbol.name == name)
                .and_then(|symbol| symbol.metadata.as_ref())
                .and_then(|metadata| metadata.get("source"))
                .and_then(|value| value.as_str())
        };
        assert_eq!(source_for("React"), Some("react"));
        assert_eq!(source_for("useState"), Some("react"));
        assert_eq!(source_for("h"), Some("react"));
        assert_eq!(source_for("Utils"), Some("./utils"));
        assert_eq!(source_for("Bar"), Some("./types"));
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
