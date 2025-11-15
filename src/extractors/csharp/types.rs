// C# Type Declaration Extraction

use super::helpers;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract namespace
pub fn extract_namespace(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "qualified_name" || c.kind() == "identifier")?;

    let name = base.get_node_text(&name_node);
    let signature = format!("namespace {}", name);

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Namespace, options))
}

/// Extract using statement
pub fn extract_using(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node.children(&mut cursor).find(|c| {
        matches!(
            c.kind(),
            "qualified_name" | "identifier" | "member_access_expression"
        )
    })?;

    let full_using_path = base.get_node_text(&name_node);
    let is_static = node.children(&mut cursor).any(|c| c.kind() == "static");

    let mut cursor2 = node.walk();
    let alias_node = node
        .children(&mut cursor2)
        .find(|c| c.kind() == "name_equals");
    let name = if let Some(alias_node) = alias_node {
        let mut alias_cursor = alias_node.walk();
        let alias_identifier = alias_node
            .children(&mut alias_cursor)
            .find(|c| c.kind() == "identifier");
        if let Some(alias_identifier) = alias_identifier {
            base.get_node_text(&alias_identifier)
        } else {
            full_using_path.clone()
        }
    } else {
        full_using_path
            .split('.')
            .next_back()
            .unwrap_or(&full_using_path)
            .to_string()
    };

    let signature = if is_static {
        format!("using static {}", full_using_path)
    } else {
        format!("using {}", full_using_path)
    };

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Import, options))
}

/// Extract class
pub fn extract_class(
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

    let mut signature = if modifiers.is_empty() {
        format!("class {}", name)
    } else {
        format!("{} class {}", modifiers.join(" "), name)
    };

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature = signature.replace(
            &format!("class {}", name),
            &format!("class {}{}", name, type_params),
        );
    }

    let base_list = helpers::extract_base_list(base, &node);
    if !base_list.is_empty() {
        signature += &format!(" : {}", base_list.join(", "));
    }

    let mut node_cursor = node.walk();
    let where_clauses: Vec<String> = node
        .children(&mut node_cursor)
        .filter(|c| c.kind() == "type_parameter_constraints_clause")
        .map(|clause| base.get_node_text(&clause))
        .collect();

    if !where_clauses.is_empty() {
        signature += &format!(" {}", where_clauses.join(" "));
    }

    let mut metadata = HashMap::new();
    let csharp_visibility = helpers::get_csharp_visibility_string(&modifiers);
    metadata.insert(
        "csharp_visibility".to_string(),
        serde_json::Value::String(csharp_visibility),
    );

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
    };

    Some(base.create_symbol(&node, name, SymbolKind::Class, options))
}

/// Extract interface
pub fn extract_interface(
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

    let mut signature = if modifiers.is_empty() {
        format!("interface {}", name)
    } else {
        format!("{} interface {}", modifiers.join(" "), name)
    };

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature = signature.replace(
            &format!("interface {}", name),
            &format!("interface {}{}", name, type_params),
        );
    }

    let base_list = helpers::extract_base_list(base, &node);
    if !base_list.is_empty() {
        signature += &format!(" : {}", base_list.join(", "));
    }

    let mut node_cursor = node.walk();
    let where_clauses: Vec<String> = node
        .children(&mut node_cursor)
        .filter(|c| c.kind() == "type_parameter_constraints_clause")
        .map(|clause| base.get_node_text(&clause))
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

    Some(base.create_symbol(&node, name, SymbolKind::Interface, options))
}

/// Extract struct
pub fn extract_struct(
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

    let mut signature = if modifiers.is_empty() {
        format!("struct {}", name)
    } else {
        format!("{} struct {}", modifiers.join(" "), name)
    };

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature = signature.replace(
            &format!("struct {}", name),
            &format!("struct {}{}", name, type_params),
        );
    }

    let base_list = helpers::extract_base_list(base, &node);
    if !base_list.is_empty() {
        signature += &format!(" : {}", base_list.join(", "));
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

    Some(base.create_symbol(&node, name, SymbolKind::Struct, options))
}

/// Extract enum
pub fn extract_enum(
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

    let mut signature = if modifiers.is_empty() {
        format!("enum {}", name)
    } else {
        format!("{} enum {}", modifiers.join(" "), name)
    };

    let base_list = helpers::extract_base_list(base, &node);
    if !base_list.is_empty() {
        signature += &format!(" : {}", base_list[0]);
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

    Some(base.create_symbol(&node, name, SymbolKind::Enum, options))
}

/// Extract enum member
pub fn extract_enum_member(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")?;

    let name = base.get_node_text(&name_node);

    let mut signature = name.clone();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    if let Some(equals_index) = children.iter().position(|c| c.kind() == "=") {
        if equals_index + 1 < children.len() {
            let value_nodes: Vec<String> = children[equals_index + 1..]
                .iter()
                .map(|n| base.get_node_text(n))
                .collect();
            let value = value_nodes.join("").trim().to_string();
            if !value.is_empty() {
                signature += &format!(" = {}", value);
            }
        }
    }

    // Extract XML doc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::EnumMember, options))
}

/// Extract record
pub fn extract_record(
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

    let is_struct = modifiers.contains(&"struct".to_string())
        || node.children(&mut cursor).any(|c| c.kind() == "struct");

    let record_type = if is_struct { "record struct" } else { "record" };
    let mut signature = if modifiers.is_empty() {
        format!("{} {}", record_type, name)
    } else {
        format!("{} {} {}", modifiers.join(" "), record_type, name)
    };

    if let Some(param_list) = node
        .children(&mut cursor)
        .find(|c| c.kind() == "parameter_list")
    {
        signature += &base.get_node_text(&param_list);
    }

    if let Some(base_list) = node.children(&mut cursor).find(|c| c.kind() == "base_list") {
        signature += &format!(" {}", base.get_node_text(&base_list));
    }

    let symbol_kind = if is_struct {
        SymbolKind::Struct
    } else {
        SymbolKind::Class
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
