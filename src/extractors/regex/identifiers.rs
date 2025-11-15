use crate::extractors::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

use super::flags;

/// Extract all identifier usages (backreferences and named groups)
/// Following the Rust extractor reference implementation pattern
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
        // Backreferences: tree-sitter-regex uses "backreference_escape" for \k
        // But doesn't properly parse the <name> part, so we need to extract manually
        "backreference_escape" => {
            // Get the full text context around this node to find the group name
            let start_byte = node.start_byte();
            let content_after = &base.content[start_byte..];

            // Try to extract \k<name> pattern manually
            if content_after.starts_with("\\k<") {
                if let Some(end_pos) = content_after.find('>') {
                    // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
                    if content_after.is_char_boundary(3) && content_after.is_char_boundary(end_pos)
                    {
                        let group_name = content_after[3..end_pos].to_string();
                        if !group_name.is_empty() {
                            let containing_symbol_id =
                                find_containing_symbol_id(base, node, symbol_map);

                            base.create_identifier(
                                &node,
                                group_name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                }
            }
        }

        // Original "backreference" node type (if tree-sitter-regex ever adds proper support)
        "backreference" => {
            let backref_text = base.get_node_text(&node);

            // Try to extract named backreference (e.g., \k<email>)
            if let Some(group_name) = flags::extract_backref_group_name(&backref_text) {
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &node,
                    group_name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // Note: Numeric backreferences (\1, \2) don't have names to track
        }

        // Named groups: (?<name>...) (these are "member access" in regex context)
        "named_capturing_group" => {
            let group_text = base.get_node_text(&node);

            // Extract the group name using the flags module
            if let Some(group_name) = extract_group_name(&group_text) {
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &node,
                    group_name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {
            // Skip other node types for now
        }
    }
}

/// Extract the name from a named group (?<name>...) or (?P<name>...)
pub(crate) fn extract_group_name(group_text: &str) -> Option<String> {
    if let Some(start) = group_text.find("(?<") {
        if let Some(end) = group_text[start + 3..].find('>') {
            let end_idx = start + 3 + end;
            // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
            if group_text.is_char_boundary(start + 3) && group_text.is_char_boundary(end_idx) {
                return Some(group_text[start + 3..end_idx].to_string());
            }
        }
    }
    if let Some(start) = group_text.find("(?P<") {
        if let Some(end) = group_text[start + 4..].find('>') {
            let end_idx = start + 4 + end;
            // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
            if group_text.is_char_boundary(start + 4) && group_text.is_char_boundary(end_idx) {
                return Some(group_text[start + 4..end_idx].to_string());
            }
        }
    }
    None
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
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
