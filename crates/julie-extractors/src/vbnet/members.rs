use super::helpers;
use crate::base::{
    AnnotationMarker, BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility,
    normalize_annotations,
};
use crate::test_detection::is_test_symbol;
use tree_sitter::Node;

pub fn extract_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, "public");

    let is_function = node.child_by_field_name("return_type").is_some();
    let keyword = if is_function { "Function" } else { "Sub" };
    let params = helpers::extract_parameters(base, &node);
    let type_params = helpers::extract_type_parameters(base, &node).unwrap_or_default();

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
    let annotations = normalize_annotations(&helpers::extract_attributes(base, &node), "vbnet");
    let annotation_keys = annotations
        .iter()
        .map(|annotation| annotation.annotation_key.clone())
        .collect::<Vec<_>>();

    let mut metadata = helpers::vb_visibility_metadata(&modifiers, "public");
    if is_test_symbol(
        "vbnet",
        &name,
        &base.file_path,
        &SymbolKind::Method,
        &annotation_keys,
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
        annotations,
    };

    Some(base.create_symbol(&node, name, SymbolKind::Method, options))
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
    let visibility = helpers::determine_visibility(&modifiers, "public");
    let params = helpers::extract_parameters(base, &node);

    let signature = format!("{}Sub New{}", helpers::modifier_prefix(&modifiers), params);
    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, "public");
    let annotations = normalize_annotations(&helpers::extract_attributes(base, &node), "vbnet");

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        annotations,
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
    let visibility = helpers::determine_visibility(&modifiers, "public");

    let indexed_params = node
        .child_by_field_name("parameters")
        .map(|p| base.get_node_text(&p));

    let mut signature = format!("{}Property {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(params) = indexed_params {
        signature.push_str(&params);
    }

    if let Some(prop_type) = helpers::extract_as_clause_type(base, &node) {
        signature.push_str(&format!(" As {}", prop_type));
    }

    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, "public");
    let annotations = normalize_annotations(&helpers::extract_attributes(base, &node), "vbnet");

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        annotations,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Property, options))
}

pub fn extract_field(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    extract_fields(base, node, parent_id).into_iter().next()
}

pub fn extract_fields(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Vec<Symbol> {
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, "private");

    let mut declarators = Vec::new();
    collect_descendants_of_kind(node, "variable_declarator", &mut declarators);

    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, "private");
    let annotations = normalize_annotations(&helpers::extract_attributes(base, &node), "vbnet");

    declarators
        .into_iter()
        .filter_map(|declarator| {
            let name_node = declarator.child_by_field_name("name")?;
            let name = base.get_node_text(&name_node);
            let mut signature = format!("{}Dim {}", helpers::modifier_prefix(&modifiers), name);
            append_as_clause(base, declarator, &mut signature);

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility.clone()),
                parent_id: parent_id.clone(),
                metadata: Some(metadata.clone()),
                doc_comment: doc_comment.clone(),
                annotations: annotations.clone(),
                ..Default::default()
            };

            Some(base.create_symbol(&node, name, SymbolKind::Field, options))
        })
        .collect()
}

pub fn extract_event(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, "public");

    let mut signature = format!("{}Event {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(event_type) = helpers::extract_as_clause_type(base, &node) {
        signature.push_str(&format!(" As {}", event_type));
    }

    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, "public");
    let annotations = normalize_annotations(&helpers::extract_attributes(base, &node), "vbnet");

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        annotations,
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
    let visibility = helpers::determine_visibility(&modifiers, "public");
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
    let metadata = helpers::vb_visibility_metadata(&modifiers, "public");
    let annotations = normalize_annotations(&helpers::extract_attributes(base, &node), "vbnet");

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        annotations,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Operator, options))
}

pub fn extract_const(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    extract_consts(base, node, parent_id).into_iter().next()
}

pub fn extract_consts(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Vec<Symbol> {
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers, "public");

    let mut declarators = Vec::new();
    collect_descendants_of_kind(node, "variable_declarator", &mut declarators);

    let doc_comment = base.find_doc_comment(&node);
    let metadata = helpers::vb_visibility_metadata(&modifiers, "public");
    let annotations = normalize_annotations(&helpers::extract_attributes(base, &node), "vbnet");

    if declarators.is_empty() {
        return extract_flat_consts(
            base,
            node,
            parent_id,
            &modifiers,
            visibility,
            metadata,
            doc_comment,
            annotations,
        );
    }

    declarators
        .into_iter()
        .filter_map(|declarator| {
            let name_node = declarator.child_by_field_name("name")?;
            let name = base.get_node_text(&name_node);
            let mut signature = format!("{}Const {}", helpers::modifier_prefix(&modifiers), name);
            append_as_clause(base, declarator, &mut signature);

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility.clone()),
                parent_id: parent_id.clone(),
                metadata: Some(metadata.clone()),
                doc_comment: doc_comment.clone(),
                annotations: annotations.clone(),
                ..Default::default()
            };

            Some(base.create_symbol(&node, name, SymbolKind::Constant, options))
        })
        .collect()
}

fn append_as_clause(base: &BaseExtractor, node: Node, signature: &mut String) {
    if let Some(type_name) = as_clause_type(base, node) {
        signature.push_str(&format!(" As {}", type_name));
    }
}

fn as_clause_type(base: &BaseExtractor, node: Node) -> Option<String> {
    if node.kind() == "as_clause" {
        return node
            .child_by_field_name("type")
            .map(|type_node| base.get_node_text(&type_node));
    }

    let mut cursor = node.walk();
    let as_clause = node.children(&mut cursor).find(|c| c.kind() == "as_clause");
    if let Some(as_clause) = as_clause {
        if let Some(type_node) = as_clause.child_by_field_name("type") {
            return Some(base.get_node_text(&type_node));
        }
    }

    None
}

fn extract_flat_consts(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
    modifiers: &[String],
    visibility: Visibility,
    metadata: std::collections::HashMap<String, serde_json::Value>,
    doc_comment: Option<String>,
    annotations: Vec<AnnotationMarker>,
) -> Vec<Symbol> {
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    let mut constants = Vec::new();
    let mut index = 0;

    while index < children.len() {
        if children[index].kind() != "identifier" {
            index += 1;
            continue;
        }

        let name_node = children[index];
        let mut type_name = None;
        let mut next = index + 1;
        while next < children.len() && children[next].kind() != "identifier" {
            if children[next].kind() == "as_clause" {
                type_name = as_clause_type(base, children[next]);
            }
            next += 1;
        }

        let name = base.get_node_text(&name_node);
        let mut signature = format!("{}Const {}", helpers::modifier_prefix(modifiers), name);
        if let Some(type_name) = type_name {
            signature.push_str(&format!(" As {}", type_name));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility.clone()),
            parent_id: parent_id.clone(),
            metadata: Some(metadata.clone()),
            doc_comment: doc_comment.clone(),
            annotations: annotations.clone(),
            ..Default::default()
        };

        constants.push(base.create_symbol(&node, name, SymbolKind::Constant, options));
        index = next;
    }

    constants
}

fn collect_descendants_of_kind<'a>(node: Node<'a>, kind: &str, matches: &mut Vec<Node<'a>>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            matches.push(child);
        }
        collect_descendants_of_kind(child, kind, matches);
    }
}

pub fn extract_declare(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let default_visibility = if parent_id.is_some() {
        "public"
    } else {
        "friend"
    };
    let visibility = helpers::determine_visibility(&modifiers, default_visibility);

    let is_function = node.child_by_field_name("return_type").is_some();
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
    let metadata = helpers::vb_visibility_metadata(&modifiers, default_visibility);
    let annotations = normalize_annotations(&helpers::extract_attributes(base, &node), "vbnet");

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
        annotations,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Function, options))
}
