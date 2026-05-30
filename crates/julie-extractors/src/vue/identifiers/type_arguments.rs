use crate::base::TypeArgument;
use tree_sitter::Node;

use super::get_node_text_from_content;

/// Recursively extract ordered, nested type arguments from a TypeScript
/// `type_arguments` node, reading type names from the Vue script section text.
pub(super) fn extract_vue_type_arguments<'a>(
    arg_list_node: Node<'a>,
    script_content: &str,
) -> Vec<TypeArgument> {
    let mut arguments = Vec::new();
    let mut ordinal: u32 = 0;
    let children: Vec<Node<'a>> = arg_list_node.children(&mut arg_list_node.walk()).collect();
    for child in children {
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "generic_type" => {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| get_node_text_from_content(&n, script_content))
                    .unwrap_or_else(|| get_node_text_from_content(&child, script_content));
                let nested_children: Vec<Node<'a>> = child.children(&mut child.walk()).collect();
                let nested_arg_list = nested_children
                    .iter()
                    .find(|c| c.kind() == "type_arguments")
                    .copied();
                let children = nested_arg_list
                    .map(|nested| extract_vue_type_arguments(nested, script_content))
                    .unwrap_or_default();
                arguments.push(TypeArgument {
                    ordinal,
                    type_name: name,
                    children,
                });
                ordinal += 1;
            }
            _ => {
                arguments.push(TypeArgument {
                    ordinal,
                    type_name: get_node_text_from_content(&child, script_content),
                    children: Vec::new(),
                });
                ordinal += 1;
            }
        }
    }
    arguments
}
