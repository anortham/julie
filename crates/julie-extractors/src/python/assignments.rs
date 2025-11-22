/// Variable and constant assignment extraction
/// Handles variable assignments, type annotations, enum members, and constants
use super::super::base::{Symbol, SymbolKind, SymbolOptions};
use super::PythonExtractor;
use super::{signatures, types};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract an assignment statement - can return multiple symbols for tuple unpacking
pub(super) fn extract_assignment(extractor: &mut PythonExtractor, node: Node) -> Vec<Symbol> {
    // Handle assignments like: x = 5, x: int = 5, self.x = 5, a, b = 1, 2
    let left = match node.child_by_field_name("left") {
        Some(left) => left,
        None => return vec![],
    };
    let right = node.child_by_field_name("right");

    // Handle multiple assignment patterns (a, b = 1, 2)
    if left.kind() == "pattern_list" || left.kind() == "tuple_pattern" {
        return extract_multiple_assignment_targets(extractor, left, right);
    }

    // Handle single assignments
    let (name, mut symbol_kind) = match left.kind() {
        "identifier" => {
            let name = extractor.base_mut().get_node_text(&left);
            (name, SymbolKind::Variable)
        }
        "attribute" => {
            // Handle self.attribute assignments
            let object_node = left.child_by_field_name("object");
            let attribute_node = left.child_by_field_name("attribute");

            if let (Some(object_node), Some(attribute_node)) = (object_node, attribute_node) {
                if extractor.base_mut().get_node_text(&object_node) == "self" {
                    let name = extractor.base_mut().get_node_text(&attribute_node);
                    (name, SymbolKind::Property)
                } else {
                    return vec![]; // Skip non-self attributes for now
                }
            } else {
                return vec![];
            }
        }
        _ => return vec![],
    };

    // Check if this is a special class attribute
    if name == "__slots__" {
        symbol_kind = SymbolKind::Property;
    }
    // Check if it's a constant (uppercase name)
    else if symbol_kind == SymbolKind::Variable && name == name.to_uppercase() && name.len() > 1 {
        // Check if we're inside an enum class
        if types::is_inside_enum_class(extractor, &node) {
            symbol_kind = SymbolKind::EnumMember;
        } else {
            symbol_kind = SymbolKind::Constant;
        }
    }

    // Extract type annotation from assignment node
    let type_annotation = if let Some(type_node) = signatures::find_type_annotation(&node) {
        format!(": {}", extractor.base_mut().get_node_text(&type_node))
    } else {
        String::new()
    };

    // Extract value for signature
    let value = if let Some(right) = right {
        extractor.base_mut().get_node_text(&right)
    } else {
        String::new()
    };

    let signature = format!("{}{} = {}", name, type_annotation, value);

    // Infer visibility from name
    let visibility = signatures::infer_visibility(&name);

    // Parent tracking not yet implemented for assignments
    // Enhancement: Could walk AST to find parent class (see functions.rs:determine_function_kind for pattern)
    let parent_id = None;

    let mut metadata = HashMap::new();
    metadata.insert(
        "hasTypeAnnotation".to_string(),
        serde_json::json!(!type_annotation.is_empty()),
    );

    // Extract doc comment (preceding comments)
    let doc_comment = extractor.base().find_doc_comment(&node);

    vec![extractor.base_mut().create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    )]
}

/// Extract multiple assignment targets from pattern_list or tuple_pattern
/// Example: a, b = 1, 2 extracts both 'a' and 'b' as separate variables
fn extract_multiple_assignment_targets(
    extractor: &mut PythonExtractor,
    left_node: Node,
    right: Option<Node>,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    // Extract the right-hand side value for signature
    let value = if let Some(right) = right {
        extractor.base_mut().get_node_text(&right)
    } else {
        String::new()
    };

    // Iterate through all identifiers in the pattern
    let mut cursor = left_node.walk();
    for child in left_node.children(&mut cursor) {
        if child.kind() == "identifier" {
            let name = extractor.base_mut().get_node_text(&child);

            // Determine if it's a constant (uppercase) or variable
            let symbol_kind = if name == name.to_uppercase() && name.len() > 1 {
                SymbolKind::Constant
            } else {
                SymbolKind::Variable
            };

            // Build signature for this variable
            let signature = format!("{} = {}", name, value);

            // Infer visibility from name
            let visibility = signatures::infer_visibility(&name);

            // Extract doc comment (preceding comments)
            let doc_comment = extractor.base().find_doc_comment(&child);

            // Create symbol for this variable
            let symbol = extractor.base_mut().create_symbol(
                &child,
                name,
                symbol_kind,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: None,
                    metadata: None,
                    doc_comment,
                },
            );

            symbols.push(symbol);
        }
    }

    symbols
}
