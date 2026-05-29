/// Identifier extraction for LSP-quality find_references
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, extract_type_arguments};
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

                let identifier = extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
                // Generic method calls: `list.<String>stream()` carry a `type_arguments`
                // field directly on the method_invocation node.
                if let Some(type_args) = node.child_by_field_name("type_arguments") {
                    let arguments =
                        extract_type_arguments(extractor.base(), type_args, decompose_java_type_arg);
                    extractor.base_mut().record_type_arguments(&identifier, arguments);
                }
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

            let identifier = extractor.base_mut().create_identifier(
                &node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
            // If this type_identifier is the name of a `generic_type` use site
            // (e.g. `List` in `List<String>`), record the ordered type arguments.
            // Nested generics are skipped here — they ride along as `children`.
            record_outermost_java_type_arguments(extractor, node, &identifier);
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

/// Record outermost generic type arguments for a `type_identifier` node.
///
/// Fires when `name_node` is the type-name child of a `generic_type` node
/// (e.g. `List` in `List<String>`), but only if that `generic_type` is not
/// itself nested inside a `type_arguments` list (i.e. it is the outermost use
/// site). Nested generics like `List` in `Map<String, List<Integer>>` are
/// captured as `children` of the outer usage, not as separate rows.
fn record_outermost_java_type_arguments(
    extractor: &mut JavaExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(generic_type) = name_node.parent() else {
        return;
    };
    if generic_type.kind() != "generic_type" {
        return;
    }
    // A `generic_type` whose parent is `type_arguments` is itself nested inside
    // another generic — its args ride along under the outer usage as `children`.
    if generic_type
        .parent()
        .map(|p| p.kind() == "type_arguments")
        .unwrap_or(false)
    {
        return;
    }
    let Some(arg_list) = type_arguments_child(generic_type) else {
        return;
    };
    let arguments = extract_type_arguments(extractor.base(), arg_list, decompose_java_type_arg);
    extractor.base_mut().record_type_arguments(identifier, arguments);
}

/// Decompose a single child of a Java `type_arguments` node into a
/// `(type_name, optional_nested_arg_list)` pair for `extract_type_arguments`.
///
/// Java `type_arguments` children may be:
/// - `type_identifier` — a simple reference type (String, Integer, …)
/// - `generic_type`    — a nested generic (List<Integer>)
/// - `wildcard`        — `? extends Foo`, `? super Bar`
/// - primitive/array types — rare as explicit generic args
fn decompose_java_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip commas and punctuation
    }
    match node.kind() {
        "generic_type" => {
            // Nested generic: name comes from the `type_identifier` child.
            let name = {
                let mut cursor = node.walk();
                node.children(&mut cursor)
                    .find(|c| c.kind() == "type_identifier")
                    .map(|n| base.get_node_text(&n))
                    .unwrap_or_else(|| base.get_node_text(&node))
            };
            Some((name, type_arguments_child(node)))
        }
        _ => {
            // type_identifier, wildcard, integral_type, floating_point_type,
            // array_type, scoped_type_identifier, etc. — use full text.
            Some((base.get_node_text(&node), None))
        }
    }
}

/// Find the `type_arguments` child of a `generic_type` node.
fn type_arguments_child(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.kind() == "type_arguments")
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &JavaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    extractor
        .base()
        .find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}
