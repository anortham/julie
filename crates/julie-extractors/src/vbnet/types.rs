use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

pub fn extract_namespace(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let signature = format!("Namespace {}", name);
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

pub fn extract_imports(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    let alias_node = children.iter().find(|c| c.kind() == "identifier");
    let has_equals = children.iter().any(|c| c.kind() == "=");

    if has_equals {
        if let Some(alias_id) = alias_node {
            let name = base.get_node_text(alias_id);
            let ns_node = children.iter().find(|c| c.kind() == "namespace_name");
            let full_path = ns_node
                .map(|n| base.get_node_text(n))
                .unwrap_or_else(|| name.clone());
            let signature = format!("Imports {} = {}", name, full_path);
            let doc_comment = base.find_doc_comment(&node);

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                doc_comment,
                ..Default::default()
            };

            return Some(base.create_symbol(&node, name, SymbolKind::Import, options));
        }
    }

    let ns_node = children.iter().find(|c| c.kind() == "namespace_name")?;
    let full_path = base.get_node_text(ns_node);
    let name = full_path
        .split('.')
        .next_back()
        .unwrap_or(&full_path)
        .to_string();
    let signature = format!("Imports {}", full_path);
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

pub fn extract_class(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let default_visibility = helpers::default_type_visibility(parent_id.as_ref());
    let visibility = helpers::determine_visibility(&modifiers, default_visibility);

    let mut signature = format!("{}Class {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature.push_str(&type_params);
    }

    let inherits = helpers::extract_inherits(base, &node);
    if !inherits.is_empty() {
        signature.push_str(&format!(" Inherits {}", inherits.join(", ")));
    }

    let implements = helpers::extract_implements(base, &node);
    if !implements.is_empty() {
        signature.push_str(&format!(" Implements {}", implements.join(", ")));
    }

    let metadata = helpers::vb_visibility_metadata(&modifiers, default_visibility);

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        annotations: Vec::new(),
    };

    Some(base.create_symbol(&node, name, SymbolKind::Class, options))
}

pub fn extract_module(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let default_visibility = helpers::default_type_visibility(parent_id.as_ref());
    let visibility = helpers::determine_visibility(&modifiers, default_visibility);

    let signature = format!("{}Module {}", helpers::modifier_prefix(&modifiers), name);

    let mut metadata = helpers::vb_visibility_metadata(&modifiers, default_visibility);
    metadata.insert("vb_module".to_string(), serde_json::Value::Bool(true));

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        annotations: Vec::new(),
    };

    Some(base.create_symbol(&node, name, SymbolKind::Class, options))
}

pub fn extract_structure(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let default_visibility = helpers::default_type_visibility(parent_id.as_ref());
    let visibility = helpers::determine_visibility(&modifiers, default_visibility);

    let mut signature = format!("{}Structure {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature.push_str(&type_params);
    }

    let implements = helpers::extract_implements(base, &node);
    if !implements.is_empty() {
        signature.push_str(&format!(" Implements {}", implements.join(", ")));
    }

    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, default_visibility);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Struct, options))
}

pub fn extract_interface(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let default_visibility = helpers::default_type_visibility(parent_id.as_ref());
    let visibility = helpers::determine_visibility(&modifiers, default_visibility);

    let mut signature = format!("{}Interface {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature.push_str(&type_params);
    }

    let inherits = helpers::extract_inherits(base, &node);
    if !inherits.is_empty() {
        signature.push_str(&format!(" Inherits {}", inherits.join(", ")));
    }

    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, default_visibility);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Interface, options))
}

pub fn extract_enum(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let default_visibility = helpers::default_type_visibility(parent_id.as_ref());
    let visibility = helpers::determine_visibility(&modifiers, default_visibility);

    let mut signature = format!("{}Enum {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(underlying) = node.child_by_field_name("underlying_type") {
        signature.push_str(&format!(" As {}", base.get_node_text(&underlying)));
    }

    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, default_visibility);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Enum, options))
}

pub fn extract_enum_member(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);

    let mut signature = name.clone();
    if let Some(value_node) = node.child_by_field_name("value") {
        let value = base.get_node_text(&value_node);
        signature.push_str(&format!(" = {}", value));
    }

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

pub fn extract_delegate(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let default_visibility = helpers::default_type_visibility(parent_id.as_ref());
    let visibility = helpers::determine_visibility(&modifiers, default_visibility);

    let is_function = node.child_by_field_name("return_type").is_some();
    let keyword = if is_function { "Function" } else { "Sub" };
    let params = helpers::extract_parameters(base, &node);
    let type_params = helpers::extract_type_parameters(base, &node).unwrap_or_default();

    let mut signature = format!(
        "{}Delegate {} {}{}{}",
        helpers::modifier_prefix(&modifiers),
        keyword,
        name,
        type_params,
        params
    );

    if is_function {
        if let Some(rt) = helpers::extract_return_type(base, &node) {
            signature.push_str(&format!(" As {}", rt));
        }
    }

    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, default_visibility);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Delegate, options))
}
