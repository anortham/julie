use super::helpers::{
    extract_extern_modifier, extract_visibility, find_doc_comment, has_async_keyword,
    has_unsafe_keyword, is_inside_impl, ImplBlockInfo,
};
use super::signatures::extract_return_type;
/// Rust function and method extraction
/// - Functions and methods
/// - Impl blocks and two-phase processing
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::extractors::rust::RustExtractor;
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract function parameters from a function node
pub(super) fn extract_function_parameters(
    base: &crate::extractors::base::BaseExtractor,
    node: Node,
) -> Vec<String> {
    let mut parameters = Vec::new();
    let param_list = node.child_by_field_name("parameters");

    if let Some(params) = param_list {
        for child in params.children(&mut params.walk()) {
            if child.kind() == "parameter" {
                let param_text = base.get_node_text(&child);
                parameters.push(param_text);
            } else if child.kind() == "self_parameter" {
                // Handle &self, &mut self, self, etc.
                let self_text = base.get_node_text(&child);
                parameters.push(self_text);
            }
        }
    }

    parameters
}

/// Extract function or method definition
pub(super) fn extract_function(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    // Determine if this is a method (inside impl block) or standalone function
    let kind = if is_inside_impl(node) {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    };

    // Extract function signature components
    let visibility = extract_visibility(base, node);
    let is_async = has_async_keyword(base, node);
    let is_unsafe = has_unsafe_keyword(base, node);
    let extern_modifier = extract_extern_modifier(base, node);
    let params = extract_function_parameters(base, node);
    let return_type = extract_return_type(base, node);

    // Extract generic type parameters
    let type_params = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_parameters")
        .map(|c| base.get_node_text(&c))
        .unwrap_or_default();

    // Extract where clause
    let where_clause = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "where_clause")
        .map(|c| format!(" {}", base.get_node_text(&c)))
        .unwrap_or_default();

    // Build signature
    let mut signature = String::new();
    if !visibility.is_empty() {
        signature.push_str(&visibility);
    }
    if !extern_modifier.is_empty() {
        signature.push_str(&format!("{} ", extern_modifier));
    }
    if is_unsafe {
        signature.push_str("unsafe ");
    }
    if is_async {
        signature.push_str("async ");
    }
    signature.push_str(&format!("fn {}{}", name, type_params));
    signature.push_str(&format!("({})", params.join(", ")));
    if !return_type.is_empty() {
        signature.push_str(&format!(" -> {}", return_type));
    }
    signature.push_str(&where_clause);

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Store information about an impl block for phase 2 processing
pub(super) fn extract_impl(extractor: &mut RustExtractor, node: Node, _parent_id: Option<String>) {
    let base = extractor.get_base_mut();
    // Store impl block info for phase 2 processing
    let type_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_identifier");
    let type_name = type_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    // SAFETY FIX: Store byte ranges instead of Node references
    // This avoids unsafe lifetime transmutation and is safe to store
    extractor.add_impl_block(ImplBlockInfo {
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        type_name,
        parent_id: None,
    });
}

/// Process impl blocks during phase 2
/// Extracts methods from impl blocks and links them to their parent types
pub(super) fn process_impl_blocks(
    extractor: &mut RustExtractor,
    tree: &Tree,
    symbols: &mut Vec<Symbol>,
) {
    let impl_blocks = extractor.get_impl_blocks().to_vec();

    for impl_block in impl_blocks {
        // Find the struct/enum this impl is for
        let struct_symbol = symbols.iter().find(|s| {
            s.name == impl_block.type_name
                && (s.kind == SymbolKind::Class || s.kind == SymbolKind::Interface)
        });

        let parent_id = struct_symbol.map(|s| s.id.clone());

        // SAFETY FIX: Reconstruct node from byte range using the tree
        // This is safe because we have a valid tree reference with proper lifetime
        let node = tree
            .root_node()
            .descendant_for_byte_range(impl_block.start_byte, impl_block.end_byte);

        if let Some(node) = node {
            // Extract methods with correct parent_id (or none for cross-file impls)
            if let Some(declaration_list) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "declaration_list")
            {
                for child in declaration_list.children(&mut declaration_list.walk()) {
                    if child.kind() == "function_item" {
                        let mut method_symbol =
                            extract_function(extractor, child, parent_id.clone());
                        method_symbol.kind = SymbolKind::Method;

                        // Preserve the impl type name in metadata so cross-file methods stay discoverable
                        let metadata = method_symbol.metadata.get_or_insert_with(HashMap::new);
                        metadata.insert(
                            "impl_type_name".to_string(),
                            Value::String(impl_block.type_name.clone()),
                        );
                        metadata.insert(
                            "impl_parent_id_resolved".to_string(),
                            Value::Bool(parent_id.is_some()),
                        );

                        symbols.push(method_symbol);
                    }
                }
            }
        }
    }
}
