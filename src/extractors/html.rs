// HTML Extractor
//
// Port of Miller's HTML extractor to idiomatic Rust
// Original: /Users/murphy/Source/miller/src/extractors/html-extractor.ts

use crate::extractors::base::{
    BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use regex::Regex;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct HTMLExtractor {
    base: BaseExtractor,
}

impl HTMLExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Check if tree is valid and has a root node - start from actual root like Miller
        let root_node = tree.root_node();
        if root_node.child_count() > 0 {
            self.visit_node(root_node, &mut symbols, None);
        } else {
            // Fallback extraction when normal parsing fails
            return self.extract_basic_structure(tree);
        }

        // If we only extracted error symbols, try basic structure fallback
        let has_only_errors = !symbols.is_empty()
            && symbols.iter().all(|s| {
                s.metadata
                    .as_ref()
                    .and_then(|m| m.get("isError"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            });

        if has_only_errors || symbols.is_empty() {
            self.extract_basic_structure(tree)
        } else {
            symbols
        }
    }

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        if let Some(symbol) = self.extract_node_symbol(node, parent_id) {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);

            // Recursively visit children with the new symbol as parent
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.visit_node(child, symbols, Some(&symbol_id));
            }
        } else {
            // If no symbol was extracted, continue with children using current parent
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.visit_node(child, symbols, parent_id);
            }
        }
    }

    fn extract_node_symbol(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        match node.kind() {
            "element" => Some(self.extract_element(node, parent_id)),
            "script_element" => Some(self.extract_script_element(node, parent_id)),
            "style_element" => Some(self.extract_style_element(node, parent_id)),
            "doctype" => Some(self.extract_doctype(node, parent_id)),
            "comment" => self.extract_comment(node, parent_id),
            _ => None,
        }
    }

    fn extract_element(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let tag_name = self.extract_tag_name(node);
        let attributes = self.extract_attributes(node);
        let text_content = self.extract_element_text_content(node);
        let signature =
            self.build_element_signature(&tag_name, &attributes, text_content.as_deref());

        // Determine symbol kind based on element type
        let symbol_kind = self.get_symbol_kind_for_element(&tag_name, &attributes);

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("html-element".to_string()),
        );
        metadata.insert(
            "tagName".to_string(),
            serde_json::Value::String(tag_name.clone()),
        );
        metadata.insert(
            "isVoid".to_string(),
            serde_json::Value::Bool(self.is_void_element(&tag_name)),
        );
        metadata.insert(
            "isSemantic".to_string(),
            serde_json::Value::Bool(self.is_semantic_element(&tag_name)),
        );

        if !attributes.is_empty() {
            metadata.insert(
                "attributes".to_string(),
                serde_json::to_value(&attributes).unwrap_or_default(),
            );
        }

        if let Some(content) = text_content {
            metadata.insert(
                "textContent".to_string(),
                serde_json::Value::String(content),
            );
        }

        self.base.create_symbol(
            &node,
            tag_name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    fn extract_script_element(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let attributes = self.extract_attributes(node);
        let content = self.extract_text_content(node);
        let signature = self.build_element_signature("script", &attributes, content.as_deref());

        // Determine symbol kind based on src attribute
        let symbol_kind = if attributes.contains_key("src") {
            SymbolKind::Import
        } else {
            SymbolKind::Variable
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("script-element".to_string()),
        );
        metadata.insert(
            "isInline".to_string(),
            serde_json::Value::Bool(!attributes.contains_key("src")),
        );

        if !attributes.is_empty() {
            metadata.insert(
                "attributes".to_string(),
                serde_json::to_value(&attributes).unwrap_or_default(),
            );
        }

        let script_type = attributes
            .get("type")
            .cloned()
            .unwrap_or_else(|| "text/javascript".to_string());
        metadata.insert(
            "scriptType".to_string(),
            serde_json::Value::String(script_type),
        );

        if let Some(content) = content {
            let truncated_content = if content.len() > 100 {
                format!("{}...", &content[..100])
            } else {
                content
            };
            metadata.insert(
                "content".to_string(),
                serde_json::Value::String(truncated_content),
            );
        }

        self.base.create_symbol(
            &node,
            "script".to_string(),
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    fn extract_style_element(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let attributes = self.extract_attributes(node);
        let content = self.extract_text_content(node);
        let signature = self.build_element_signature("style", &attributes, content.as_deref());

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("style-element".to_string()),
        );
        metadata.insert("isInline".to_string(), serde_json::Value::Bool(true));

        if !attributes.is_empty() {
            metadata.insert(
                "attributes".to_string(),
                serde_json::to_value(&attributes).unwrap_or_default(),
            );
        }

        if let Some(content) = content {
            let truncated_content = if content.len() > 100 {
                format!("{}...", &content[..100])
            } else {
                content
            };
            metadata.insert(
                "content".to_string(),
                serde_json::Value::String(truncated_content),
            );
        }

        self.base.create_symbol(
            &node,
            "style".to_string(),
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    fn extract_doctype(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let doctype_text = self.base.get_node_text(&node);

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("doctype".to_string()),
        );
        metadata.insert(
            "declaration".to_string(),
            serde_json::Value::String(doctype_text.clone()),
        );

        self.base.create_symbol(
            &node,
            "DOCTYPE".to_string(),
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(doctype_text),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    fn extract_comment(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let comment_text = self.base.get_node_text(&node);
        let clean_comment = comment_text
            .replace("<!--", "")
            .replace("-->", "")
            .trim()
            .to_string();

        // Only extract meaningful comments (not empty or very short)
        if clean_comment.len() < 3 {
            return None;
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("comment".to_string()),
        );
        metadata.insert(
            "content".to_string(),
            serde_json::Value::String(clean_comment.clone()),
        );

        Some(self.base.create_symbol(
            &node,
            "comment".to_string(),
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(format!("<!-- {} -->", clean_comment)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_tag_name(&self, node: Node) -> String {
        // Look for start_tag or self_closing_tag child and extract tag name
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "start_tag" | "self_closing_tag") {
                let mut inner_cursor = child.walk();
                for inner_child in child.children(&mut inner_cursor) {
                    if inner_child.kind() == "tag_name" {
                        return self.base.get_node_text(&inner_child);
                    }
                }
            }
        }

        // Fallback: look for any tag_name child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "tag_name" {
                return self.base.get_node_text(&child);
            }
        }

        "unknown".to_string()
    }

    fn extract_attributes(&self, node: Node) -> HashMap<String, String> {
        let mut attributes = HashMap::new();

        // Find the tag container (start_tag or self_closing_tag)
        let mut cursor = node.walk();
        let tag_container = node
            .children(&mut cursor)
            .find(|c| matches!(c.kind(), "start_tag" | "self_closing_tag"))
            .unwrap_or(node);

        let mut tag_cursor = tag_container.walk();
        for child in tag_container.children(&mut tag_cursor) {
            if child.kind() == "attribute" {
                if let (Some(attr_name), attr_value) = self.extract_attribute_name_value(child) {
                    attributes.insert(attr_name, attr_value.unwrap_or_default());
                }
            }
        }

        attributes
    }

    fn extract_attribute_name_value(&self, attr_node: Node) -> (Option<String>, Option<String>) {
        let mut name = None;
        let mut value = None;

        let mut cursor = attr_node.walk();
        for child in attr_node.children(&mut cursor) {
            match child.kind() {
                "attribute_name" => {
                    name = Some(self.base.get_node_text(&child));
                }
                "attribute_value" | "quoted_attribute_value" => {
                    let text = self.base.get_node_text(&child);
                    // Remove quotes if present
                    value = Some(text.trim_matches(|c| c == '"' || c == '\'').to_string());
                }
                _ => {}
            }
        }

        (name, value)
    }

    fn extract_text_content(&self, node: Node) -> Option<String> {
        // Extract text content from script or style elements
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "text" | "raw_text") {
                let text = self.base.get_node_text(&child).trim().to_string();
                return if text.is_empty() { None } else { Some(text) };
            }
        }
        None
    }

    fn extract_element_text_content(&self, node: Node) -> Option<String> {
        // Extract text content from HTML elements
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "text" {
                let text = self.base.get_node_text(&child).trim().to_string();
                return if text.is_empty() { None } else { Some(text) };
            }
        }
        None
    }

    fn build_element_signature(
        &self,
        tag_name: &str,
        attributes: &HashMap<String, String>,
        text_content: Option<&str>,
    ) -> String {
        let mut signature = format!("<{}", tag_name);

        // Include important attributes in signature
        let important_attrs = self.get_important_attributes(tag_name, attributes);
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
            if self.should_include_text_content(tag_name) {
                let truncated_content = if content.len() > 100 {
                    format!("{}...", &content[..100])
                } else {
                    content.to_string()
                };
                signature.push_str(&truncated_content);
            }
        }

        signature
    }

    fn get_important_attributes(
        &self,
        tag_name: &str,
        attributes: &HashMap<String, String>,
    ) -> Vec<(String, String)> {
        let mut important = Vec::new();
        let priority_attrs = self.get_priority_attributes_for_tag(tag_name);

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
                && self.is_interesting_attribute(name)
                && important.len() < max_attrs
            {
                important.push((name.clone(), value.clone()));
            }
        }

        important
    }

    fn get_priority_attributes_for_tag(&self, tag_name: &str) -> Vec<String> {
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

    fn is_interesting_attribute(&self, name: &str) -> bool {
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

    fn get_symbol_kind_for_element(
        &self,
        tag_name: &str,
        attributes: &HashMap<String, String>,
    ) -> SymbolKind {
        match tag_name {
            // Meta elements are properties
            "meta" => SymbolKind::Property,

            // Link elements with stylesheet are imports
            "link"
                if attributes
                    .get("rel")
                    .map(|v| v == "stylesheet")
                    .unwrap_or(false) =>
            {
                SymbolKind::Import
            }

            // Form input elements are fields
            _ if self.is_form_field(tag_name) => SymbolKind::Field,

            // Media elements are variables
            _ if matches!(
                tag_name,
                "img" | "video" | "audio" | "picture" | "source" | "track"
            ) =>
            {
                SymbolKind::Variable
            }

            // All other HTML elements are classes
            _ => SymbolKind::Class,
        }
    }

    fn is_form_field(&self, tag_name: &str) -> bool {
        matches!(
            tag_name,
            "input" | "textarea" | "select" | "button" | "fieldset" | "legend" | "label"
        )
    }

    fn is_void_element(&self, tag_name: &str) -> bool {
        matches!(
            tag_name,
            "area"
                | "base"
                | "br"
                | "col"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        )
    }

    fn is_semantic_element(&self, tag_name: &str) -> bool {
        matches!(
            tag_name,
            "article"
                | "aside"
                | "details"
                | "figcaption"
                | "figure"
                | "footer"
                | "header"
                | "main"
                | "nav"
                | "section"
                | "summary"
                | "time"
        )
    }

    fn should_include_text_content(&self, tag_name: &str) -> bool {
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

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();

        if let Some(root_node) = tree.root_node().child(0) {
            self.visit_node_for_relationships(root_node, symbols, &mut relationships);
        }

        relationships
    }

    fn visit_node_for_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "element" => {
                self.extract_element_relationships(node, symbols, relationships);
            }
            "script_element" => {
                self.extract_script_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    fn extract_element_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let attributes = self.extract_attributes(node);

        // Extract href relationships (links)
        if let Some(href) = attributes.get("href") {
            if let Some(element) = self.find_element_symbol(node, symbols) {
                let to_id = format!("url:{}", href);
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        element.id,
                        to_id,
                        RelationshipKind::References,
                        node.start_position().row
                    ),
                    from_symbol_id: element.id.clone(),
                    to_symbol_id: to_id,
                    kind: RelationshipKind::References,
                    file_path: self.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert("href".to_string(), serde_json::Value::String(href.clone()));
                        meta
                    }),
                });
            }
        }

        // Extract src relationships (images, scripts, etc.)
        if let Some(src) = attributes.get("src") {
            if let Some(element) = self.find_element_symbol(node, symbols) {
                let to_id = format!("resource:{}", src);
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        element.id,
                        to_id,
                        RelationshipKind::Uses,
                        node.start_position().row
                    ),
                    from_symbol_id: element.id.clone(),
                    to_symbol_id: to_id,
                    kind: RelationshipKind::Uses,
                    file_path: self.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert("src".to_string(), serde_json::Value::String(src.clone()));
                        meta
                    }),
                });
            }
        }

        // Extract form relationships
        if let Some(action) = attributes.get("action") {
            if let Some(element) = self.find_element_symbol(node, symbols) {
                let to_id = format!("endpoint:{}", action);
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        element.id,
                        to_id,
                        RelationshipKind::Calls,
                        node.start_position().row
                    ),
                    from_symbol_id: element.id.clone(),
                    to_symbol_id: to_id,
                    kind: RelationshipKind::Calls,
                    file_path: self.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert(
                            "action".to_string(),
                            serde_json::Value::String(action.clone()),
                        );
                        meta.insert(
                            "method".to_string(),
                            serde_json::Value::String(
                                attributes
                                    .get("method")
                                    .cloned()
                                    .unwrap_or_else(|| "GET".to_string()),
                            ),
                        );
                        meta
                    }),
                });
            }
        }
    }

    fn extract_script_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let attributes = self.extract_attributes(node);

        if let Some(src) = attributes.get("src") {
            if let Some(script_symbol) = symbols.iter().find(|s| {
                s.metadata
                    .as_ref()
                    .and_then(|m| m.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|t| t == "script-element")
                    .unwrap_or(false)
            }) {
                let to_id = format!("script:{}", src);
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        script_symbol.id,
                        to_id,
                        RelationshipKind::Imports,
                        node.start_position().row
                    ),
                    from_symbol_id: script_symbol.id.clone(),
                    to_symbol_id: to_id,
                    kind: RelationshipKind::Imports,
                    file_path: self.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert("src".to_string(), serde_json::Value::String(src.clone()));
                        if let Some(script_type) = attributes.get("type") {
                            meta.insert(
                                "type".to_string(),
                                serde_json::Value::String(script_type.clone()),
                            );
                        }
                        meta
                    }),
                });
            }
        }
    }

    fn find_element_symbol<'a>(&self, node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
        let tag_name = self.extract_tag_name(node);
        let target_line = (node.start_position().row + 1) as u32;

        symbols.iter().find(|s| {
            s.name == tag_name
                && s.file_path == self.base.file_path
                && s.start_line.abs_diff(target_line) < 2
        })
    }

    fn extract_basic_structure(&mut self, tree: &Tree) -> Vec<Symbol> {
        // Fallback extraction when normal parsing fails
        let mut symbols = Vec::new();
        let content = self.base.get_node_text(&tree.root_node());

        // Extract DOCTYPE if present
        if let Some(doctype_match) = self.find_doctype(&content) {
            let mut metadata = HashMap::new();
            metadata.insert(
                "type".to_string(),
                serde_json::Value::String("doctype".to_string()),
            );
            metadata.insert(
                "declaration".to_string(),
                serde_json::Value::String(doctype_match.clone()),
            );

            let symbol = self.base.create_symbol(
                &tree.root_node(),
                "DOCTYPE".to_string(),
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(doctype_match.clone()),
                    visibility: Some(Visibility::Public),
                    parent_id: None,
                    metadata: Some(metadata),
                    doc_comment: None,
                },
            );
            symbols.push(symbol);
        }

        // Extract elements using regex as fallback
        symbols.extend(self.extract_elements_with_regex(&content, &tree.root_node()));

        symbols
    }

    fn find_doctype(&self, content: &str) -> Option<String> {
        // Simple regex to find DOCTYPE declaration
        if let Some(start) = content.find("<!DOCTYPE") {
            if let Some(end) = content[start..].find('>') {
                return Some(content[start..start + end + 1].to_string());
            }
        }
        None
    }

    fn extract_elements_with_regex(&mut self, content: &str, root_node: &Node) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Enhanced regex for HTML elements - handles both self-closing and container elements
        // Note: Rust regex doesn't support backreferences, so we match any closing tag
        let re = Regex::new(r#"<([a-zA-Z][a-zA-Z0-9\-]*)(?:\s+([^>]*?))?\s*(?:/>|>(.*?)</[^>]+>|>)"#)
            .unwrap();

        for captures in re.captures_iter(content) {
            if let Some(tag_name_match) = captures.get(1) {
                let tag_name = tag_name_match.as_str().to_string();
                let attributes_text = captures.get(2).map(|m| m.as_str()).unwrap_or("");
                let text_content = captures.get(3).map(|m| m.as_str());

                // Parse attributes
                let attributes = self.parse_attributes_from_text(attributes_text);

                // Build signature
                let signature = self.build_element_signature(&tag_name, &attributes, text_content);

                // Determine symbol kind
                let symbol_kind = self.get_symbol_kind_for_element(&tag_name, &attributes);

                // Create metadata
                let mut metadata = HashMap::new();
                metadata.insert(
                    "type".to_string(),
                    serde_json::Value::String("html-element-fallback".to_string()),
                );
                metadata.insert(
                    "tagName".to_string(),
                    serde_json::Value::String(tag_name.clone()),
                );
                metadata.insert("isFallback".to_string(), serde_json::Value::Bool(true));

                if !attributes.is_empty() {
                    metadata.insert(
                        "attributes".to_string(),
                        serde_json::to_value(&attributes).unwrap_or_default(),
                    );
                }

                if let Some(content) = text_content {
                    if !content.trim().is_empty() {
                        metadata.insert(
                            "textContent".to_string(),
                            serde_json::Value::String(content.trim().to_string()),
                        );
                    }
                }

                let symbol = self.base.create_symbol(
                    root_node,
                    tag_name,
                    symbol_kind,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: None,
                        metadata: Some(metadata),
                        doc_comment: None,
                    },
                );
                symbols.push(symbol);
            }
        }

        symbols
    }

    fn parse_attributes_from_text(&self, attributes_text: &str) -> HashMap<String, String> {
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

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            let metadata = &symbol.metadata;
            if let Some(symbol_type) = metadata
                .as_ref()
                .and_then(|m| m.get("type"))
                .and_then(|v| v.as_str())
            {
                types.insert(symbol.id.clone(), symbol_type.to_string());
            } else if let Some(tag_name) = metadata
                .as_ref()
                .and_then(|m| m.get("tagName"))
                .and_then(|v| v.as_str())
            {
                types.insert(symbol.id.clone(), format!("html:{}", tag_name));
            }
        }

        types
    }
}
