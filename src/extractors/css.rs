// CSS Extractor - Port of Miller's css-extractor.ts
//
// Extracts CSS symbols including:
// - Selectors (element, class, ID, attribute, pseudo)
// - At-rules (@media, @keyframes, @import, @supports, etc.)
// - Custom properties (CSS variables)
// - Modern CSS features (Grid, Flexbox, Container Queries)

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Tree;
use std::collections::HashMap;

pub struct CSSExtractor {
    base: BaseExtractor,
}

impl CSSExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Main tree traversal - port of Miller's visitNode function
    fn visit_node(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        let mut current_parent_id = parent_id;

        match node.kind() {
            "rule_set" => {
                if let Some(rule_symbol) = self.extract_rule(node, current_parent_id.as_deref()) {
                    current_parent_id = Some(rule_symbol.id.clone());
                    symbols.push(rule_symbol);
                }
            },
            "at_rule" | "import_statement" | "charset_statement" | "namespace_statement" => {
                if let Some(at_rule_symbol) = self.extract_at_rule(node, current_parent_id.as_deref()) {
                    current_parent_id = Some(at_rule_symbol.id.clone());
                    symbols.push(at_rule_symbol);
                }
            },
            "keyframes_statement" => {
                if let Some(keyframes_symbol) = self.extract_keyframes_rule(node, current_parent_id.as_deref()) {
                    current_parent_id = Some(keyframes_symbol.id.clone());
                    symbols.push(keyframes_symbol);
                }
                // Also extract individual keyframes
                self.extract_keyframes(node, symbols, current_parent_id.as_deref());
            },
            "keyframe_block_list" => {
                // Handle keyframes content
                self.extract_keyframes(node, symbols, current_parent_id.as_deref());
            },
            "media_statement" => {
                if let Some(media_symbol) = self.extract_media_rule(node, current_parent_id.as_deref()) {
                    current_parent_id = Some(media_symbol.id.clone());
                    symbols.push(media_symbol);
                }
            },
            "supports_statement" => {
                if let Some(supports_symbol) = self.extract_supports_rule(node, current_parent_id.as_deref()) {
                    current_parent_id = Some(supports_symbol.id.clone());
                    symbols.push(supports_symbol);
                }
            },
            "property_name" => {
                // CSS custom properties (variables)
                let property_text = self.base.get_node_text(&node);
                if property_text.starts_with("--") {
                    if let Some(custom_prop) = self.extract_custom_property(node, current_parent_id.as_deref()) {
                        symbols.push(custom_prop);
                    }
                }
            },
            _ => {}
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract CSS rule - port of Miller's extractRule
    fn extract_rule(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Find selectors and declaration block
        let mut selectors_node = None;
        let mut declaration_block = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "selectors" => selectors_node = Some(child),
                "block" => declaration_block = Some(child),
                _ => {}
            }
        }

        let selector_text = if let Some(selectors) = selectors_node {
            self.base.get_node_text(&selectors)
        } else {
            "unknown".to_string()
        };

        let signature = self.build_rule_signature(&node, &selector_text);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), serde_json::Value::String("css-rule".to_string()));
        metadata.insert("selector".to_string(), serde_json::Value::String(selector_text.clone()));

        let properties = self.extract_properties(declaration_block.as_ref());
        metadata.insert("properties".to_string(), serde_json::Value::Array(
            properties.into_iter().map(|p| serde_json::Value::String(p)).collect()
        ));

        Some(self.base.create_symbol(
            &node,
            selector_text,
            SymbolKind::Variable, // CSS rules as variables per Miller
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    /// Extract at-rule - port of Miller's extractAtRule
    fn extract_at_rule(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        let rule_name = self.extract_at_rule_name(&node);
        let signature = self.base.get_node_text(&node);

        // Determine symbol kind based on at-rule type - match Miller's logic
        let symbol_kind = if rule_name == "@keyframes" {
            SymbolKind::Function // Animations as functions
        } else if rule_name == "@import" {
            SymbolKind::Import
        } else {
            SymbolKind::Variable
        };

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), serde_json::Value::String("at-rule".to_string()));
        metadata.insert("ruleName".to_string(), serde_json::Value::String(rule_name.clone()));
        let at_rule_type = if rule_name.starts_with('@') {
            &rule_name[1..]
        } else {
            &rule_name
        };
        metadata.insert("atRuleType".to_string(), serde_json::Value::String(at_rule_type.to_string()));

        Some(self.base.create_symbol(
            &node,
            rule_name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    /// Extract media rule - port of Miller's extractMediaRule
    fn extract_media_rule(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        let media_query = self.extract_media_query(&node);
        let signature = self.base.get_node_text(&node);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), serde_json::Value::String("media-rule".to_string()));
        metadata.insert("query".to_string(), serde_json::Value::String(media_query.clone()));

        Some(self.base.create_symbol(
            &node,
            media_query,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    /// Extract keyframes rule - port of Miller's extractKeyframesRule
    fn extract_keyframes_rule(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        let keyframes_name = self.extract_keyframes_name(&node);
        let signature = self.base.get_node_text(&node);
        let symbol_name = format!("@keyframes {}", keyframes_name);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), serde_json::Value::String("keyframes".to_string()));
        metadata.insert("animationName".to_string(), serde_json::Value::String(keyframes_name));

        Some(self.base.create_symbol(
            &node,
            symbol_name,
            SymbolKind::Function, // Animations as functions per Miller
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    /// Extract supports rule - port of Miller's extractSupportsRule
    fn extract_supports_rule(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        let condition = self.extract_supports_condition(&node);
        let signature = self.base.get_node_text(&node);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), serde_json::Value::String("supports-rule".to_string()));
        metadata.insert("condition".to_string(), serde_json::Value::String(condition.clone()));

        Some(self.base.create_symbol(
            &node,
            condition,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    /// Extract individual keyframes - port of Miller's extractKeyframes
    fn extract_keyframes(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "keyframe_block" {
                // Find keyframe selector (from, to, or percentage)
                let mut keyframe_selector = None;
                let mut child_cursor = child.walk();
                for grandchild in child.children(&mut child_cursor) {
                    match grandchild.kind() {
                        "from" | "to" | "percentage" => {
                            keyframe_selector = Some(grandchild);
                            break;
                        }
                        _ => {}
                    }
                }

                if let Some(selector) = keyframe_selector {
                    let selector_text = self.base.get_node_text(&selector);
                    let signature = self.base.get_node_text(&child);

                    // Create metadata
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("keyframe".to_string()));
                    metadata.insert("selector".to_string(), serde_json::Value::String(selector_text.clone()));

                    let symbol = self.base.create_symbol(
                        &child,
                        selector_text,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|id| id.to_string()),
                            metadata: Some(metadata),
                            doc_comment: None,
                        },
                    );

                    symbols.push(symbol);
                }
            }
        }
    }

    /// Extract custom property - port of Miller's extractCustomProperty
    fn extract_custom_property(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        let property_name = self.base.get_node_text(&node);
        let value_node = self.find_property_value(&node);
        let value = if let Some(val_node) = value_node {
            self.base.get_node_text(&val_node)
        } else {
            String::new()
        };

        let signature = format!("{}: {}", property_name, value);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), serde_json::Value::String("custom-property".to_string()));
        metadata.insert("property".to_string(), serde_json::Value::String(property_name.clone()));
        metadata.insert("value".to_string(), serde_json::Value::String(value));

        Some(self.base.create_symbol(
            &node,
            property_name,
            SymbolKind::Property, // Custom properties as properties
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    /// Build rule signature - port of Miller's buildRuleSignature
    fn build_rule_signature(&self, node: &tree_sitter::Node, selector: &str) -> String {
        let declaration_block = self.find_declaration_block(node);

        if let Some(block) = declaration_block {
            let key_properties = self.extract_key_properties(&block, Some(selector));
            if !key_properties.is_empty() {
                return format!("{} {{ {} }}", selector, key_properties.join("; "));
            }
        }

        selector.to_string()
    }

    /// Find declaration block in rule
    fn find_declaration_block<'a>(&self, node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "block" {
                return Some(child);
            }
        }
        None
    }

    /// Extract properties - port of Miller's extractProperties
    fn extract_properties(&self, declaration_block: Option<&tree_sitter::Node>) -> Vec<String> {
        let mut properties = Vec::new();

        if let Some(block) = declaration_block {
            let mut cursor = block.walk();
            for child in block.children(&mut cursor) {
                if child.kind() == "declaration" {
                    let prop = self.base.get_node_text(&child);
                    properties.push(prop);
                }
            }
        }

        properties
    }

    /// Extract key properties - port of Miller's extractKeyProperties
    fn extract_key_properties(&self, declaration_block: &tree_sitter::Node, selector: Option<&str>) -> Vec<String> {
        let important_properties = [
            "display", "position", "background", "color", "font-family", "font-weight",
            "grid-template", "grid-area", "flex", "margin", "padding", "width", "height",
            "transform", "text-decoration", "box-shadow", "border", "backdrop-filter",
            "linear-gradient", "max-width", "text-align", "cursor", "opacity", "content"
        ];

        let mut all_props = Vec::new();
        let mut custom_props = Vec::new();
        let mut important_props = Vec::new();
        let mut unique_props = Vec::new();

        let mut cursor = declaration_block.walk();
        for child in declaration_block.children(&mut cursor) {
            if child.kind() == "declaration" {
                let prop_text = self.base.get_node_text(&child).trim().to_string();

                // Remove trailing semicolon
                let clean_prop = if prop_text.ends_with(';') {
                    prop_text[..prop_text.len()-1].to_string()
                } else {
                    prop_text
                };

                all_props.push(clean_prop.clone());

                // Categorize properties following Miller's logic
                if clean_prop.starts_with("--") {
                    custom_props.push(clean_prop);
                } else if important_properties.iter().any(|&prop| clean_prop.starts_with(prop)) {
                    important_props.push(clean_prop);
                } else if self.is_unique_property(&clean_prop) {
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

    /// Check if property is unique/interesting - port of Miller's isUniqueProperty
    fn is_unique_property(&self, property: &str) -> bool {
        property.contains("calc(") ||
        property.contains("var(") ||
        property.contains("attr(") ||
        property.contains("url(") ||
        property.contains("linear-gradient") ||
        property.contains("radial-gradient") ||
        property.contains("rgba(") ||
        property.contains("hsla(") ||
        property.contains("repeat(") ||
        property.contains("minmax(") ||
        property.contains("clamp(") ||
        property.contains("min(") ||
        property.contains("max(") ||
        property.starts_with("grid-") ||
        property.starts_with("flex-") ||
        property.contains("transform") ||
        property.contains("animation") ||
        property.contains("transition")
    }

    /// Extract at-rule name - port of Miller's extractAtRuleName
    fn extract_at_rule_name(&self, node: &tree_sitter::Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "at_keyword" {
                return self.base.get_node_text(&child);
            }
            let text = self.base.get_node_text(&child);
            if text.starts_with('@') {
                return text.split_whitespace().next().unwrap_or("@unknown").to_string();
            }
        }
        "@unknown".to_string()
    }

    /// Extract media query - port of Miller's extractMediaQuery
    fn extract_media_query(&self, node: &tree_sitter::Node) -> String {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Find @media keyword
        for (i, child) in children.iter().enumerate() {
            let text = self.base.get_node_text(child);
            if text == "@media" {
                let mut query_parts = Vec::new();

                // Get the query parts after @media
                for j in (i + 1)..children.len() {
                    let child = &children[j];
                    if child.kind() == "block" {
                        break; // Stop at the rule block
                    }
                    let part = self.base.get_node_text(child).trim().to_string();
                    if !part.is_empty() {
                        query_parts.push(part);
                    }
                }

                return format!("@media {}", query_parts.join(" ").trim());
            }
        }

        "@media".to_string()
    }

    /// Find property value - port of Miller's findPropertyValue
    fn find_property_value<'a>(&self, property_node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
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

    /// Extract keyframes name - port of Miller's extractKeyframesName
    fn extract_keyframes_name(&self, node: &tree_sitter::Node) -> String {
        let text = self.base.get_node_text(node);
        if let Some(captures) = regex::Regex::new(r"@keyframes\s+([^\s{]+)")
            .unwrap()
            .captures(&text) {
            captures.get(1).unwrap().as_str().to_string()
        } else {
            "unknown".to_string()
        }
    }

    /// Extract supports condition - port of Miller's extractSupportsCondition
    fn extract_supports_condition(&self, node: &tree_sitter::Node) -> String {
        let text = self.base.get_node_text(node);
        if let Some(captures) = regex::Regex::new(r"@supports\s+([^{]+)")
            .unwrap()
            .captures(&text) {
            format!("@supports {}", captures.get(1).unwrap().as_str().trim())
        } else {
            "@supports".to_string()
        }
    }
}