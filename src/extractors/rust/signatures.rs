/// Rust function signatures and related declarations
/// - Function signatures (extern functions)
/// - Associated types
/// - Return type extraction
/// - Macro invocations
/// - Use declarations
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::extractors::rust::RustExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract function return type from a function node
pub(super) fn extract_return_type(
    base: &crate::extractors::base::BaseExtractor,
    node: Node,
) -> String {
    let return_type_node = node.child_by_field_name("return_type");

    if let Some(ret_type) = return_type_node {
        // Skip the -> token and get the actual type
        let type_nodes: Vec<_> = ret_type
            .children(&mut ret_type.walk())
            .filter(|c| c.kind() != "->" && base.get_node_text(c) != "->")
            .collect();

        if !type_nodes.is_empty() {
            return type_nodes
                .iter()
                .map(|n| base.get_node_text(n))
                .collect::<Vec<_>>()
                .join("");
        }
    }

    String::new()
}

/// Extract function signature (for extern functions)
pub(super) fn extract_function_signature(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    // Extract parameters
    let params_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "parameters");
    let params = params_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "()".to_string());

    // Extract return type (after -> token)
    let children: Vec<_> = node.children(&mut node.walk()).collect();
    let arrow_index = children.iter().position(|c| c.kind() == "->");
    let return_type = if let Some(index) = arrow_index {
        if index + 1 < children.len() {
            format!(" -> {}", base.get_node_text(&children[index + 1]))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let signature = format!("fn {}{}{}", name, params, return_type);

    base.create_symbol(
        &node,
        name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public), // extern functions are typically public
            parent_id,
            doc_comment: None,
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract associated type in a trait
pub(super) fn extract_associated_type(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    // Extract trait bounds (: Debug + Clone, etc.)
    let trait_bounds = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "trait_bounds")
        .map(|c| base.get_node_text(&c))
        .unwrap_or_default();

    let signature = format!("type {}{}", name, trait_bounds);

    base.create_symbol(
        &node,
        name,
        SymbolKind::Type,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public), // associated types in traits are public
            parent_id,
            doc_comment: None,
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract macro invocation (for code generation patterns)
pub(super) fn extract_macro_invocation(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let base = extractor.get_base_mut();
    let macro_name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier");
    let macro_name = macro_name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_default();

    // Look for struct-generating macros or known patterns
    if macro_name.contains("struct") || macro_name.contains("generate") {
        let token_tree_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "token_tree");
        if let Some(token_tree) = token_tree_node {
            // Extract the first identifier from the token tree as the struct name
            let struct_name_node = token_tree
                .children(&mut token_tree.walk())
                .find(|c| c.kind() == "identifier");
            if let Some(struct_name_node) = struct_name_node {
                let struct_name = base.get_node_text(&struct_name_node);
                let signature = format!("struct {}", struct_name);

                return Some(base.create_symbol(
                    &node,
                    struct_name,
                    SymbolKind::Class,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public), // assume macro-generated types are public
                        parent_id,
                        doc_comment: None,
                        metadata: Some(HashMap::new()),
                    },
                ));
            }
        }
    }

    None
}

/// Extract use statement (imports)
pub(super) fn extract_use(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let base = extractor.get_base_mut();
    let use_text = base.get_node_text(&node);

    // Simple pattern matching for common use cases
    if use_text.contains(" as ") {
        // use std::collections::HashMap as Map;
        let parts: Vec<&str> = use_text.split(" as ").collect();
        if parts.len() == 2 {
            let alias = parts[1].replace(";", "").trim().to_string();
            return Some(base.create_symbol(
                &node,
                alias,
                SymbolKind::Import,
                SymbolOptions {
                    signature: Some(use_text),
                    visibility: Some(Visibility::Public),
                    parent_id,
                    doc_comment: None,
                    metadata: Some(HashMap::new()),
                },
            ));
        }
    } else {
        // use std::collections::HashMap;
        if let Some(captures) = regex::Regex::new(r"use\s+(?:.*::)?(\w+)\s*;")
            .ok()
            .and_then(|re| re.captures(&use_text))
        {
            if let Some(name_match) = captures.get(1) {
                let name = name_match.as_str().to_string();
                return Some(base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Import,
                    SymbolOptions {
                        signature: Some(use_text),
                        visibility: Some(Visibility::Public),
                        parent_id,
                        doc_comment: None,
                        metadata: Some(HashMap::new()),
                    },
                ));
            }
        }
    }

    None
}
