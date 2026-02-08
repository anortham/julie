//! Core symbol extraction logic
//!
//! This module handles the main tree traversal and symbol type routing.
//! It delegates to specialized modules for specific symbol kinds.

use super::{classes, functions, imports_exports, interfaces};
use crate::base::Symbol;
use crate::typescript::TypeScriptExtractor;
use tree_sitter::{Node, Tree};

/// Extract all symbols from the syntax tree
pub(super) fn extract_symbols(extractor: &mut TypeScriptExtractor, tree: &Tree) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    visit_node(extractor, tree.root_node(), &mut symbols);
    symbols
}

/// Recursively visit nodes and extract symbols based on node kind
fn visit_node(extractor: &mut TypeScriptExtractor, node: Node, symbols: &mut Vec<Symbol>) {
    let mut symbol: Option<Symbol> = None;

    // Route node types to appropriate extraction modules
    match node.kind() {
        // Class extraction
        "class_declaration" => {
            symbol = classes::extract_class(extractor, node);
        }

        // Function extraction
        "function_declaration" | "function" => {
            symbol = functions::extract_function(extractor, node);
        }

        // Method extraction (inside classes)
        "method_definition" | "method_signature" => {
            symbol = functions::extract_method(extractor, node);
        }

        // Variable/arrow function assignment
        "variable_declarator" => {
            symbol = functions::extract_variable(extractor, node);
        }

        // Interface extraction
        "interface_declaration" => {
            symbol = interfaces::extract_interface(extractor, node);
        }

        // Type aliases
        "type_alias_declaration" => {
            symbol = interfaces::extract_type_alias(extractor, node);
        }

        // Enums
        "enum_declaration" => {
            let enum_symbols = interfaces::extract_enum(extractor, node);
            symbols.extend(enum_symbols);
        }

        // Import/export statements
        "import_statement" | "import_declaration" => {
            symbol = imports_exports::extract_import(extractor, node);
        }
        "export_statement" => {
            let export_symbols = imports_exports::extract_export(extractor, node);
            symbols.extend(export_symbols);
        }

        // Namespaces/modules
        "namespace_declaration" | "module_declaration" => {
            symbol = interfaces::extract_namespace(extractor, node);
        }

        // Properties and fields
        "property_signature" | "public_field_definition" | "property_definition" => {
            symbol = interfaces::extract_property(extractor, node);
        }

        _ => {}
    }

    if let Some(sym) = symbol {
        symbols.push(sym);
    }

    // Recursively visit children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_node(extractor, child, symbols);
    }
}
