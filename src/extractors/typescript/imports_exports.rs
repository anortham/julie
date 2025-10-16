//! Import and export statement extraction
//!
//! This module handles extraction of import and export statements,
//! including named imports/exports, default exports, and re-exports.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use crate::extractors::typescript::TypeScriptExtractor;
use tree_sitter::Node;

/// Extract an import statement
pub(super) fn extract_import(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    // For imports, extract the source (what's being imported from)
    let name = if let Some(source_node) = node.child_by_field_name("source") {
        extractor
            .base()
            .get_node_text(&source_node)
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string()
    } else {
        // Try to get import clause for named imports
        node.children(&mut node.walk())
            .find(|c| c.kind() == "import_clause")
            .and_then(|clause| clause.child_by_field_name("name"))
            .map(|n| extractor.base().get_node_text(&n))
            .unwrap_or_else(|| "import".to_string())
    };

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Import,
        SymbolOptions::default(),
    )
}

/// Extract an export statement
pub(super) fn extract_export(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    // For exports, extract what's being exported
    let name = if let Some(declaration_node) = node.child_by_field_name("declaration") {
        // export class/function/const/etc
        declaration_node
            .child_by_field_name("name")
            .map(|n| extractor.base().get_node_text(&n))
            .unwrap_or_else(|| "export".to_string())
    } else if let Some(source_node) = node.child_by_field_name("source") {
        // export { ... } from '...'
        extractor
            .base()
            .get_node_text(&source_node)
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string()
    } else {
        // export { ... }
        node.children(&mut node.walk())
            .find(|c| c.kind() == "export_clause")
            .and_then(|clause| clause.named_child(0))
            .and_then(|spec| spec.child_by_field_name("name"))
            .map(|n| extractor.base().get_node_text(&n))
            .unwrap_or_else(|| "export".to_string())
    };

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Export,
        SymbolOptions::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_import_named() {
        let code = "import { foo } from './bar';";
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            "test.ts".to_string(),
            code.to_string(),
        );
        let symbols = extractor.extract_symbols(&tree);

        assert!(symbols.iter().any(|s| s.kind == SymbolKind::Import));
    }

    #[test]
    fn test_extract_export_named() {
        let code = "export { myVar };";
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            "test.ts".to_string(),
            code.to_string(),
        );
        let symbols = extractor.extract_symbols(&tree);

        assert!(symbols.iter().any(|s| s.kind == SymbolKind::Export));
    }
}
