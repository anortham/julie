//! Core symbol extraction logic
//!
//! This module handles the main tree traversal and symbol type routing.
//! It delegates to specialized modules for specific symbol kinds.

use super::{classes, functions, imports_exports, interfaces};
use crate::extractors::base::Symbol;
use crate::extractors::typescript::TypeScriptExtractor;
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
            symbol = Some(classes::extract_class(extractor, node));
        }

        // Function extraction
        "function_declaration" | "function" => {
            symbol = Some(functions::extract_function(extractor, node));
        }

        // Method extraction (inside classes)
        "method_definition" | "method_signature" => {
            symbol = Some(functions::extract_method(extractor, node));
        }

        // Variable/arrow function assignment
        "variable_declarator" => {
            symbol = Some(functions::extract_variable(extractor, node));
        }

        // Interface extraction
        "interface_declaration" => {
            symbol = Some(interfaces::extract_interface(extractor, node));
        }

        // Type aliases
        "type_alias_declaration" => {
            symbol = Some(interfaces::extract_type_alias(extractor, node));
        }

        // Enums
        "enum_declaration" => {
            symbol = Some(interfaces::extract_enum(extractor, node));
        }

        // Import/export statements
        "import_statement" | "import_declaration" => {
            symbol = Some(imports_exports::extract_import(extractor, node));
        }
        "export_statement" => {
            symbol = Some(imports_exports::extract_export(extractor, node));
        }

        // Namespaces/modules
        "namespace_declaration" | "module_declaration" => {
            symbol = Some(interfaces::extract_namespace(extractor, node));
        }

        // Properties and fields
        "property_signature" | "public_field_definition" | "property_definition" => {
            symbol = Some(interfaces::extract_property(extractor, node));
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
