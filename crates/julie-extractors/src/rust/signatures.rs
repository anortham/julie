/// Rust function signatures and related declarations
/// - Function signatures (extern functions)
/// - Associated types
/// - Return type extraction
/// - Macro invocations
/// - Use declarations
use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::rust::RustExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract function return type from a function node
pub(super) fn extract_return_type(base: &crate::base::BaseExtractor, node: Node) -> String {
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
) -> Option<Symbol> {
    let base = extractor.get_base_mut();
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier");
    let name = name_node.map(|n| base.get_node_text(&n))?;

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

    Some(base.create_symbol(
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
    ))
}

/// Extract associated type in a trait
pub(super) fn extract_associated_type(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let base = extractor.get_base_mut();
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_identifier");
    let name = name_node.map(|n| base.get_node_text(&n))?;

    // Extract trait bounds (: Debug + Clone, etc.)
    let trait_bounds = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "trait_bounds")
        .map(|c| base.get_node_text(&c))
        .unwrap_or_default();

    let signature = format!("type {}{}", name, trait_bounds);

    Some(base.create_symbol(
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
    ))
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
    let macro_name = macro_name_node.map(|n| base.get_node_text(&n))?;

    if macro_name.is_empty() {
        return None;
    }

    // Extract all macro invocations as symbols
    let signature = format!("{}!(..)", macro_name);

    Some(base.create_symbol(
        &node,
        macro_name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            doc_comment: None,
            metadata: Some(HashMap::new()),
        },
    ))
}

/// Extract use statement (imports)
///
/// Handles four patterns:
/// 1. Grouped imports: `use foo::{Bar, Baz}` — name is path prefix, signature is full text
/// 2. Glob imports: `use foo::*` — name is path prefix, signature is full text
/// 3. Aliased imports: `use foo::Bar as B` — name is alias
/// 4. Simple imports: `use foo::Bar` — name is last identifier
pub(super) fn extract_use(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let base = extractor.get_base_mut();
    let use_text = base.get_node_text(&node);

    // Strip visibility + "use" keyword to get the path portion
    let path_text = use_text
        .trim_start_matches("pub(crate) use ")
        .trim_start_matches("pub(super) use ")
        .trim_start_matches("pub use ")
        .trim_start_matches("use ")
        .trim_end_matches(';')
        .trim();

    // Case 1: Grouped imports — use foo::{Bar, Baz}
    // Check before aliased imports since groups may contain inner "as" clauses
    if path_text.contains('{') {
        let name = path_text
            .split("::{")
            .next()
            .unwrap_or(path_text)
            .trim()
            .to_string();
        let name = if name.is_empty() {
            path_text.to_string()
        } else {
            name
        };
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

    // Case 2: Glob imports — use foo::*
    if path_text.ends_with("::*") || path_text == "*" {
        let name = path_text.trim_end_matches("::*").trim().to_string();
        let name = if name.is_empty() {
            "*".to_string()
        } else {
            name
        };
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

    // Case 3: Aliased imports — use foo::Bar as B
    if use_text.contains(" as ") {
        let parts: Vec<&str> = use_text.split(" as ").collect();
        if parts.len() == 2 {
            let alias = parts[1].replace(';', "").trim().to_string();
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
    }

    // Case 4: Simple imports — use foo::Bar
    // Extract the last path segment as the name
    let name = path_text.rsplit("::").next().unwrap_or(path_text).trim();
    if !name.is_empty() {
        return Some(base.create_symbol(
            &node,
            name.to_string(),
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

    None
}
