//! Import and export statement extraction
//!
//! This module handles extraction of import and export statements,
//! including named imports/exports, default exports, and re-exports.

use crate::base::{Symbol, SymbolKind, SymbolOptions};
use crate::typescript::TypeScriptExtractor;
use tree_sitter::Node;

/// Extract an import statement
pub(super) fn extract_import(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Option<Symbol> {
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
            .map(|n| extractor.base().get_node_text(&n))?
    };

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    Some(extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Import,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    ))
}

/// Extract an export statement
///
/// Returns a `Vec<Symbol>` because `export { a, b, c }` produces one symbol per specifier.
pub(super) fn extract_export(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Vec<Symbol> {
    // For exports, extract what's being exported
    if let Some(declaration_node) = node.child_by_field_name("declaration") {
        // export class/function/const/etc — single symbol
        let name = match declaration_node
            .child_by_field_name("name")
            .map(|n| extractor.base().get_node_text(&n))
        {
            Some(n) => n,
            None => return Vec::new(),
        };
        let doc_comment = extractor.base().find_doc_comment(&node);
        vec![extractor.base_mut().create_symbol(
            &node,
            name,
            SymbolKind::Export,
            SymbolOptions {
                doc_comment,
                ..Default::default()
            },
        )]
    } else if let Some(source_node) = node.child_by_field_name("source") {
        // export { ... } from '...' — single re-export symbol
        let name = extractor
            .base()
            .get_node_text(&source_node)
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string();
        let doc_comment = extractor.base().find_doc_comment(&node);
        vec![extractor.base_mut().create_symbol(
            &node,
            name,
            SymbolKind::Export,
            SymbolOptions {
                doc_comment,
                ..Default::default()
            },
        )]
    } else {
        // export { a, b, c } — one symbol per specifier
        let doc_comment = extractor.base().find_doc_comment(&node);
        let export_clause = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "export_clause");
        let clause = match export_clause {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut symbols = Vec::new();
        let mut cursor = clause.walk();
        for spec in clause.named_children(&mut cursor) {
            if let Some(name_node) = spec.child_by_field_name("name") {
                let name = extractor.base().get_node_text(&name_node);
                symbols.push(extractor.base_mut().create_symbol(
                    &node,
                    name,
                    SymbolKind::Export,
                    SymbolOptions {
                        doc_comment: doc_comment.clone(),
                        ..Default::default()
                    },
                ));
            }
        }
        symbols
    }
}
