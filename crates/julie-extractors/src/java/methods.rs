/// Method and constructor extraction
use crate::base::{Symbol, SymbolKind, SymbolOptions};
use crate::java::JavaExtractor;
use crate::test_detection::is_test_symbol;
use std::collections::HashMap;
use tree_sitter::Node;

use super::helpers;

/// Extract method declaration from a node
pub(super) fn extract_method(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Get return type (comes before the method name in the AST)
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    let name_index = children.iter().position(|c| c.id() == name_node.id())?;

    let return_type_node = children[0..name_index].iter().find(|c| {
        matches!(
            c.kind(),
            "type_identifier"
                | "generic_type"
                | "void_type"
                | "array_type"
                | "primitive_type"
                | "integral_type"
                | "floating_point_type"
                | "boolean_type"
        )
    });
    let return_type = return_type_node
        .map(|n| extractor.base().get_node_text(n))
        .unwrap_or_else(|| "void".to_string());

    // Get parameters
    let param_list = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "formal_parameters");
    let params = param_list
        .map(|p| extractor.base().get_node_text(&p))
        .unwrap_or_else(|| "()".to_string());

    // Handle generic type parameters on the method
    let type_params = helpers::extract_type_parameters(extractor.base(), node);

    // Check for throws clause
    let throws_clause = helpers::extract_throws_clause(extractor.base(), node);

    // Build signature
    let modifier_str = if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    };
    let type_param_str = type_params.map(|tp| format!("{} ", tp)).unwrap_or_default();
    let throws_str = throws_clause
        .map(|tc| format!(" {}", tc))
        .unwrap_or_default();

    let signature = format!(
        "{}{}{} {}{}{}",
        modifier_str, type_param_str, return_type, name, params, throws_str
    );

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    // Extract annotations for test detection (modifiers starting with '@')
    let annotations: Vec<String> = modifiers
        .iter()
        .filter(|m| m.starts_with('@'))
        .map(|m| m.strip_prefix('@').unwrap_or(m).to_string())
        .collect();

    let mut metadata = HashMap::new();
    if is_test_symbol(
        "java",
        &name,
        &extractor.base().file_path,
        &SymbolKind::Method,
        &annotations,
        &[],
        doc_comment.as_deref(),
    ) {
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
    }

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        metadata: if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        },
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Method, options),
    )
}

/// Extract constructor declaration from a node
pub(super) fn extract_constructor(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Get parameters
    let param_list = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "formal_parameters");
    let params = param_list
        .map(|p| extractor.base().get_node_text(&p))
        .unwrap_or_else(|| "()".to_string());

    // Build signature (constructors don't have return types)
    let modifier_str = if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    };
    let signature = format!("{}{}{}", modifier_str, name, params);

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    // Extract annotations for test detection
    let annotations: Vec<String> = modifiers
        .iter()
        .filter(|m| m.starts_with('@'))
        .map(|m| m.strip_prefix('@').unwrap_or(m).to_string())
        .collect();

    let mut metadata = HashMap::new();
    if is_test_symbol(
        "java",
        &name,
        &extractor.base().file_path,
        &SymbolKind::Constructor,
        &annotations,
        &[],
        doc_comment.as_deref(),
    ) {
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
    }

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        metadata: if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        },
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Constructor, options),
    )
}
