/// Helper utilities for Rust extractor
/// - Impl block tracking
/// - Visibility and attribute extraction
/// - Keyword detection
use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

/// Information about an impl block (stored by byte range for safety)
#[derive(Debug, Clone)]
pub struct ImplBlockInfo {
    /// Byte range of the impl block in the source file (safe to store)
    pub start_byte: usize,
    pub end_byte: usize,
    pub type_name: String,
    pub parent_id: Option<String>,
}

/// Extract visibility modifier from a node (pub, pub(crate), etc.)
pub(super) fn extract_visibility(base: &BaseExtractor, node: Node) -> String {
    let visibility_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "visibility_modifier");

    if let Some(vis_node) = visibility_node {
        let vis_text = base.get_node_text(&vis_node);
        if vis_text == "pub" {
            "pub ".to_string()
        } else if vis_text.starts_with("pub(") {
            format!("{} ", vis_text)
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

/// Get preceding attributes (like #[derive(...)]) for a node
pub(super) fn get_preceding_attributes<'a>(_base: &BaseExtractor, node: Node<'a>) -> Vec<Node<'a>> {
    let mut attributes = Vec::new();

    if let Some(parent) = node.parent() {
        let siblings: Vec<_> = parent.children(&mut parent.walk()).collect();
        if let Some(node_index) = siblings.iter().position(|&n| n.id() == node.id()) {
            // Look backwards for attribute_item nodes
            for i in (0..node_index).rev() {
                let sibling = siblings[i];
                if sibling.kind() == "attribute_item" {
                    attributes.insert(0, sibling);
                } else {
                    break; // Stop at the first non-attribute
                }
            }
        }
    }

    attributes
}

/// Extract trait names from #[derive(...)] attributes
pub(super) fn extract_derived_traits(base: &BaseExtractor, attributes: &[Node]) -> Vec<String> {
    let mut traits = Vec::new();

    for attr in attributes {
        // Look for derive attribute
        let attribute_node = attr
            .children(&mut attr.walk())
            .find(|c| c.kind() == "attribute");

        if let Some(attr_node) = attribute_node {
            let identifier_node = attr_node
                .children(&mut attr_node.walk())
                .find(|c| c.kind() == "identifier");

            if let Some(ident) = identifier_node {
                if base.get_node_text(&ident) == "derive" {
                    // Find the token tree with the trait list
                    let token_tree = attr_node
                        .children(&mut attr_node.walk())
                        .find(|c| c.kind() == "token_tree");

                    if let Some(tree) = token_tree {
                        for child in tree.children(&mut tree.walk()) {
                            if child.kind() == "identifier" {
                                traits.push(base.get_node_text(&child));
                            }
                        }
                    }
                }
            }
        }
    }

    traits
}

/// Check if node is inside an impl block
pub(super) fn is_inside_impl(node: Node) -> bool {
    let mut parent = node.parent();
    while let Some(p) = parent {
        if p.kind() == "impl_item" {
            return true;
        }
        parent = p.parent();
    }
    false
}

/// Check if node has async keyword
pub(super) fn has_async_keyword(base: &BaseExtractor, node: Node) -> bool {
    node.children(&mut node.walk())
        .any(|c| c.kind() == "async" || base.get_node_text(&c) == "async")
}

/// Check if node has unsafe keyword
pub(super) fn has_unsafe_keyword(base: &BaseExtractor, node: Node) -> bool {
    node.children(&mut node.walk())
        .any(|c| c.kind() == "unsafe" || base.get_node_text(&c) == "unsafe")
}

/// Extract extern modifier from a function node
pub(super) fn extract_extern_modifier(base: &BaseExtractor, node: Node) -> String {
    let function_modifiers_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "function_modifiers");

    if let Some(modifiers) = function_modifiers_node {
        let extern_modifier_node = modifiers
            .children(&mut modifiers.walk())
            .find(|c| c.kind() == "extern_modifier");

        if let Some(extern_node) = extern_modifier_node {
            return base.get_node_text(&extern_node);
        }
    }

    String::new()
}

/// Find doc comment preceding a node (/// or #[doc = "..."])
pub(super) fn find_doc_comment(base: &BaseExtractor, node: Node) -> Option<String> {
    // Look for preceding doc comments (///)
    if let Some(parent) = node.parent() {
        let siblings: Vec<_> = parent.children(&mut parent.walk()).collect();
        if let Some(node_index) = siblings.iter().position(|&n| n.id() == node.id()) {
            if node_index > 0 {
                let prev_sibling = siblings[node_index - 1];
                if prev_sibling.kind() == "line_comment" {
                    let comment_text = base.get_node_text(&prev_sibling);
                    // Rust doc comments start with ///
                    if let Some(doc_text) = comment_text.strip_prefix("///") {
                        return Some(doc_text.trim().to_string());
                    }
                }
            }
        }
    }

    // Look for attribute doc comments like #[doc = "..."]
    let attributes = get_preceding_attributes(base, node);
    for attr in &attributes {
        if let Some(doc_comment) = extract_doc_from_attribute(base, *attr) {
            return Some(doc_comment);
        }
    }

    None
}

/// Extract doc string from #[doc = "..."] attribute
pub(super) fn extract_doc_from_attribute(base: &BaseExtractor, node: Node) -> Option<String> {
    let attr_text = base.get_node_text(&node);
    if let Some(captures) = regex::Regex::new(r#"#\[doc\s*=\s*"([^"]+)"\]"#)
        .ok()
        .and_then(|re| re.captures(&attr_text))
    {
        if let Some(doc_match) = captures.get(1) {
            return Some(doc_match.as_str().to_string());
        }
    }
    None
}

/// Find the containing function for a given node
pub(super) fn find_containing_function(base: &BaseExtractor, node: Node) -> Option<String> {
    let mut parent = node.parent();

    while let Some(p) = parent {
        if p.kind() == "function_item" {
            let name_node = p.child_by_field_name("name");
            return name_node.map(|n| base.get_node_text(&n));
        }
        parent = p.parent();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_visibility_public() {
        // Visibility extraction is tested through integration tests
    }

    #[test]
    fn test_is_inside_impl() {
        // is_inside_impl is tested through integration tests
    }
}
