/// Rust identifier extraction for LSP-quality reference tracking
/// - Function calls
/// - Variable references
/// - Member access expressions
use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use crate::extractors::rust::RustExtractor;
use std::collections::HashMap;
use tree_sitter::Tree;

/// Extract all identifiers (references/usages) for LSP-quality reference tracking
///
/// Phase 1 - basic extraction. We extract:
/// - Function calls (call_expression)
/// - Variable references (identifier nodes in certain contexts)
///
/// Identifiers are stored unresolved (target_symbol_id = None) and resolved
/// on-demand during queries for optimal incremental update performance.
pub(super) fn extract_identifiers(
    extractor: &mut RustExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Build symbol map for finding containing symbols
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return extracted identifiers from base extractor
    extractor.get_base_mut().identifiers.clone()
}

/// Walk the tree extracting identifiers
fn walk_tree_for_identifiers(
    extractor: &mut RustExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node(extractor, node, symbol_map);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(extractor, child, symbol_map);
    }
}

/// Extract identifier from a single node
fn extract_identifier_from_node(
    extractor: &mut RustExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function calls: foo(), bar.baz()
        "call_expression" => {
            if let Some(func_node) = node.child_by_field_name("function") {
                // Handle method calls (e.g., self.method())
                // Extract just the method name, not the whole "self.method" text
                let name = {
                    let base = extractor.get_base_mut();
                    if func_node.kind() == "field_expression" {
                        // Method call: extract just the field name
                        if let Some(field_node) = func_node.child_by_field_name("field") {
                            base.get_node_text(&field_node)
                        } else {
                            base.get_node_text(&func_node)
                        }
                    } else {
                        // Regular function call
                        base.get_node_text(&func_node)
                    }
                };

                let identifier_node = if func_node.kind() == "field_expression" {
                    if let Some(field_node) = func_node.child_by_field_name("field") {
                        field_node
                    } else {
                        func_node
                    }
                } else {
                    func_node
                };

                // Find containing symbol (which function/method contains this call)
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                // Create identifier for this function call
                {
                    let base = extractor.get_base_mut();
                    base.create_identifier(
                        &identifier_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }
        }

        // Variable/field references in specific contexts
        // We're conservative - only extract clear variable usages, not all identifiers
        "field_expression" => {
            // Skip if this field_expression is the function of a call_expression
            // (e.g., self.method() - we want "method" as Call, not MemberAccess)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    if let Some(func_child) = parent.child_by_field_name("function") {
                        if func_child.id() == node.id() {
                            // This field_expression IS the function being called, skip it
                            return;
                        }
                    }
                }
            }

            // object.field - extract the field name (not part of a call)
            if let Some(field_node) = node.child_by_field_name("field") {
                let name = {
                    let base = extractor.get_base_mut();
                    base.get_node_text(&field_node)
                };
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                {
                    let base = extractor.get_base_mut();
                    base.create_identifier(
                        &field_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }
        }

        _ => {
            // Skip other node types for now
            // Future: type_usage, import statements, etc.
        }
    }
}

/// Find the ID of the symbol that contains this node
fn find_containing_symbol_id(
    extractor: &mut RustExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let base = extractor.get_base_mut();
    // CRITICAL FIX: Only search symbols from THIS FILE, not all files
    // Bug was: searching all symbols in DB caused wrong file symbols to match
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == base.file_path)
        .map(|&s| s.clone())
        .collect();

    base.find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
