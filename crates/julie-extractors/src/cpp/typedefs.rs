//! Typedef extraction for C++ symbols
//! Handles `type_definition` nodes: simple typedefs, pointer typedefs,
//! function pointer typedefs, and typedef structs.

use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extract typedef / type alias declaration (`type_definition` node)
/// Handles simple typedefs, pointer typedefs, function pointer typedefs, and typedef structs.
pub(super) fn extract_typedef(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // The typedef name can appear in different places depending on the form:
    //   typedef int ErrorCode;                -> type_identifier child
    //   typedef int* IntPtr;                  -> pointer_declarator > type_identifier
    //   typedef void (*callback_t)(int);      -> function_declarator > parenthesized_declarator > pointer_declarator > type_identifier
    //   typedef struct { ... } Point;         -> type_identifier child (after struct_specifier)
    //   typedef unsigned long size_t;         -> primitive_type child (tree-sitter treats known types as primitive_type)
    let name = find_typedef_name(base, node)?;
    let signature = base.get_node_text(&node);

    // Clean up the signature: collapse whitespace from multi-line typedefs
    let signature = signature
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(';')
        .trim()
        .to_string();

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Type,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Find the typedef alias name from a type_definition node.
/// Searches through various child structures where the name can appear.
fn find_typedef_name(base: &BaseExtractor, node: Node) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    // Strategy 1: Direct type_identifier child (simple typedef or typedef struct)
    // For typedef struct, the type_identifier comes AFTER the struct_specifier
    if let Some(type_id) = children
        .iter()
        .rev() // Search from end - the alias name is typically the last meaningful child
        .find(|c| c.kind() == "type_identifier")
    {
        return Some(base.get_node_text(type_id));
    }

    // Strategy 2: Name inside pointer_declarator (typedef int* IntPtr;)
    if let Some(ptr_decl) = children.iter().find(|c| c.kind() == "pointer_declarator") {
        if let Some(name) = find_type_identifier_recursive(base, *ptr_decl) {
            return Some(name);
        }
    }

    // Strategy 3: Name inside function_declarator (typedef void (*callback_t)(int, float);)
    if let Some(func_decl) = children.iter().find(|c| c.kind() == "function_declarator") {
        if let Some(name) = find_type_identifier_recursive(base, *func_decl) {
            return Some(name);
        }
    }

    // Strategy 4: Last primitive_type child as fallback
    // tree-sitter parses known types like size_t as primitive_type
    if let Some(prim) = children
        .iter()
        .rev()
        .find(|c| c.kind() == "primitive_type")
    {
        // Only use primitive_type if it's the last named child before ';'
        // (to distinguish the alias name from the base type)
        let last_named = children
            .iter()
            .rev()
            .find(|c| c.is_named() && c.kind() != ";" && c.kind() != "comment");
        if last_named.map(|n| n.id()) == Some(prim.id()) {
            return Some(base.get_node_text(prim));
        }
    }

    None
}

/// Recursively search for a type_identifier node in a subtree
fn find_type_identifier_recursive(base: &BaseExtractor, node: Node) -> Option<String> {
    if node.kind() == "type_identifier" {
        return Some(base.get_node_text(&node));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = find_type_identifier_recursive(base, child) {
            return Some(name);
        }
    }
    None
}
