// C# Member Declaration Extraction

use super::helpers;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extract method
pub fn extract_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    let param_list_index = children.iter().position(|c| c.kind() == "parameter_list")?;
    let name_node = children[..param_list_index]
        .iter()
        .rev()
        .find(|c| c.kind() == "identifier")?;

    let name = base.get_node_text(name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);
    let return_type =
        helpers::extract_return_type(base, &node).unwrap_or_else(|| "void".to_string());
    let param_list = children.iter().find(|c| c.kind() == "parameter_list");
    let params = param_list
        .map(|p| base.get_node_text(p))
        .unwrap_or_else(|| "()".to_string());
    let type_params = helpers::extract_type_parameters(base, &node);

    let modifier_str = if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    };
    let type_param_str = type_params
        .as_ref()
        .map(|tp| format!("{} ", tp))
        .unwrap_or_default();
    let mut signature = format!(
        "{}{}{} {}{}",
        modifier_str, type_param_str, return_type, name, params
    );

    let arrow_clause = children
        .iter()
        .find(|c| c.kind() == "arrow_expression_clause");
    if let Some(arrow_clause) = arrow_clause {
        signature += &format!(" {}", base.get_node_text(arrow_clause));
    }

    let where_clauses: Vec<String> = children
        .iter()
        .filter(|c| c.kind() == "type_parameter_constraints_clause")
        .map(|clause| base.get_node_text(clause))
        .collect();
    if !where_clauses.is_empty() {
        signature += &format!(" {}", where_clauses.join(" "));
    }

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Method, options))
}

/// Extract constructor
pub fn extract_constructor(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, Some("constructor_declaration"));
    let param_list = node
        .children(&mut cursor)
        .find(|c| c.kind() == "parameter_list");
    let params = param_list
        .map(|p| base.get_node_text(&p))
        .unwrap_or_else(|| "()".to_string());
    let signature = if modifiers.is_empty() {
        format!("{}{}", name, params)
    } else {
        format!("{} {}{}", modifiers.join(" "), name, params)
    };

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Constructor, options))
}

/// Extract destructor
pub fn extract_destructor(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")?;
    let class_name = base.get_node_text(&name_node);
    let name = format!("~{}", class_name);
    let param_list = node
        .children(&mut cursor)
        .find(|c| c.kind() == "parameter_list");
    let params = param_list
        .map(|p| base.get_node_text(&p))
        .unwrap_or_else(|| "()".to_string());
    let signature = format!("~{}{}", class_name, params);

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Protected),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Method, options))
}

/// Extract property
pub fn extract_property(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);
    let prop_type =
        helpers::extract_property_type(base, &node).unwrap_or_else(|| "var".to_string());
    let accessor_list = node
        .children(&mut cursor)
        .find(|c| c.kind() == "accessor_list");
    let accessors = if let Some(accessor_list) = accessor_list {
        format!(" {}", base.get_node_text(&accessor_list))
    } else {
        let arrow_clause = node
            .children(&mut cursor)
            .find(|c| c.kind() == "arrow_expression_clause");
        if let Some(arrow_clause) = arrow_clause {
            format!(" {}", base.get_node_text(&arrow_clause))
        } else {
            String::new()
        }
    };

    let signature = if modifiers.is_empty() {
        format!("{} {}{}", prop_type, name, accessors)
    } else {
        format!(
            "{} {} {}{}",
            modifiers.join(" "),
            prop_type,
            name,
            accessors
        )
    };

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Property, options))
}

/// Extract field
pub fn extract_field(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);
    let field_type = helpers::extract_field_type(base, &node).unwrap_or_else(|| "var".to_string());

    let mut cursor = node.walk();
    let var_declaration = node
        .children(&mut cursor)
        .find(|c| c.kind() == "variable_declaration")?;
    let mut var_cursor = var_declaration.walk();
    let declarators: Vec<Node> = var_declaration
        .children(&mut var_cursor)
        .filter(|c| c.kind() == "variable_declarator")
        .collect();

    let declarator = declarators.first()?;
    let mut decl_cursor = declarator.walk();
    let name_node = declarator
        .children(&mut decl_cursor)
        .find(|c| c.kind() == "identifier")?;
    let name = base.get_node_text(&name_node);

    let is_constant = modifiers.contains(&"const".to_string())
        || (modifiers.contains(&"static".to_string())
            && modifiers.contains(&"readonly".to_string()));
    let symbol_kind = if is_constant {
        SymbolKind::Constant
    } else {
        SymbolKind::Field
    };

    let children: Vec<Node> = declarator.children(&mut decl_cursor).collect();
    let initializer = if let Some(equals_index) = children.iter().position(|c| c.kind() == "=") {
        if equals_index + 1 < children.len() {
            let init_nodes: Vec<String> = children[equals_index + 1..]
                .iter()
                .map(|n| base.get_node_text(n))
                .collect();
            let init_text = init_nodes.join("").trim().to_string();
            if !init_text.is_empty() {
                format!(" = {}", init_text)
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

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

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, symbol_kind, options))
}

/// Extract event
pub fn extract_event(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let var_declaration = node
        .children(&mut cursor)
        .find(|c| c.kind() == "variable_declaration")?;
    let mut var_cursor = var_declaration.walk();
    let var_declarator = var_declaration
        .children(&mut var_cursor)
        .find(|c| c.kind() == "variable_declarator")?;
    let mut decl_cursor = var_declarator.walk();
    let name_node = var_declarator
        .children(&mut decl_cursor)
        .find(|c| c.kind() == "identifier")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);

    let type_node = var_declaration
        .children(&mut var_cursor)
        .find(|c| c.kind() != "variable_declarator");
    let event_type = type_node
        .map(|node| base.get_node_text(&node))
        .unwrap_or_else(|| "EventHandler".to_string());

    let signature = if modifiers.is_empty() {
        format!("event {} {}", event_type, name)
    } else {
        format!("{} event {} {}", modifiers.join(" "), event_type, name)
    };

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Event, options))
}

/// Extract delegate
pub fn extract_delegate(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    let _delegate_keyword = children.iter().find(|c| c.kind() == "delegate")?;
    let delegate_index = children.iter().position(|c| c.kind() == "delegate")?;

    let identifiers_after_delegate: Vec<&Node> = children[delegate_index + 1..]
        .iter()
        .filter(|c| c.kind() == "identifier")
        .collect();

    let name_node = if identifiers_after_delegate.len() == 1 {
        identifiers_after_delegate[0]
    } else if identifiers_after_delegate.len() >= 2 {
        identifiers_after_delegate[1]
    } else {
        return None;
    };

    let name = base.get_node_text(name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);

    let mut return_type = "void".to_string();
    for child in &children[delegate_index + 1..] {
        if matches!(
            child.kind(),
            "predefined_type" | "identifier" | "qualified_name" | "generic_name"
        ) {
            return_type = base.get_node_text(child);
            break;
        }
    }

    let param_list = children.iter().find(|c| c.kind() == "parameter_list");
    let params = param_list
        .map(|p| base.get_node_text(p))
        .unwrap_or_else(|| "()".to_string());
    let type_params = helpers::extract_type_parameters(base, &node);

    let modifier_str = if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    };
    let name_with_type_params = type_params
        .map(|tp| format!("{}{}", name, tp))
        .unwrap_or_else(|| name.clone());
    let signature = format!(
        "{}delegate {} {}{}",
        modifier_str, return_type, name_with_type_params, params
    );

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Delegate, options))
}
