/// Import and package declaration extraction
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::extractors::java::JavaExtractor;
use tree_sitter::Node;

/// Extract package declaration from a node
pub(super) fn extract_package(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let scoped_id = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "scoped_identifier")?;

    let package_name = extractor.base().get_node_text(&scoped_id);
    let signature = format!("package {}", package_name);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, package_name, SymbolKind::Namespace, options),
    )
}

/// Extract import declaration from a node
pub(super) fn extract_import(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let scoped_id = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "scoped_identifier")?;

    let mut full_import_path = extractor.base().get_node_text(&scoped_id);

    // Check if it's a static import
    let is_static = node
        .children(&mut node.walk())
        .any(|c| c.kind() == "static");

    // Check for wildcard imports (asterisk node)
    let has_asterisk = node
        .children(&mut node.walk())
        .any(|c| c.kind() == "asterisk");
    if has_asterisk {
        full_import_path.push_str(".*");
    }

    // Extract the class/member name (last part after the last dot)
    let parts: Vec<&str> = full_import_path.split('.').collect();
    let name = parts.last().unwrap_or(&"");

    // Handle wildcard imports
    let (symbol_name, signature) = if *name == "*" {
        let package_name = parts.get(parts.len().saturating_sub(2)).unwrap_or(&"");
        let sig = if is_static {
            format!("import static {}", full_import_path)
        } else {
            format!("import {}", full_import_path)
        };
        (package_name.to_string(), sig)
    } else {
        let sig = if is_static {
            format!("import static {}", full_import_path)
        } else {
            format!("import {}", full_import_path)
        };
        (name.to_string(), sig)
    };

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, symbol_name, SymbolKind::Import, options),
    )
}
