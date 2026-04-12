use crate::base::{BaseExtractor, Visibility};
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

pub fn determine_visibility(modifiers: &[String]) -> Visibility {
    for m in modifiers {
        match m.as_str() {
            "public" => return Visibility::Public,
            "private" => return Visibility::Private,
            "protected" => return Visibility::Protected,
            "friend" => return Visibility::Private,
            _ => {}
        }
    }
    Visibility::Private
}

pub fn get_vb_visibility_string(modifiers: &[String]) -> String {
    for m in modifiers {
        match m.as_str() {
            "public" | "private" | "protected" | "friend" => return m.clone(),
            _ => {}
        }
    }
    "private".to_string()
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
                if inner.kind() != ","
                    && !text.eq_ignore_ascii_case("Implements")
                {
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
