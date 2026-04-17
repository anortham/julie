use crate::base::{BaseExtractor, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

pub fn extract_modifiers(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let modifiers_node = node.child_by_field_name("modifiers");
    let Some(modifiers_node) = modifiers_node else {
        return Vec::new();
    };

    let mut cursor = modifiers_node.walk();
    modifiers_node
        .children(&mut cursor)
        .filter(|c| c.kind() == "modifier")
        .map(|c| base.get_node_text(&c).to_lowercase())
        .collect()
}

pub fn determine_visibility(modifiers: &[String], default_visibility: &str) -> Visibility {
    match get_vb_visibility_string(modifiers, default_visibility).as_str() {
        "public" => Visibility::Public,
        "protected" | "protected friend" => Visibility::Protected,
        "friend" | "private" | "private protected" => Visibility::Private,
        _ => Visibility::Private,
    }
}

pub fn get_vb_visibility_string(modifiers: &[String], default_visibility: &str) -> String {
    let has_public = modifiers.iter().any(|m| m == "public");
    let has_private = modifiers.iter().any(|m| m == "private");
    let has_protected = modifiers.iter().any(|m| m == "protected");
    let has_friend = modifiers.iter().any(|m| m == "friend");

    if has_public {
        "public".to_string()
    } else if has_private && has_protected {
        "private protected".to_string()
    } else if has_protected && has_friend {
        "protected friend".to_string()
    } else if has_private {
        "private".to_string()
    } else if has_protected {
        "protected".to_string()
    } else if has_friend {
        "friend".to_string()
    } else {
        default_visibility.to_string()
    }
}

pub fn vb_visibility_metadata(
    modifiers: &[String],
    default_visibility: &str,
) -> HashMap<String, Value> {
    let mut metadata = HashMap::new();
    metadata.insert(
        "vb_visibility".to_string(),
        Value::String(get_vb_visibility_string(modifiers, default_visibility)),
    );
    metadata
}

pub fn default_type_visibility(parent_id: Option<&String>) -> &'static str {
    if parent_id.is_some() {
        "public"
    } else {
        "friend"
    }
}

pub fn extract_return_type(base: &BaseExtractor, node: &Node) -> Option<String> {
    let rt = node.child_by_field_name("return_type")?;
    Some(base.get_node_text(&rt))
}

pub fn extract_parameters(base: &BaseExtractor, node: &Node) -> String {
    node.child_by_field_name("parameters")
        .map(|p| base.get_node_text(&p))
        .unwrap_or_else(|| "()".to_string())
}

pub fn extract_type_parameters(base: &BaseExtractor, node: &Node) -> Option<String> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.kind() == "type_parameters")
        .map(|tp| base.get_node_text(&tp))
}

pub fn extract_as_clause_type(base: &BaseExtractor, node: &Node) -> Option<String> {
    let mut cursor = node.walk();
    let as_clause = node
        .children(&mut cursor)
        .find(|c| c.kind() == "as_clause")?;
    let type_node = as_clause.child_by_field_name("type")?;
    Some(base.get_node_text(&type_node))
}

pub fn extract_inherits(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let inherits_node = node.child_by_field_name("inherits");
    let Some(inherits_node) = inherits_node else {
        return Vec::new();
    };

    let mut cursor = inherits_node.walk();
    inherits_node
        .children(&mut cursor)
        .filter(|c| c.kind() != "," && !base.get_node_text(c).eq_ignore_ascii_case("Inherits"))
        .map(|c| base.get_node_text(&c))
        .collect()
}

pub fn extract_implements(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "implements_clause" {
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                let text = base.get_node_text(&inner);
                if inner.kind() != "," && !text.eq_ignore_ascii_case("Implements") {
                    result.push(text);
                }
            }
        }
    }
    result
}

pub fn extract_attributes(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_block" {
            let mut inner_cursor = child.walk();
            for attr in child.children(&mut inner_cursor) {
                if attr.kind() == "attribute" {
                    if let Some(name_node) = attr.child_by_field_name("name") {
                        attrs.push(base.get_node_text(&name_node));
                    }
                }
            }
        }
    }
    attrs
}

pub fn modifier_prefix(modifiers: &[String]) -> String {
    if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    }
}

pub fn unresolved_type_target(type_name: &str) -> Option<crate::base::UnresolvedTarget> {
    let normalized = normalize_type_name(type_name)?;
    let parts: Vec<String> = normalized
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect();

    if parts.is_empty() {
        return None;
    }

    let terminal_name = parts.last().cloned()?;
    let namespace_path = if parts.len() > 1 {
        parts[..parts.len() - 1].to_vec()
    } else {
        Vec::new()
    };

    Some(crate::base::UnresolvedTarget {
        display_name: parts.join("."),
        terminal_name,
        receiver: None,
        namespace_path,
        import_context: None,
    })
}

fn normalize_type_name(type_name: &str) -> Option<String> {
    let mut normalized = type_name.trim().to_string();
    if normalized.is_empty() {
        return None;
    }

    while let Some(stripped) = normalized.strip_suffix("()") {
        normalized = stripped.trim_end().to_string();
    }

    if let Some(generic_start) = normalized.find("(Of") {
        normalized.truncate(generic_start);
        normalized = normalized.trim_end().to_string();
    }

    if let Some(generic_start) = normalized.find("(of") {
        normalized.truncate(generic_start);
        normalized = normalized.trim_end().to_string();
    }

    let predefined = [
        "boolean", "byte", "sbyte", "short", "ushort", "integer", "uinteger", "long", "ulong",
        "single", "double", "decimal", "char", "string", "date", "object",
    ];

    if predefined.contains(&normalized.to_ascii_lowercase().as_str()) {
        return None;
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}
