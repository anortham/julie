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

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Import,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
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

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Export,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    )
}
