//! Import and export statement extraction
//!
//! This module handles extraction of import and export statements,
//! including named imports/exports, default exports, and re-exports.

use crate::base::{Symbol, SymbolKind, SymbolOptions};
use crate::typescript::TypeScriptExtractor;
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract an import statement
pub(super) fn extract_import(extractor: &mut TypeScriptExtractor, node: Node) -> Vec<Symbol> {
    let source = import_source(extractor, node);
    let signature = extractor.base().get_node_text(&node);
    let doc_comment = extractor.base().find_doc_comment(&node);
    extract_import_bindings(extractor, node)
        .into_iter()
        .map(|binding| {
            let mut metadata = HashMap::new();
            metadata.insert("source".to_string(), json!(source.clone()));
            metadata.insert("specifier".to_string(), json!(binding.local_name.clone()));
            metadata.insert("importedName".to_string(), json!(binding.imported_name));
            metadata.insert("isDefault".to_string(), json!(binding.is_default));
            metadata.insert("isNamespace".to_string(), json!(binding.is_namespace));
            metadata.insert("isTypeOnly".to_string(), json!(binding.is_type_only));

            extractor.base_mut().create_symbol(
                &node,
                binding.local_name,
                SymbolKind::Import,
                SymbolOptions {
                    signature: Some(signature.clone()),
                    metadata: Some(metadata),
                    doc_comment: doc_comment.clone(),
                    ..Default::default()
                },
            )
        })
        .collect()
}

#[derive(Debug)]
struct ImportBinding {
    local_name: String,
    imported_name: String,
    is_default: bool,
    is_namespace: bool,
    is_type_only: bool,
}

fn import_source(extractor: &TypeScriptExtractor, node: Node) -> String {
    node.child_by_field_name("source")
        .map(|source| {
            extractor
                .base()
                .get_node_text(&source)
                .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                .to_string()
        })
        .unwrap_or_default()
}

fn extract_import_bindings(extractor: &TypeScriptExtractor, node: Node) -> Vec<ImportBinding> {
    let Some(clause) = node
        .children(&mut node.walk())
        .find(|child| child.kind() == "import_clause")
    else {
        return Vec::new();
    };

    let is_type_only = extractor
        .base()
        .get_node_text(&node)
        .trim_start()
        .starts_with("import type");
    let mut bindings = Vec::new();
    let mut cursor = clause.walk();
    for child in clause.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                let local_name = extractor.base().get_node_text(&child);
                bindings.push(ImportBinding {
                    imported_name: "default".to_string(),
                    local_name,
                    is_default: true,
                    is_namespace: false,
                    is_type_only,
                });
            }
            "named_imports" => extract_named_imports(extractor, child, is_type_only, &mut bindings),
            "namespace_import" => {
                if let Some(local_node) = child
                    .children(&mut child.walk())
                    .find(|candidate| candidate.kind() == "identifier")
                {
                    let local_name = extractor.base().get_node_text(&local_node);
                    bindings.push(ImportBinding {
                        imported_name: "*".to_string(),
                        local_name,
                        is_default: false,
                        is_namespace: true,
                        is_type_only,
                    });
                }
            }
            _ => {}
        }
    }
    bindings
}

fn extract_named_imports(
    extractor: &TypeScriptExtractor,
    named_imports: Node,
    is_type_only: bool,
    bindings: &mut Vec<ImportBinding>,
) {
    let mut cursor = named_imports.walk();
    for specifier in named_imports.children(&mut cursor) {
        if specifier.kind() != "import_specifier" {
            continue;
        }
        let Some(name_node) = specifier.child_by_field_name("name") else {
            continue;
        };
        let imported_name = extractor.base().get_node_text(&name_node);
        let local_name = specifier
            .child_by_field_name("alias")
            .map(|alias| extractor.base().get_node_text(&alias))
            .unwrap_or_else(|| imported_name.clone());
        bindings.push(ImportBinding {
            local_name,
            imported_name,
            is_default: false,
            is_namespace: false,
            is_type_only,
        });
    }
}

/// Extract an export statement
///
/// Returns a `Vec<Symbol>` because `export { a, b, c }` produces one symbol per specifier.
pub(super) fn extract_export(extractor: &mut TypeScriptExtractor, node: Node) -> Vec<Symbol> {
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
