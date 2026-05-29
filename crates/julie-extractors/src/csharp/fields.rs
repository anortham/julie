use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use tree_sitter::Node;

use super::helpers;

/// Extract field
pub fn extract_field(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    extract_fields(base, node, parent_id).into_iter().next()
}

/// Extract all fields from a declaration
pub fn extract_fields(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Vec<Symbol> {
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);
    let field_type = helpers::extract_field_type(base, &node).unwrap_or_else(|| "var".to_string());
    let annotations = helpers::extract_annotations(base, &node);

    let mut cursor = node.walk();
    let var_declaration = node
        .children(&mut cursor)
        .find(|c| c.kind() == "variable_declaration");
    let Some(var_declaration) = var_declaration else {
        return Vec::new();
    };
    let mut var_cursor = var_declaration.walk();
    let declarators: Vec<Node> = var_declaration
        .children(&mut var_cursor)
        .filter(|c| c.kind() == "variable_declarator")
        .collect();

    let is_constant = modifiers.contains(&"const".to_string())
        || (modifiers.contains(&"static".to_string())
            && modifiers.contains(&"readonly".to_string()));
    let symbol_kind = if is_constant {
        SymbolKind::Constant
    } else {
        SymbolKind::Field
    };

    let doc_comment = base.find_doc_comment(&node);

    declarators
        .into_iter()
        .filter_map(|declarator| {
            let mut decl_cursor = declarator.walk();
            let name_node = declarator
                .children(&mut decl_cursor)
                .find(|c| c.kind() == "identifier")?;
            let name = base.get_node_text(&name_node);
            let initializer = extract_declarator_initializer(base, declarator);

            let signature = if modifiers.is_empty() {
                format!("{} {}{}", field_type, name, initializer)
            } else {
                format!(
                    "{} {} {}{}",
                    modifiers.join(" "),
                    field_type,
                    name,
                    initializer
                )
            };

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility.clone()),
                parent_id: parent_id.clone(),
                doc_comment: doc_comment.clone(),
                annotations: annotations.clone(),
                ..Default::default()
            };

            Some(base.create_symbol(&node, name, symbol_kind.clone(), options))
        })
        .collect()
}

/// Extract event
pub fn extract_event(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    extract_events(base, node, parent_id).into_iter().next()
}

/// Extract all events from an event field declaration
pub fn extract_events(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Vec<Symbol> {
    let mut cursor = node.walk();
    let var_declaration = node
        .children(&mut cursor)
        .find(|c| c.kind() == "variable_declaration");
    let Some(var_declaration) = var_declaration else {
        return Vec::new();
    };

    let mut var_cursor = var_declaration.walk();
    let var_declarators: Vec<Node> = var_declaration
        .children(&mut var_cursor)
        .filter(|c| c.kind() == "variable_declarator")
        .collect();

    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);
    let annotations = helpers::extract_annotations(base, &node);

    let mut type_cursor = var_declaration.walk();
    let type_node = var_declaration
        .children(&mut type_cursor)
        .find(|c| c.kind() != "variable_declarator");
    let event_type = type_node
        .map(|node| base.get_node_text(&node))
        .unwrap_or_else(|| "EventHandler".to_string());

    let doc_comment = base.find_doc_comment(&node);

    var_declarators
        .into_iter()
        .filter_map(|var_declarator| {
            let mut decl_cursor = var_declarator.walk();
            let name_node = var_declarator
                .children(&mut decl_cursor)
                .find(|c| c.kind() == "identifier")?;
            let name = base.get_node_text(&name_node);

            let signature = if modifiers.is_empty() {
                format!("event {} {}", event_type, name)
            } else {
                format!("{} event {} {}", modifiers.join(" "), event_type, name)
            };

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility.clone()),
                parent_id: parent_id.clone(),
                doc_comment: doc_comment.clone(),
                annotations: annotations.clone(),
                ..Default::default()
            };

            Some(base.create_symbol(&node, name, SymbolKind::Event, options))
        })
        .collect()
}

fn extract_declarator_initializer(base: &BaseExtractor, declarator: Node) -> String {
    let children: Vec<Node> = declarator.children(&mut declarator.walk()).collect();
    if let Some(equals_index) = children.iter().position(|c| c.kind() == "=") {
        if equals_index + 1 < children.len() {
            let init_nodes: Vec<String> = children[equals_index + 1..]
                .iter()
                .map(|n| base.get_node_text(n))
                .collect();
            let init_text = init_nodes.join("").trim().to_string();
            if !init_text.is_empty() {
                return format!(" = {}", init_text);
            }
        }
    }

    String::new()
}
