//! PowerShell identifier extraction for LSP-quality find_references
//! Extracts identifier usages (function calls, member access, etc.)

use crate::extractors::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::Node;

use super::helpers::find_command_name_node;

/// Extract all identifier usages (function calls, member access, etc.)
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    base.identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node(base, node, symbol_map);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(base, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // PowerShell commands and cmdlet calls: Get-Process, Write-Host, etc.
        "command" | "command_expression" => {
            // Extract command name
            if let Some(name_node) = find_command_name_node(node) {
                let name = base.get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
        }

        // PowerShell invocation expressions: function calls
        "invocation_expression" => {
            // Extract function name from invocation
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "command_name" || child.kind() == "identifier" {
                    let name = base.get_node_text(&child);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                    base.create_identifier(
                        &child,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                    break;
                } else if child.kind() == "member_access_expression" {
                    // For member access in invocation (e.g., $obj.Method())
                    // Extract the rightmost identifier (the method name)
                    let text = base.get_node_text(&child);
                    if let Some(last_dot_pos) = text.rfind('.') {
                        if last_dot_pos + 1 < text.len() {
                            let method_name = &text[last_dot_pos + 1..];
                            let containing_symbol_id =
                                find_containing_symbol_id(base, node, symbol_map);

                            base.create_identifier(
                                &child,
                                method_name.to_string(),
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                    break;
                }
            }
        }

        // PowerShell member access: $object.Property, $this.Name
        // PowerShell tree-sitter uses "member_access" (not "member_access_expression")
        "member_access" => {
            // Only extract if it's NOT part of an invocation_expression or command
            // (we handle method calls separately)
            if let Some(parent) = node.parent() {
                if parent.kind() == "invocation_expression" || parent.kind() == "command" {
                    return; // Skip - handled by invocation/command
                }
            }

            // Extract member name from member_access node
            // Structure: member_access -> member_name -> simple_name
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "member_name" {
                    // Get the simple_name child
                    let mut name_cursor = child.walk();
                    for name_child in child.children(&mut name_cursor) {
                        if name_child.kind() == "simple_name" {
                            let member_name = base.get_node_text(&name_child);
                            let containing_symbol_id =
                                find_containing_symbol_id(base, node, symbol_map);

                            base.create_identifier(
                                &name_child,
                                member_name,
                                IdentifierKind::MemberAccess,
                                containing_symbol_id,
                            );
                            return;
                        }
                    }
                }
            }
        }

        _ => {
            // Skip other node types for now
        }
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
/// POWERSHELL-SPECIFIC: Skip command symbols to avoid matching command calls with themselves
fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE, not all files
    // Bug was: searching all symbols in DB caused wrong file symbols to match
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| {
            s.file_path == base.file_path
                // PowerShell-specific: Skip command symbols (they're calls, not containers)
                // Only consider functions, methods, and classes as potential containers
                && matches!(s.kind, SymbolKind::Function | SymbolKind::Method | SymbolKind::Class)
                // PowerShell-specific: Skip single-line symbols (they're likely command calls)
                // A true containing symbol must have a range (start_line < end_line)
                && s.start_line < s.end_line
        })
        .map(|&s| s.clone())
        .collect();

    base.find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
