// CSS Extractor Helpers - Property extraction and classification utilities

use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

pub(super) struct PropertyHelper;

impl PropertyHelper {
    /// Extract properties from a declaration block
    pub(super) fn extract_properties(
        base: &BaseExtractor,
        declaration_block: Option<&Node>,
    ) -> Vec<String> {
        let mut properties = Vec::new();

        if let Some(block) = declaration_block {
            let mut cursor = block.walk();
            for child in block.children(&mut cursor) {
                if child.kind() == "declaration" {
                    let prop = base.get_node_text(&child);
                    properties.push(prop);
                }
            }
        }

        properties
    }

    /// Check if property is unique/interesting
    pub(super) fn is_unique_property(property: &str) -> bool {
        property.contains("calc(")
            || property.contains("var(")
            || property.contains("attr(")
            || property.contains("url(")
            || property.contains("linear-gradient")
            || property.contains("radial-gradient")
            || property.contains("rgba(")
            || property.contains("hsla(")
            || property.contains("repeat(")
            || property.contains("minmax(")
            || property.contains("clamp(")
            || property.contains("min(")
            || property.contains("max(")
            || property.starts_with("grid-")
            || property.starts_with("flex-")
            || property.contains("transform")
            || property.contains("animation")
            || property.contains("transition")
    }

    /// Extract key properties for signature
    pub(super) fn extract_key_properties(
        base: &BaseExtractor,
        declaration_block: &Node,
        selector: Option<&str>,
    ) -> Vec<String> {
        let important_properties = [
            "display",
            "position",
            "background",
            "color",
            "font-family",
            "font-weight",
            "grid-template",
            "grid-area",
            "flex",
            "margin",
            "padding",
            "width",
            "height",
            "transform",
            "text-decoration",
            "box-shadow",
            "border",
            "backdrop-filter",
            "linear-gradient",
            "max-width",
            "text-align",
            "cursor",
            "opacity",
            "content",
        ];

        let mut all_props = Vec::new();
        let mut custom_props = Vec::new();
        let mut important_props = Vec::new();
        let mut unique_props = Vec::new();

        let mut cursor = declaration_block.walk();
        for child in declaration_block.children(&mut cursor) {
            if child.kind() == "declaration" {
                let prop_text = base.get_node_text(&child).trim().to_string();

                // Remove trailing semicolon (';' is ASCII, safe to slice)
                let clean_prop = if prop_text.ends_with(';') {
                    let new_len = prop_text.len() - 1;
                    if prop_text.is_char_boundary(new_len) {
                        prop_text[..new_len].to_string()
                    } else {
                        prop_text
                    }
                } else {
                    prop_text
                };

                all_props.push(clean_prop.clone());

                // Categorize properties following reference logic
                if clean_prop.starts_with("--") {
                    custom_props.push(clean_prop);
                } else if important_properties
                    .iter()
                    .any(|&prop| clean_prop.starts_with(prop))
                {
                    important_props.push(clean_prop);
                } else if Self::is_unique_property(&clean_prop) {
                    unique_props.push(clean_prop);
                }
            }
        }

        let mut key_properties = Vec::new();

        // Special handling for :root selector - include all CSS custom properties
        if let Some(sel) = selector {
            if sel == ":root" && !custom_props.is_empty() {
                key_properties.extend(custom_props); // Include ALL CSS variables for :root
                key_properties.extend(important_props.into_iter().take(3));
                key_properties.extend(unique_props.into_iter().take(2));
            } else {
                // Normal priority system
                key_properties.extend(custom_props.into_iter().take(12)); // More space for CSS variables
                key_properties.extend(important_props.into_iter().take(5));
                key_properties.extend(unique_props.into_iter().take(3));

                // Fill remaining space with other properties
                for prop in all_props {
                    if !key_properties.contains(&prop) && key_properties.len() < 12 {
                        key_properties.push(prop);
                    }
                }
            }
        } else {
            // Default behavior when no selector provided
            key_properties.extend(custom_props.into_iter().take(12));
            key_properties.extend(important_props.into_iter().take(5));
            key_properties.extend(unique_props.into_iter().take(3));
        }

        key_properties
    }

    /// Find declaration block in rule
    #[allow(clippy::manual_find)]
    pub(super) fn find_declaration_block<'a>(node: &Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "block" {
                return Some(child);
            }
        }
        None
    }

    /// Find property value
    pub(super) fn find_property_value<'a>(property_node: &Node<'a>) -> Option<Node<'a>> {
        if let Some(parent) = property_node.parent() {
            if parent.kind() == "declaration" {
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    match child.kind() {
                        "property_value" | "integer_value" | "plain_value" => {
                            return Some(child);
                        }
                        _ => {}
                    }
                }
            }
        }
        None
    }
}
