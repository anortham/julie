use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract local function declarations inside method bodies.
pub(super) fn extract_local_function(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name").or_else(|| {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let param_list_index = children.iter().position(|c| c.kind() == "parameter_list")?;
        children[..param_list_index]
            .iter()
            .rev()
            .find(|c| c.kind() == "identifier")
            .copied()
    })?;

    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);

    let return_type = node
        .child_by_field_name("type")
        .map(|node| base.get_node_text(&node))
        .unwrap_or_else(|| "void".to_string());
    let params = node
        .child_by_field_name("parameters")
        .map(|node| base.get_node_text(&node))
        .unwrap_or_else(|| "()".to_string());
    let type_params = node
        .child_by_field_name("type_parameters")
        .map(|node| base.get_node_text(&node))
        .unwrap_or_default();
    let type_param_part = if type_params.is_empty() {
        String::new()
    } else {
        type_params
    };

    let mut signature = if modifiers.is_empty() {
        format!("{} {}{}{}", return_type, name, type_param_part, params)
    } else {
        format!(
            "{} {} {}{}{}",
            modifiers.join(" "),
            return_type,
            name,
            type_param_part,
            params
        )
    };

    if let Some(body) = node.child_by_field_name("body") {
        if body.kind() == "arrow_expression_clause" {
            signature.push(' ');
            signature.push_str(&base.get_node_text(&body));
        }
    }

    let mut cursor = node.walk();
    let where_clauses: Vec<String> = node
        .children(&mut cursor)
        .filter(|c| c.kind() == "type_parameter_constraints_clause")
        .map(|clause| base.get_node_text(&clause))
        .collect();
    if !where_clauses.is_empty() {
        signature += &format!(" {}", where_clauses.join(" "));
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("local_function".to_string()),
    );

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Function, options))
}

/// Extract lambda or anonymous method when a stable naming context exists.
pub(super) fn extract_lambda(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name = stable_lambda_name(base, node)?;

    let params = match node.kind() {
        "lambda_expression" => node
            .child_by_field_name("parameters")
            .map(|params| base.get_node_text(&params))
            .unwrap_or_else(|| "()".to_string()),
        "anonymous_method_expression" => node
            .child_by_field_name("parameters")
            .map(|params| base.get_node_text(&params))
            .unwrap_or_else(|| "()".to_string()),
        _ => return None,
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("lambda".to_string()),
    );

    let options = SymbolOptions {
        signature: Some(format!("lambda {}", params)),
        parent_id,
        metadata: Some(metadata),
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Function, options))
}

fn stable_lambda_name(base: &BaseExtractor, node: Node) -> Option<String> {
    let parent = node.parent()?;
    match parent.kind() {
        "variable_declarator" => {
            let name_node = parent.child_by_field_name("name")?;
            let name = base.get_node_text(&name_node);
            Some(format!("{}$lambda", name))
        }
        "assignment_expression" => {
            let left = parent.child_by_field_name("left")?;
            let candidate = terminal_identifier_from_text(&base.get_node_text(&left))?;
            Some(format!("{}$lambda", candidate))
        }
        "arrow_expression_clause" => {
            let member = parent.parent()?;
            let member_name = stable_member_name(base, member)?;
            Some(format!("{}$lambda", member_name))
        }
        _ => None,
    }
}

fn stable_member_name(base: &BaseExtractor, node: Node) -> Option<String> {
    match node.kind() {
        "method_declaration"
        | "local_function_statement"
        | "property_declaration"
        | "event_declaration"
        | "delegate_declaration" => node
            .child_by_field_name("name")
            .map(|name| base.get_node_text(&name)),
        "field_declaration" | "event_field_declaration" => {
            let mut cursor = node.walk();
            let declarator = node
                .children(&mut cursor)
                .find(|child| child.kind() == "variable_declaration")?;
            let mut declarator_cursor = declarator.walk();
            let first_name = declarator
                .children(&mut declarator_cursor)
                .find(|child| child.kind() == "variable_declarator")
                .and_then(|var| var.child_by_field_name("name"))?;
            Some(base.get_node_text(&first_name))
        }
        _ => None,
    }
}

fn terminal_identifier_from_text(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|part| !part.is_empty())
        .next_back()
        .map(|part| part.to_string())
}
