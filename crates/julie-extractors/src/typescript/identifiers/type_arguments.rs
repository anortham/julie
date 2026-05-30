use crate::base::{BaseExtractor, Identifier, extract_type_arguments};
use crate::typescript::TypeScriptExtractor;
use tree_sitter::Node;

/// First `type_arguments` child of a `generic_type` (its `<...>`), if any.
fn type_arguments_child(generic_type: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = generic_type.walk();
    generic_type
        .children(&mut cursor)
        .find(|child| child.kind() == "type_arguments")
}

/// `TypeArgDecomposer` for TypeScript: maps a child of a `type_arguments` list to its
/// applied argument. Skips unnamed nodes (punctuation `<`, `,`, `>`); for a nested
/// `generic_type` returns the base name plus its inner `type_arguments` to recurse into;
/// for every other named type node returns its source text as a leaf.
pub(super) fn decompose_ts_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None;
    }
    match node.kind() {
        "generic_type" => {
            // The `name` field of a `generic_type` is its head identifier/type_identifier.
            let name = node
                .child_by_field_name("name")
                .map(|n| base.get_node_text(&n))
                .unwrap_or_else(|| base.get_node_text(&node));
            let nested = type_arguments_child(node);
            Some((name, nested))
        }
        _ => Some((base.get_node_text(&node), None)),
    }
}

/// If `name_node` is the `name` field of an *outermost* `generic_type` use site
/// (e.g. the `Base` in `extends Base<Foo,Bar>` or the `Map` in `field: Map<K,V>`),
/// record that generic's ordered/nested applied type arguments against `identifier`.
pub(super) fn record_outermost_generic_type_arguments_ts(
    extractor: &mut TypeScriptExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(generic_type) = name_node.parent() else {
        return;
    };
    if generic_type.kind() != "generic_type" {
        return;
    }
    // A generic_type whose parent is type_arguments is itself nested inside another
    // generic — its args ride along under the outer usage.
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
    let arguments = extract_type_arguments(extractor.base(), arg_list, decompose_ts_type_arg);
    extractor
        .base_mut()
        .record_type_arguments(identifier, arguments);
}

/// Check if a `type_identifier` node is a declaration name rather than a type reference.
pub(super) fn is_type_declaration_name(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        // Check if this node is the `name` field of a declaration or type param
        if let Some(name_node) = parent.child_by_field_name("name") {
            if name_node.id() == node.id() {
                return matches!(
                    parent.kind(),
                    "interface_declaration"
                        | "type_alias_declaration"
                        | "class_declaration"
                        | "abstract_class_declaration"
                        | "type_parameter"
                        | "mapped_type_clause"
                );
            }
        }
    }
    false
}

/// Returns true for TypeScript types that are too common to be meaningful
/// type references for centrality scoring.
pub(super) fn is_ts_noise_type(name: &str) -> bool {
    // Single-letter names are almost always generic type parameters used in scope.
    // Even when they appear as references (e.g. `: T`), they carry no cross-file signal.
    if name.len() == 1
        && name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_uppercase())
    {
        return true;
    }

    // TypeScript compiler utility types — these are never user-defined
    matches!(
        name,
        "Record"
            | "Partial"
            | "Required"
            | "Readonly"
            | "Pick"
            | "Omit"
            | "Exclude"
            | "Extract"
            | "NonNullable"
            | "ReturnType"
            | "Parameters"
            | "InstanceType"
            | "ConstructorParameters"
            | "ThisType"
            | "Awaited"
    )
}
