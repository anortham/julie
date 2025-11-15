use crate::extractors::base::BaseExtractor;
use regex::Regex;
use std::collections::HashMap;

/// Attribute handling and signature building
pub(super) struct AttributeHandler;

impl AttributeHandler {
    /// Build element signature with tag name and attributes
    pub(super) fn build_element_signature(
        tag_name: &str,
        attributes: &HashMap<String, String>,
        text_content: Option<&str>,
    ) -> String {
        let mut signature = format!("<{}", tag_name);

        // Include important attributes in signature
        let important_attrs = Self::get_important_attributes(tag_name, attributes);
        for (name, value) in important_attrs {
            if value.is_empty() {
                // Boolean attributes like 'novalidate', 'disabled', etc.
                signature.push_str(&format!(" {}", name));
            } else {
                signature.push_str(&format!(r#" {}="{}""#, name, value));
            }
        }

        signature.push('>');

        // Include text content for certain elements
        if let Some(content) = text_content {
            if Self::should_include_text_content(tag_name) {
                // Safely truncate UTF-8 string at character boundary
                let truncated_content = BaseExtractor::truncate_string(&content, 100);
                signature.push_str(&truncated_content);
            }
        }

        signature
    }

    /// Get important attributes for an element signature
    fn get_important_attributes(
        tag_name: &str,
        attributes: &HashMap<String, String>,
    ) -> Vec<(String, String)> {
        let mut important = Vec::new();
        let priority_attrs = Self::get_priority_attributes_for_tag(tag_name);

        // Add priority attributes first
        for attr_name in &priority_attrs {
            if let Some(value) = attributes.get(attr_name) {
                important.push((attr_name.clone(), value.clone()));
            }
        }

        // Add other interesting attributes with limit
        let max_attrs = if tag_name == "img" { 12 } else { 8 };
        for (name, value) in attributes {
            if !priority_attrs.contains(name)
                && Self::is_interesting_attribute(name)
                && important.len() < max_attrs
            {
                important.push((name.clone(), value.clone()));
            }
        }

        important
    }

    /// Get priority attributes for specific tag types
    fn get_priority_attributes_for_tag(tag_name: &str) -> Vec<String> {
        let mut common_priority = vec!["id".to_string(), "class".to_string(), "role".to_string()];

        let tag_specific = match tag_name {
            "html" => vec!["lang", "dir", "data-theme"],
            "meta" => vec!["name", "property", "content", "charset"],
            "link" => vec!["rel", "href", "type", "as"],
            "script" => vec!["src", "type", "async", "defer"],
            "img" => vec![
                "src", "alt", "width", "height", "loading", "decoding", "sizes", "srcset",
            ],
            "a" => vec!["href", "target", "rel"],
            "form" => vec!["action", "method", "enctype", "novalidate"],
            "input" => vec![
                "type",
                "name",
                "value",
                "placeholder",
                "required",
                "disabled",
                "autocomplete",
                "pattern",
                "min",
                "max",
                "step",
                "accept",
            ],
            "select" => vec!["name", "id", "multiple", "required", "disabled"],
            "textarea" => vec![
                "name",
                "placeholder",
                "required",
                "disabled",
                "maxlength",
                "minlength",
                "rows",
                "cols",
            ],
            "time" => vec!["datetime"],
            "details" => vec!["open"],
            "button" => vec!["type", "data-action", "disabled"],
            "iframe" => vec![
                "src",
                "title",
                "width",
                "height",
                "allowfullscreen",
                "allow",
                "loading",
            ],
            "video" => vec!["src", "controls", "autoplay", "preload", "poster"],
            "audio" => vec!["src", "controls", "preload"],
            "source" => vec!["src", "type", "media", "srcset"],
            "track" => vec!["src", "kind", "srclang", "label", "default"],
            "svg" => vec!["viewBox", "xmlns", "role", "aria-labelledby"],
            "animate" => vec!["attributeName", "values", "dur", "repeatCount"],
            "rect" => vec!["x", "y", "width", "height", "fill"],
            "circle" => vec!["cx", "cy", "r", "fill"],
            "path" => vec!["d", "fill", "stroke"],
            "object" => vec!["type", "data", "width", "height"],
            "embed" => vec!["type", "src", "width", "height"],
            "body" => vec!["class", "data-theme"],
            "custom-video-player" => vec!["src", "controls", "width", "height"],
            "image-gallery" => vec!["images", "layout", "lazy-loading"],
            "data-visualization" => vec!["type", "data-source", "refresh-interval"],
            _ => vec![],
        };

        common_priority.extend(tag_specific.iter().map(|s| s.to_string()));
        common_priority
    }

    /// Check if an attribute is interesting for display
    pub(super) fn is_interesting_attribute(name: &str) -> bool {
        name.starts_with("data-")
            || name.starts_with("aria-")
            || name.starts_with("on")
            || matches!(
                name,
                "title"
                    | "alt"
                    | "placeholder"
                    | "value"
                    | "href"
                    | "src"
                    | "target"
                    | "rel"
                    | "multiple"
                    | "required"
                    | "disabled"
                    | "readonly"
                    | "checked"
                    | "selected"
                    | "autocomplete"
                    | "datetime"
                    | "pattern"
                    | "maxlength"
                    | "minlength"
                    | "rows"
                    | "cols"
                    | "accept"
                    | "open"
                    | "class"
                    | "role"
                    | "novalidate"
                    | "slot"
                    | "controls"
            )
    }

    /// Check if text content should be included in signature
    fn should_include_text_content(tag_name: &str) -> bool {
        matches!(
            tag_name,
            "title"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "p"
                | "span"
                | "a"
                | "button"
                | "label"
                | "option"
                | "th"
                | "td"
                | "dt"
                | "dd"
                | "figcaption"
                | "summary"
                | "script"
                | "style"
        )
    }

    /// Parse attributes from raw attribute text (for regex fallback)
    pub(super) fn parse_attributes_from_text(attributes_text: &str) -> HashMap<String, String> {
        let mut attributes = HashMap::new();

        // Clean up the text
        let clean_text = attributes_text.trim();
        if clean_text.is_empty() {
            return attributes;
        }

        // Enhanced attribute parsing
        let re =
            Regex::new(r#"(\w+(?:-\w+)*)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?"#).unwrap();

        for captures in re.captures_iter(clean_text) {
            if let Some(name_match) = captures.get(1) {
                let name = name_match.as_str().to_string();
                let value = captures
                    .get(2)
                    .or_else(|| captures.get(3))
                    .or_else(|| captures.get(4))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                attributes.insert(name, value);
            }
        }

        attributes
    }
}
