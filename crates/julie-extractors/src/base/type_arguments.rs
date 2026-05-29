// Shared core for capturing ordered, nested generic type arguments at use
// sites (Miller bridge Phase 2). The recursion + ordinal assignment live here;
// the per-language node-kind specifics are supplied by a `decompose` function.

use tree_sitter::Node;

use super::BaseExtractor;
use super::types::TypeArgument;

/// Maps a candidate child of a type-argument list to its applied argument.
///
/// Returns `Some((type_name, nested_list))` when `node` is a type argument
/// worth recording — `nested_list` is the inner type-argument-list node to
/// recurse into for nested generics (e.g. the `<int>` inside `List<int>`), or
/// `None` for a leaf. Returns `None` to skip non-type children (punctuation
/// such as `<`, `,`, `>`). The input and output nodes share a tree lifetime so
/// the recursion can descend into the returned nested node.
pub type TypeArgDecomposer =
    for<'a> fn(&BaseExtractor, Node<'a>) -> Option<(String, Option<Node<'a>>)>;

/// Recursively extract ordered, nested type arguments from a type-argument-list
/// node (C# `type_argument_list`, TS `type_arguments`, …).
///
/// Walks the list's children in document order, assigning 0-based ordinals to
/// the children `decompose` accepts, and recurses into each accepted child's
/// nested list (if any) with the same `decompose`. Punctuation and other
/// non-type children are skipped without consuming an ordinal.
pub fn extract_type_arguments(
    base: &BaseExtractor,
    arg_list_node: Node<'_>,
    decompose: TypeArgDecomposer,
) -> Vec<TypeArgument> {
    let mut arguments = Vec::new();
    let mut cursor = arg_list_node.walk();
    let mut ordinal: u32 = 0;
    for child in arg_list_node.children(&mut cursor) {
        if let Some((type_name, nested)) = decompose(base, child) {
            let children = match nested {
                Some(nested_list) => extract_type_arguments(base, nested_list, decompose),
                None => Vec::new(),
            };
            arguments.push(TypeArgument {
                ordinal,
                type_name,
                children,
            });
            ordinal += 1;
        }
    }
    arguments
}
