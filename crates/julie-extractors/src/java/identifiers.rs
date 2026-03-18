/// Identifier extraction for LSP-quality find_references
use crate::base::{Identifier, IdentifierKind, Symbol};
use crate::java::JavaExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub(super) fn extract_identifiers(
    extractor: &mut JavaExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base().identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    extractor: &mut JavaExtractor,
    node: Node,
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

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    extractor: &mut JavaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Method calls: foo(), bar.baz(), System.out.println()
        "method_invocation" => {
            // Try to get the method name from the "name" field (standard tree-sitter pattern)
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = extractor.base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            } else {
                // Fallback: look for identifier children
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = extractor.base().get_node_text(&child);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    }
                }
            }
        }

        // Field access: object.field
        "field_access" => {
            // Only extract if it's NOT part of a method_invocation
            // (we handle those in the method_invocation case above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "method_invocation" {
                    return; // Skip - handled by method_invocation
                }
            }

            // Extract the rightmost identifier (the field name)
            if let Some(name_node) = node.child_by_field_name("field") {
                let name = extractor.base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // Type references: Gson gson, TypeAdapter<T> adapter, List<JsonElement>, etc.
        // Java tree-sitter uses `type_identifier` for BOTH declaration names
        // (class Foo, interface Foo) AND reference positions (Gson gson).
        // We only want references — declarations are filtered by parent context.
        "type_identifier" => {
            // Skip if this is a declaration name, not a type reference.
            if is_type_declaration_name(&node) {
                return;
            }

            let name = extractor.base().get_node_text(&node);

            // Skip single-letter generics — they carry no cross-file signal.
            if is_java_noise_type(&name) {
                return;
            }

            let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

            extractor.base_mut().create_identifier(
                &node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
        }

        _ => {
            // Skip other node types for now
            // Future: constructor calls, etc.
        }
    }
}

/// Check if a `type_identifier` node is a declaration name rather than a type reference.
///
/// In Java tree-sitter, `type_identifier` appears as the `name` field of:
/// - `class_declaration` → `class Foo {}` (declaration)
/// - `interface_declaration` → `interface Foo {}` (declaration)
/// - `enum_declaration` → `enum Foo {}` (declaration)
/// - `annotation_type_declaration` → `@interface Foo {}` (declaration)
/// - `type_parameter` → `<T extends Base>` (the `T` is a declaration)
fn is_type_declaration_name(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        // Check if this node is the `name` field of a declaration or type param
        if let Some(name_node) = parent.child_by_field_name("name") {
            if name_node.id() == node.id() {
                return matches!(
                    parent.kind(),
                    "class_declaration"
                        | "interface_declaration"
                        | "enum_declaration"
                        | "annotation_type_declaration"
                        | "type_parameter"
                );
            }
        }
    }
    false
}

/// Returns true for Java types that are too common/noisy to be meaningful
/// type references for centrality scoring.
///
/// Only filters single-letter generics (T, K, V, E, R, etc.) which carry no
/// cross-file signal. Does NOT filter standard library types (String, Integer,
/// List, Map, etc.) because:
/// 1. User-defined types with those names must be trackable
/// 2. Builtin references to non-existent symbols cause zero centrality impact
///    anyway (Step 1b only boosts symbols in the symbols table)
fn is_java_noise_type(name: &str) -> bool {
    // Single-letter names are almost always generic type parameters used in scope.
    // Even when they appear as references (e.g. `: T`), they carry no cross-file signal.
    name.len() == 1
        && name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_uppercase())
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &JavaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE, not all files
    // Bug was: searching all symbols in DB caused wrong file symbols to match
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == extractor.base().file_path)
        .map(|&s| s.clone())
        .collect();

    extractor
        .base()
        .find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
