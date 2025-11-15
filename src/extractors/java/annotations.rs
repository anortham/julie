/// Annotation extraction
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use crate::extractors::java::JavaExtractor;
use tree_sitter::Node;

use super::helpers;

/// Extract annotation declaration from a node
pub(super) fn extract_annotation(
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

    // Build signature
    let signature = if modifiers.is_empty() {
        format!("@interface {}", name)
    } else {
        format!("{} @interface {}", modifiers.join(" "), name)
    };

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Interface, options),
    )
}
