//! PowerShell identifier extraction for LSP-quality find_references
//! Extracts identifier usages (function calls, member access, etc.)

use crate::base::{extract_type_arguments, BaseExtractor, Identifier, IdentifierKind, Symbol, SymbolKind};
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

        // PowerShell .NET generic type: [List[User]], [Dictionary[string, int]]
        // The grammar uses `generic_type_name` for the base name and
        // `generic_type_arguments` (sibling in the parent `type_spec`) for the args.
        "generic_type_name" => {
            // Skip nested generics: a generic_type_name whose parent type_spec lives
            // inside generic_type_arguments is itself a nested argument — its args
            // ride along as children of the enclosing usage, not as a separate row.
            let is_nested = node
                .parent() // type_spec
                .and_then(|p| p.parent()) // generic_type_arguments
                .map(|gp| gp.kind() == "generic_type_arguments")
                .unwrap_or(false);
            if is_nested {
                return;
            }

            // Extract the base type name from the `type_name` child.
            let mut cursor = node.walk();
            let Some(type_name_node) = node.named_children(&mut cursor).next() else {
                return;
            };
            let name = base.get_node_text(&type_name_node);
            let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
            let identifier = base.create_identifier(
                &type_name_node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );

            // Find the `generic_type_arguments` sibling in the parent `type_spec`.
            let Some(type_spec) = node.parent() else {
                return;
            };
            let mut spec_cursor = type_spec.walk();
            let Some(arg_list) = type_spec
                .named_children(&mut spec_cursor)
                .find(|c| c.kind() == "generic_type_arguments")
            else {
                return;
            };

            let arguments = extract_type_arguments(base, arg_list, decompose_powershell_type_arg);
            base.record_type_arguments(&identifier, arguments);
        }

        _ => {
            // Skip other node types for now
        }
    }
}

// ============================================================================
// Type-argument capture helpers (Miller bridge Phase 2)
// ============================================================================

/// `TypeArgDecomposer` for PowerShell: maps a child of a `generic_type_arguments`
/// node to its applied argument.
///
/// Each child of `generic_type_arguments` is a `type_spec`. A `type_spec` for a
/// leaf argument contains a `type_name` child; one for a nested generic contains
/// a `generic_type_name` + `generic_type_arguments`. Unnamed nodes (commas,
/// brackets `[`, `]`) return `None` and are skipped.
fn decompose_powershell_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip commas, brackets
    }
    if node.kind() != "type_spec" {
        return None; // only type_spec children are arguments
    }
    let mut cursor1 = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor1).collect();

    // Check for nested generic: generic_type_name + generic_type_arguments
    if let Some(&gtn) = children.iter().find(|c| c.kind() == "generic_type_name") {
        // Extract the type_name text from the generic_type_name child.
        let mut gtn_cursor = gtn.walk();
        let name = gtn
            .named_children(&mut gtn_cursor)
            .next()
            .map(|n| base.get_node_text(&n))
            .unwrap_or_else(|| base.get_node_text(&gtn));
        // The nested arg list is the generic_type_arguments sibling.
        let nested = children.iter().find(|c| c.kind() == "generic_type_arguments").copied();
        Some((name, nested))
    } else {
        // Leaf argument: extract name from type_name child.
        let type_name = children.iter().find(|c| c.kind() == "type_name")?;
        Some((base.get_node_text(type_name), None))
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
    base.find_containing_symbol_from_map_filtered(&node, symbol_map, |symbol| {
        matches!(
            symbol.kind,
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Class
        ) && symbol.start_line < symbol.end_line
    })
    .map(|s| s.id.clone())
}
