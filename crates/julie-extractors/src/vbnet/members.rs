use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use crate::test_detection::is_test_symbol;
use std::collections::HashMap;
use tree_sitter::Node;

pub fn extract_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut cursor = node.walk();
    let is_function = node.children(&mut cursor).any(|c| {
        base.get_node_text(&c).eq_ignore_ascii_case("Function")
    });

    let keyword = if is_function { "Function" } else { "Sub" };
    let params = helpers::extract_parameters(base, &node);
    let type_params = helpers::extract_type_parameters(base, &node)
        .unwrap_or_default();

    let mut signature = format!(
        "{}{} {}{}{}",
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
    let attributes = helpers::extract_attributes(base, &node);

    let mut metadata = HashMap::new();
    if is_test_symbol(
        "vbnet",
        &name,
        &base.file_path,
        &SymbolKind::Function,
        &[],
        &attributes,
        doc_comment.as_deref(),
    ) {
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
    }

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        metadata: if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        },
    };

    Some(base.create_symbol(&node, name, SymbolKind::Function, options))
}

pub fn extract_abstract_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    extract_method(base, node, parent_id)
}

pub fn extract_constructor(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name = "New".to_string();
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);
    let params = helpers::extract_parameters(base, &node);

    let signature = format!("{}Sub New{}", helpers::modifier_prefix(&modifiers), params);
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

pub fn extract_property(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let indexed_params = node
        .child_by_field_name("parameters")
        .map(|p| base.get_node_text(&p));

    let mut signature = format!(
        "{}Property {}",
        helpers::modifier_prefix(&modifiers),
        name
    );

    if let Some(params) = indexed_params {
        signature.push_str(&params);
    }

    if let Some(prop_type) = helpers::extract_as_clause_type(base, &node) {
        signature.push_str(&format!(" As {}", prop_type));
    }

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

pub fn extract_field(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut cursor = node.walk();
    let declarator = node
        .children(&mut cursor)
        .find(|c| c.kind() == "variable_declarator")?;

    let name_node = declarator.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);

    let mut signature = format!(
        "{}Dim {}",
        helpers::modifier_prefix(&modifiers),
        name
    );

    let mut decl_cursor = declarator.walk();
    let as_clause = declarator
        .children(&mut decl_cursor)
        .find(|c| c.kind() == "as_clause");
    if let Some(as_clause) = as_clause {
        if let Some(type_node) = as_clause.child_by_field_name("type") {
            signature.push_str(&format!(" As {}", base.get_node_text(&type_node)));
        }
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Field, options))
}

pub fn extract_event(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut signature = format!(
        "{}Event {}",
        helpers::modifier_prefix(&modifiers),
        name
    );

    if let Some(event_type) = helpers::extract_as_clause_type(base, &node) {
        signature.push_str(&format!(" As {}", event_type));
    }

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

pub fn extract_operator(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let op_node = node.child_by_field_name("operator")?;
    let op = base.get_node_text(&op_node);
    let name = format!("operator {}", op);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);
    let params = helpers::extract_parameters(base, &node);

    let mut signature = format!(
        "{}Operator {}{}",
        helpers::modifier_prefix(&modifiers),
        op,
        params
    );

    if let Some(rt) = helpers::extract_return_type(base, &node) {
        signature.push_str(&format!(" As {}", rt));
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Operator, options))
}

pub fn extract_const(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut signature = format!(
        "{}Const {}",
        helpers::modifier_prefix(&modifiers),
        name
    );

    let mut cursor = node.walk();
    let as_clause = node
        .children(&mut cursor)
        .find(|c| c.kind() == "as_clause");
    if let Some(as_clause) = as_clause {
        if let Some(type_node) = as_clause.child_by_field_name("type") {
            signature.push_str(&format!(" As {}", base.get_node_text(&type_node)));
        }
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Constant, options))
}

pub fn extract_declare(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut cursor = node.walk();
    let is_function = node.children(&mut cursor).any(|c| {
        base.get_node_text(&c).eq_ignore_ascii_case("Function")
    });

    let keyword = if is_function { "Function" } else { "Sub" };
    let params = helpers::extract_parameters(base, &node);

    let lib = node
        .child_by_field_name("library")
        .map(|l| base.get_node_text(&l))
        .unwrap_or_default();

    let mut signature = format!(
        "{}Declare {} {} Lib {}{}",
        helpers::modifier_prefix(&modifiers),
        keyword,
        name,
        lib,
        params
    );

    if is_function {
        if let Some(rt) = helpers::extract_return_type(base, &node) {
            signature.push_str(&format!(" As {}", rt));
        }
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Function, options))
}
