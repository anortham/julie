/// Variable and constant assignment extraction
/// Handles variable assignments, type annotations, enum members, and constants

use super::super::base::{Symbol, SymbolKind, SymbolOptions};
use super::{signatures, types};
use super::PythonExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract an assignment statement
pub(super) fn extract_assignment(
    extractor: &mut PythonExtractor,
    node: Node,
) -> Option<Symbol> {
    // Handle assignments like: x = 5, x: int = 5, self.x = 5
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right");

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
                    return None; // Skip non-self attributes for now
                }
            } else {
                return None;
            }
        }
        "pattern_list" | "tuple_pattern" => {
            // Handle multiple assignment: a, b = 1, 2
            return None; // TODO: Handle multiple assignments
        }
        _ => return None,
    };

    // Check if this is a special class attribute
    if name == "__slots__" {
        symbol_kind = SymbolKind::Property;
    }
    // Check if it's a constant (uppercase name)
    else if symbol_kind == SymbolKind::Variable
        && name == name.to_uppercase()
        && name.len() > 1
    {
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

    // TODO: Handle parent_id for nested assignments
    let parent_id = None;

    let mut metadata = HashMap::new();
    metadata.insert(
        "hasTypeAnnotation".to_string(),
        serde_json::json!(!type_annotation.is_empty()),
    );

    Some(extractor.base_mut().create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            metadata: Some(metadata),
            doc_comment: None,
        },
    ))
}
