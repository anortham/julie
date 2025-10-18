// C# Operator Declaration Extraction

use super::helpers;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use tree_sitter::Node;

/// Extract operator
pub fn extract_operator(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let operator_symbol = node.children(&mut cursor).find(|c| {
        matches!(
            c.kind(),
            "+" | "-"
                | "*"
                | "/"
                | "=="
                | "!="
                | "<"
                | ">"
                | "<="
                | ">="
                | "!"
                | "~"
                | "++"
                | "--"
                | "%"
                | "&"
                | "|"
                | "^"
                | "<<"
                | ">>"
                | "true"
                | "false"
        )
    })?;

    let operator_text = base.get_node_text(&operator_symbol);
    let name = format!("operator {}", operator_text);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);

    let children: Vec<Node> = node.children(&mut cursor).collect();
    let operator_keyword_index = children
        .iter()
        .position(|c| base.get_node_text(c) == "operator")?;
    let return_type_node = children[..operator_keyword_index]
        .iter()
        .find(|c| matches!(c.kind(), "predefined_type" | "identifier" | "generic_name"));
    let return_type = return_type_node
        .map(|node| base.get_node_text(node))
        .unwrap_or_else(|| "void".to_string());

    let param_list = children.iter().find(|c| c.kind() == "parameter_list");
    let params = param_list
        .map(|p| base.get_node_text(p))
        .unwrap_or_else(|| "()".to_string());

    let mut signature = if modifiers.is_empty() {
        format!("{} operator {}{}", return_type, operator_text, params)
    } else {
        format!(
            "{} {} operator {}{}",
            modifiers.join(" "),
            return_type,
            operator_text,
            params
        )
    };

    if let Some(arrow_clause) = children
        .iter()
        .find(|c| c.kind() == "arrow_expression_clause")
    {
        signature += &format!(" {}", base.get_node_text(arrow_clause));
    }

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Method, options))
}

/// Extract conversion operator
pub fn extract_conversion_operator(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let conversion_type = node
        .children(&mut cursor)
        .find(|c| base.get_node_text(c) == "implicit" || base.get_node_text(c) == "explicit")?;
    let conversion_text = base.get_node_text(&conversion_type);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);

    let children: Vec<Node> = node.children(&mut cursor).collect();
    let operator_keyword_index = children
        .iter()
        .position(|c| base.get_node_text(c) == "operator")?;
    let target_type_node = children[operator_keyword_index + 1..]
        .iter()
        .find(|c| matches!(c.kind(), "predefined_type" | "identifier" | "generic_name"));
    let target_type = target_type_node
        .map(|node| base.get_node_text(node))
        .unwrap_or_else(|| "unknown".to_string());
    let name = format!("{} operator {}", conversion_text, target_type);

    let param_list = children.iter().find(|c| c.kind() == "parameter_list");
    let params = param_list
        .map(|p| base.get_node_text(p))
        .unwrap_or_else(|| "()".to_string());

    let mut signature = if modifiers.is_empty() {
        format!("{} operator {}{}", conversion_text, target_type, params)
    } else {
        format!(
            "{} {} operator {}{}",
            modifiers.join(" "),
            conversion_text,
            target_type,
            params
        )
    };

    if let Some(arrow_clause) = children
        .iter()
        .find(|c| c.kind() == "arrow_expression_clause")
    {
        signature += &format!(" {}", base.get_node_text(arrow_clause));
    }

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Method, options))
}

/// Extract indexer
pub fn extract_indexer(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, None);

    let mut cursor = node.walk();
    let return_type_node = node
        .children(&mut cursor)
        .find(|c| matches!(c.kind(), "predefined_type" | "identifier" | "generic_name"));
    let return_type = return_type_node
        .map(|node| base.get_node_text(&node))
        .unwrap_or_else(|| "object".to_string());

    let bracketed_params = node
        .children(&mut cursor)
        .find(|c| c.kind() == "bracketed_parameter_list");
    let params = bracketed_params
        .map(|p| base.get_node_text(&p))
        .unwrap_or_else(|| "[object index]".to_string());
    let name = format!("this{}", params);

    let signature = if modifiers.is_empty() {
        format!("{} this{}", return_type, params)
    } else {
        format!("{} {} this{}", modifiers.join(" "), return_type, params)
    };

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Property, options))
}
