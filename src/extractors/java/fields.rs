/// Field and property extraction
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use crate::extractors::java::JavaExtractor;
use tree_sitter::Node;

use super::helpers;

/// Extract field declaration from a node
pub(super) fn extract_field(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Get type
    let type_node = node.children(&mut node.walk()).find(|c| {
        matches!(
            c.kind(),
            "type_identifier"
                | "generic_type"
                | "array_type"
                | "primitive_type"
                | "boolean_type"
                | "integral_type"
                | "floating_point_type"
                | "void_type"
        )
    });
    let field_type = type_node
        .map(|n| extractor.base().get_node_text(&n))
        .unwrap_or_else(|| "unknown".to_string());

    // Get variable declarator(s) - there can be multiple fields in one declaration
    let declarators: Vec<Node> = node
        .children(&mut node.walk())
        .filter(|c| c.kind() == "variable_declarator")
        .collect();

    // For now, handle the first declarator (we could extend to handle multiple)
    let declarator = declarators.first()?;
    let name_node = declarator
        .children(&mut declarator.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);

    // Check if it's a constant (static final)
    let is_constant =
        modifiers.contains(&"static".to_string()) && modifiers.contains(&"final".to_string());
    let symbol_kind = if is_constant {
        SymbolKind::Constant
    } else {
        SymbolKind::Property
    };

    // Get initializer if present
    let children: Vec<Node> = declarator.children(&mut declarator.walk()).collect();
    let assign_index = children.iter().position(|c| c.kind() == "=");
    let initializer = if let Some(idx) = assign_index {
        let init_nodes: Vec<String> = children[(idx + 1)..]
            .iter()
            .map(|n| extractor.base().get_node_text(n))
            .collect();
        format!(" = {}", init_nodes.join(""))
    } else {
        String::new()
    };

    // Build signature
    let modifier_str = if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    };
    let signature = format!("{}{} {}{}", modifier_str, field_type, name, initializer);

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, symbol_kind, options),
    )
}
