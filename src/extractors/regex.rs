use crate::extractors::base::{
    BaseExtractor, Relationship, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct RegexExtractor {
    base: BaseExtractor,
}

impl RegexExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    /// Helper function to create metadata with JSON values
    fn create_metadata(&self, pairs: &[(&str, &str)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
            .collect()
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);

        // Also extract patterns from text content directly
        self.extract_patterns_from_text(&self.base.content.clone(), &mut symbols);

        symbols
    }

    fn visit_node(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) -> Option<String> {
        let symbol = match node.kind() {
            "pattern" | "regex" | "expression" => self.extract_pattern(node, parent_id.clone()),
            "character_class" => self.extract_character_class(node, parent_id.clone()),
            "group" | "capturing_group" | "non_capturing_group" | "named_capturing_group" => {
                self.extract_group(node, parent_id.clone())
            }
            "quantifier" | "quantified_expression" => {
                self.extract_quantifier(node, parent_id.clone())
            }
            "anchor" | "start_assertion" | "end_assertion" | "word_boundary_assertion" => {
                self.extract_anchor(node, parent_id.clone())
            }
            "lookahead_assertion"
            | "lookbehind_assertion"
            | "positive_lookahead"
            | "negative_lookahead"
            | "positive_lookbehind"
            | "negative_lookbehind" => self.extract_lookaround(node, parent_id.clone()),
            "alternation" | "disjunction" => self.extract_alternation(node, parent_id.clone()),
            "character_escape" | "predefined_character_class" => {
                self.extract_predefined_class(node, parent_id.clone())
            }
            "unicode_property" | "unicode_category" => {
                self.extract_unicode_property(node, parent_id.clone())
            }
            "backreference" => self.extract_backreference(node, parent_id.clone()),
            "conditional" => self.extract_conditional(node, parent_id.clone()),
            "atomic_group" => self.extract_atomic_group(node, parent_id.clone()),
            "comment" => self.extract_comment(node, parent_id.clone()),
            "literal" | "character" => self.extract_literal(node, parent_id.clone()),
            _ => {
                if self.is_regex_pattern(&node) {
                    self.extract_generic_pattern(node, parent_id.clone())
                } else {
                    None
                }
            }
        };

        let current_parent_id = if let Some(symbol) = symbol {
            symbols.push(symbol.clone());
            Some(symbol.id)
        } else {
            parent_id
        };

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }

        current_parent_id
    }

    fn extract_pattern(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let pattern_text = self.base.get_node_text(&node);
        let signature = self.build_pattern_signature(&pattern_text);
        let symbol_kind = self.determine_pattern_kind(&pattern_text);

        let metadata = self.create_metadata(&[
            ("type", "regex-pattern"),
            ("pattern", &pattern_text),
            (
                "complexity",
                &self.calculate_complexity(&pattern_text).to_string(),
            ),
        ]);

        Some(self.base.create_symbol(
            &node,
            pattern_text,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_character_class(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let class_text = self.base.get_node_text(&node);
        let signature = self.build_character_class_signature(&class_text);

        let metadata = self.create_metadata(&[
            ("type", "character-class"),
            ("pattern", &class_text),
            ("negated", &class_text.starts_with("[^").to_string()),
        ]);

        Some(self.base.create_symbol(
            &node,
            class_text,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_group(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let group_text = self.base.get_node_text(&node);
        let signature = self.build_group_signature(&group_text);

        let mut metadata = self.create_metadata(&[
            ("type", "group"),
            ("pattern", &group_text),
            (
                "capturing",
                &self.is_capturing_group(&group_text).to_string(),
            ),
        ]);

        if let Some(name) = self.extract_group_name(&group_text) {
            metadata.insert("named".to_string(), Value::String(name));
        }

        Some(self.base.create_symbol(
            &node,
            group_text,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_quantifier(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let quantifier_text = self.base.get_node_text(&node);
        let signature = self.build_quantifier_signature(&quantifier_text);

        let metadata = self.create_metadata(&[
            ("type", "quantifier"),
            ("pattern", &quantifier_text),
            ("lazy", &quantifier_text.contains('?').to_string()),
            ("possessive", &quantifier_text.contains('+').to_string()),
        ]);

        Some(self.base.create_symbol(
            &node,
            quantifier_text,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_anchor(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let anchor_text = self.base.get_node_text(&node);
        let signature = self.build_anchor_signature(&anchor_text);

        let metadata = self.create_metadata(&[
            ("type", "anchor"),
            ("pattern", &anchor_text),
            ("position", &self.get_anchor_type(&anchor_text)),
        ]);

        Some(self.base.create_symbol(
            &node,
            anchor_text,
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_lookaround(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let lookaround_text = self.base.get_node_text(&node);
        let signature = self.build_lookaround_signature(&lookaround_text);

        let metadata = self.create_metadata(&[
            ("type", "lookaround"),
            ("pattern", &lookaround_text),
            (
                "direction",
                &self.get_lookaround_direction(&lookaround_text),
            ),
            (
                "positive",
                &self.is_positive_lookaround(&lookaround_text).to_string(),
            ),
        ]);

        Some(self.base.create_symbol(
            &node,
            lookaround_text,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_alternation(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let alternation_text = self.base.get_node_text(&node);
        let signature = self.build_alternation_signature(&alternation_text);

        let metadata = self.create_metadata(&[
            ("type", "alternation"),
            ("pattern", &alternation_text),
            (
                "options",
                &self
                    .extract_alternation_options(&alternation_text)
                    .join(","),
            ),
        ]);

        Some(self.base.create_symbol(
            &node,
            alternation_text,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_predefined_class(
        &mut self,
        node: Node,
        parent_id: Option<String>,
    ) -> Option<Symbol> {
        let class_text = self.base.get_node_text(&node);
        let signature = self.build_predefined_class_signature(&class_text);

        let metadata = self.create_metadata(&[
            ("type", "predefined-class"),
            ("pattern", &class_text),
            ("category", &self.get_predefined_class_category(&class_text)),
        ]);

        Some(self.base.create_symbol(
            &node,
            class_text,
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_unicode_property(
        &mut self,
        node: Node,
        parent_id: Option<String>,
    ) -> Option<Symbol> {
        let property_text = self.base.get_node_text(&node);
        let signature = self.build_unicode_property_signature(&property_text);

        let metadata = self.create_metadata(&[
            ("type", "unicode-property"),
            ("pattern", &property_text),
            (
                "property",
                &self.extract_unicode_property_name(&property_text),
            ),
        ]);

        Some(self.base.create_symbol(
            &node,
            property_text,
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_backreference(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let backref_text = self.base.get_node_text(&node);
        let signature = self.build_backreference_signature(&backref_text);

        let mut metadata =
            self.create_metadata(&[("type", "backreference"), ("pattern", &backref_text)]);

        if let Some(group_number) = self.extract_group_number(&backref_text) {
            metadata.insert("groupNumber".to_string(), Value::String(group_number));
        }

        if let Some(group_name) = self.extract_backref_group_name(&backref_text) {
            metadata.insert("groupName".to_string(), Value::String(group_name));
        }

        Some(self.base.create_symbol(
            &node,
            backref_text,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_conditional(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let conditional_text = self.base.get_node_text(&node);
        let signature = self.build_conditional_signature(&conditional_text);

        let metadata = self.create_metadata(&[
            ("type", "conditional"),
            ("pattern", &conditional_text),
            ("condition", &self.extract_condition(&conditional_text)),
        ]);

        Some(self.base.create_symbol(
            &node,
            conditional_text,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_atomic_group(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let atomic_text = self.base.get_node_text(&node);
        let signature = self.build_atomic_group_signature(&atomic_text);

        let metadata = self.create_metadata(&[
            ("type", "atomic-group"),
            ("pattern", &atomic_text),
            ("possessive", "true"),
        ]);

        Some(self.base.create_symbol(
            &node,
            atomic_text,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_comment(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let comment_text = self.base.get_node_text(&node);
        let clean_comment = comment_text
            .strip_prefix("(?#")
            .or_else(|| comment_text.strip_prefix("#"))
            .unwrap_or(&comment_text)
            .strip_suffix(")")
            .unwrap_or(&comment_text)
            .trim()
            .to_string();

        let metadata = self.create_metadata(&[("type", "comment"), ("content", &clean_comment)]);

        Some(self.base.create_symbol(
            &node,
            comment_text.clone(),
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(comment_text),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_literal(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let literal_text = self.base.get_node_text(&node);
        let signature = self.build_literal_signature(&literal_text);

        let metadata = self.create_metadata(&[
            ("type", "literal"),
            ("pattern", &literal_text),
            (
                "escaped",
                &self.is_escaped_literal(&literal_text).to_string(),
            ),
        ]);

        Some(self.base.create_symbol(
            &node,
            literal_text,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_generic_pattern(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let pattern_text = self.base.get_node_text(&node);
        let signature = self.build_generic_signature(&pattern_text);
        let symbol_kind = self.determine_pattern_kind(&pattern_text);

        let metadata = self.create_metadata(&[
            ("type", "generic-pattern"),
            ("pattern", &pattern_text),
            ("nodeType", node.kind()),
        ]);

        Some(self.base.create_symbol(
            &node,
            pattern_text,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_patterns_from_text(&mut self, text: &str, symbols: &mut Vec<Symbol>) {
        let lines: Vec<&str> = text.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
                continue;
            }

            // Clean the line - remove comments and extra whitespace
            let clean_line = self.clean_regex_line(line);
            if clean_line.is_empty() {
                continue;
            }

            // Extract meaningful regex patterns
            if self.is_valid_regex_pattern(&clean_line) {
                let symbol_kind = self.determine_pattern_kind(&clean_line);
                let signature = self.build_pattern_signature(&clean_line);

                let metadata = self.create_metadata(&[
                    ("type", "text-pattern"),
                    ("pattern", &clean_line),
                    ("lineNumber", &(i + 1).to_string()),
                    (
                        "complexity",
                        &self.calculate_complexity(&clean_line).to_string(),
                    ),
                ]);

                // Create a symbol using the standard method
                // For text-based patterns without tree-sitter nodes, we can create a simple Symbol directly
                let id = self.base.generate_id(&clean_line, (i + 1) as u32, 0);
                let symbol = Symbol {
                    id: id.clone(),
                    name: clean_line.clone(),
                    kind: symbol_kind,
                    language: self.base.language.clone(),
                    file_path: self.base.file_path.clone(),
                    start_line: (i + 1) as u32,
                    start_column: 0,
                    end_line: (i + 1) as u32,
                    end_column: clean_line.len() as u32,
                    start_byte: 0,
                    end_byte: clean_line.len() as u32,
                    signature: Some(signature),
                    doc_comment: None,
                    visibility: Some(Visibility::Public),
                    parent_id: None,
                    metadata: Some(metadata),
                    semantic_group: None, // Regex patterns don't have cross-language groups
                    confidence: None,     // Will be set during validation
                    code_context: None,   // Will be populated during context extraction
                };
                symbols.push(symbol);
            }
        }
    }

    fn is_regex_pattern(&self, node: &Node) -> bool {
        matches!(
            node.kind(),
            "pattern"
                | "regex"
                | "expression"
                | "character_class"
                | "group"
                | "quantifier"
                | "anchor"
                | "lookahead"
                | "lookbehind"
                | "alternation"
                | "character_escape"
                | "unicode_property"
                | "backreference"
                | "conditional"
        )
    }

    fn clean_regex_line(&self, line: &str) -> String {
        // Remove inline comments (// or #)
        let cleaned = if let Some(pos) = line.find("//") {
            &line[..pos]
        } else if let Some(pos) = line.find('#') {
            &line[..pos]
        } else {
            line
        };

        // Remove excessive whitespace
        cleaned.trim().to_string()
    }

    fn is_valid_regex_pattern(&self, text: &str) -> bool {
        // Skip very short patterns or obvious non-regex content
        if text.is_empty() {
            return false;
        }

        // Allow simple literals (letters, numbers, basic words)
        if text.chars().all(|c| c.is_alphanumeric()) {
            return true;
        }

        // Allow single character regex metacharacters
        if matches!(text, "." | "^" | "$") {
            return true;
        }

        // Allow simple groups and common patterns
        if (text.starts_with('(') && text.ends_with(')')) || text.ends_with('*') || text == "**" {
            return true;
        }

        // Check for regex-specific characters or patterns
        let regex_indicators = [
            r"[\[\](){}*+?^$|\\]", // Special regex characters
            r"\\[dwsWDSnrtfve]",   // Escape sequences
            r"\(\?\<?[!=]",        // Lookarounds
            r"\(\?\w+\)",          // Groups with modifiers
            r"\\p\{",              // Unicode properties
            r"\[\^",               // Negated character classes
            r"\{[\d,]+\}",         // Quantifiers
        ];

        regex_indicators.iter().any(|pattern| {
            // Simple pattern matching - check for common regex constructs
            match *pattern {
                r"[\[\](){}*+?^$|\\]" => text.chars().any(|c| "[](){}*+?^$|\\".contains(c)),
                r"\\[dwsWDSnrtfve]" => {
                    text.contains(r"\d")
                        || text.contains(r"\w")
                        || text.contains(r"\s")
                        || text.contains(r"\D")
                        || text.contains(r"\W")
                        || text.contains(r"\S")
                        || text.contains(r"\n")
                        || text.contains(r"\r")
                        || text.contains(r"\t")
                        || text.contains(r"\f")
                        || text.contains(r"\v")
                        || text.contains(r"\e")
                }
                r"\(\?\<?[!=]" => {
                    text.contains("(?=")
                        || text.contains("(?!")
                        || text.contains("(?<=")
                        || text.contains("(?<!")
                }
                r"\(\?\w+\)" => text.contains("(?") && text.contains(')'),
                r"\\p\{" => text.contains(r"\p{") || text.contains(r"\P{"),
                r"\[\^" => text.contains("[^"),
                r"\{[\d,]+\}" => {
                    text.contains('{')
                        && text.contains('}')
                        && text.chars().any(|c| c.is_ascii_digit())
                        && (text.contains(',')
                            || text.chars().filter(|c| c.is_ascii_digit()).count() > 0)
                }
                _ => false,
            }
        })
    }

    fn determine_pattern_kind(&self, pattern: &str) -> SymbolKind {
        // Lookarounds (check first, before groups)
        if pattern.contains("(?=")
            || pattern.contains("(?!")
            || pattern.contains("(?<=")
            || pattern.contains("(?<!")
        {
            return SymbolKind::Method;
        }

        // Character classes
        if pattern.starts_with('[') && pattern.ends_with(']') {
            return SymbolKind::Class;
        }

        // Groups (but not lookarounds)
        if pattern.starts_with('(')
            && pattern.ends_with(')')
            && !pattern.contains("(?=")
            && !pattern.contains("(?!")
            && !pattern.contains("(?<=")
            && !pattern.contains("(?<!")
        {
            return SymbolKind::Class;
        }

        // Quantifiers
        if pattern.ends_with('?')
            || pattern.ends_with('*')
            || pattern.ends_with('+')
            || (pattern.contains('{') && pattern.contains('}'))
        {
            return SymbolKind::Function;
        }

        // Anchors and predefined classes
        if matches!(pattern, "^" | "$")
            || pattern == r"\b"
            || pattern == r"\B"
            || pattern == r"\d"
            || pattern == r"\D"
            || pattern == r"\w"
            || pattern == r"\W"
            || pattern == r"\s"
            || pattern == r"\S"
            || pattern == "."
        {
            return SymbolKind::Constant;
        }

        // Unicode properties
        if pattern.contains(r"\p{") || pattern.contains(r"\P{") {
            return SymbolKind::Constant;
        }

        // Default to Variable for basic patterns
        SymbolKind::Variable
    }

    // Signature building methods
    fn build_pattern_signature(&self, pattern: &str) -> String {
        if pattern.len() <= 100 {
            pattern.to_string()
        } else {
            format!("{}...", &pattern[..97])
        }
    }

    fn build_character_class_signature(&self, class_text: &str) -> String {
        format!("Character class: {}", class_text)
    }

    fn build_group_signature(&self, group_text: &str) -> String {
        if let Some(group_name) = self.extract_group_name(group_text) {
            format!("Named group '{}': {}", group_name, group_text)
        } else {
            format!("Group: {}", group_text)
        }
    }

    fn build_quantifier_signature(&self, quantifier_text: &str) -> String {
        format!("Quantifier: {}", quantifier_text)
    }

    fn build_anchor_signature(&self, anchor_text: &str) -> String {
        let anchor_type = self.get_anchor_type(anchor_text);
        format!("Anchor ({}): {}", anchor_type, anchor_text)
    }

    fn build_lookaround_signature(&self, lookaround_text: &str) -> String {
        let direction = self.get_lookaround_direction(lookaround_text);
        let polarity = if self.is_positive_lookaround(lookaround_text) {
            "positive"
        } else {
            "negative"
        };
        format!("{} {}: {}", polarity, direction, lookaround_text)
    }

    fn build_alternation_signature(&self, alternation_text: &str) -> String {
        format!("Alternation: {}", alternation_text)
    }

    fn build_predefined_class_signature(&self, class_text: &str) -> String {
        let category = self.get_predefined_class_category(class_text);
        format!("Predefined class ({}): {}", category, class_text)
    }

    fn build_unicode_property_signature(&self, property_text: &str) -> String {
        let property = self.extract_unicode_property_name(property_text);
        format!("Unicode property ({}): {}", property, property_text)
    }

    fn build_backreference_signature(&self, backref_text: &str) -> String {
        if let Some(group_name) = self.extract_backref_group_name(backref_text) {
            format!("Named backreference to '{}': {}", group_name, backref_text)
        } else if let Some(group_number) = self.extract_group_number(backref_text) {
            format!("Backreference to group {}: {}", group_number, backref_text)
        } else {
            format!("Backreference: {}", backref_text)
        }
    }

    fn build_conditional_signature(&self, conditional_text: &str) -> String {
        let condition = self.extract_condition(conditional_text);
        format!("Conditional ({}): {}", condition, conditional_text)
    }

    fn build_atomic_group_signature(&self, atomic_text: &str) -> String {
        format!("Atomic group: {}", atomic_text)
    }

    fn build_literal_signature(&self, literal_text: &str) -> String {
        format!("Literal: {}", literal_text)
    }

    fn build_generic_signature(&self, pattern_text: &str) -> String {
        pattern_text.to_string()
    }

    // Helper methods
    fn is_capturing_group(&self, group_text: &str) -> bool {
        !group_text.starts_with("(?:")
            && !group_text.starts_with("(?<")
            && !group_text.starts_with("(?P<")
    }

    fn extract_group_name(&self, group_text: &str) -> Option<String> {
        if let Some(start) = group_text.find("(?<") {
            if let Some(end) = group_text[start + 3..].find('>') {
                return Some(group_text[start + 3..start + 3 + end].to_string());
            }
        }
        if let Some(start) = group_text.find("(?P<") {
            if let Some(end) = group_text[start + 4..].find('>') {
                return Some(group_text[start + 4..start + 4 + end].to_string());
            }
        }
        None
    }

    fn get_anchor_type(&self, anchor_text: &str) -> String {
        match anchor_text {
            "^" => "start".to_string(),
            "$" => "end".to_string(),
            r"\b" => "word-boundary".to_string(),
            r"\B" => "non-word-boundary".to_string(),
            r"\A" => "string-start".to_string(),
            r"\Z" => "string-end".to_string(),
            r"\z" => "absolute-end".to_string(),
            _ => "unknown".to_string(),
        }
    }

    fn get_lookaround_direction(&self, lookaround_text: &str) -> String {
        if lookaround_text.contains("(?<=") || lookaround_text.contains("(?<!") {
            "lookbehind".to_string()
        } else {
            "lookahead".to_string()
        }
    }

    fn is_positive_lookaround(&self, lookaround_text: &str) -> bool {
        lookaround_text.contains("(?=") || lookaround_text.contains("(?<=")
    }

    fn extract_alternation_options(&self, alternation_text: &str) -> Vec<String> {
        alternation_text
            .split('|')
            .map(|s| s.trim().to_string())
            .collect()
    }

    fn get_predefined_class_category(&self, class_text: &str) -> String {
        match class_text {
            r"\d" => "digit".to_string(),
            r"\D" => "non-digit".to_string(),
            r"\w" => "word".to_string(),
            r"\W" => "non-word".to_string(),
            r"\s" => "whitespace".to_string(),
            r"\S" => "non-whitespace".to_string(),
            "." => "any-character".to_string(),
            r"\n" => "newline".to_string(),
            r"\r" => "carriage-return".to_string(),
            r"\t" => "tab".to_string(),
            r"\v" => "vertical-tab".to_string(),
            r"\f" => "form-feed".to_string(),
            r"\a" => "bell".to_string(),
            r"\e" => "escape".to_string(),
            _ => "other".to_string(),
        }
    }

    fn extract_unicode_property_name(&self, property_text: &str) -> String {
        if let Some(start) = property_text
            .find(r"\p{")
            .or_else(|| property_text.find(r"\P{"))
        {
            if let Some(end) = property_text[start..].find('}') {
                let inner = &property_text[start + 3..start + end];
                return inner.to_string();
            }
        }
        "unknown".to_string()
    }

    fn extract_group_number(&self, backref_text: &str) -> Option<String> {
        if let Some(start) = backref_text.find('\\') {
            let rest = &backref_text[start + 1..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return Some(digits);
            }
        }
        None
    }

    fn extract_backref_group_name(&self, backref_text: &str) -> Option<String> {
        if let Some(start) = backref_text.find(r"\k<") {
            if let Some(end) = backref_text[start + 3..].find('>') {
                return Some(backref_text[start + 3..start + 3 + end].to_string());
            }
        }
        if let Some(start) = backref_text.find("(?P=") {
            if let Some(end) = backref_text[start + 4..].find(')') {
                return Some(backref_text[start + 4..start + 4 + end].to_string());
            }
        }
        None
    }

    fn extract_condition(&self, conditional_text: &str) -> String {
        if let Some(start) = conditional_text.find("(?(") {
            if let Some(end) = conditional_text[start + 3..].find(')') {
                return conditional_text[start + 3..start + 3 + end].to_string();
            }
        }
        "unknown".to_string()
    }

    fn is_escaped_literal(&self, literal_text: &str) -> bool {
        literal_text.starts_with('\\')
    }

    fn calculate_complexity(&self, pattern: &str) -> u32 {
        let mut complexity = 0;

        // Basic complexity indicators
        complexity += pattern.matches(['*', '+', '?']).count() as u32; // Quantifiers
        complexity += pattern.matches(['[', ']', '(', ')', '{', '}']).count() as u32; // Grouping constructs
        complexity += pattern.matches("(?").count() as u32 * 2; // Lookarounds
        complexity += pattern.matches(r"\p{").count() as u32; // Unicode properties
        complexity += pattern.matches('|').count() as u32; // Alternations

        complexity
    }

    pub fn extract_relationships(
        &mut self,
        _tree: &Tree,
        _symbols: &[Symbol],
    ) -> Vec<Relationship> {
        // For now, return empty relationships
        // In a full implementation, this would extract relationships between
        // backreferences and their corresponding groups, etc.
        Vec::new()
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            if let Some(symbol_type) = symbol.metadata.as_ref().and_then(|m| m.get("type")) {
                if let Some(type_str) = symbol_type.as_str() {
                    types.insert(symbol.id.clone(), format!("regex:{}", type_str));
                }
            } else if symbol.kind == SymbolKind::Variable {
                types.insert(symbol.id.clone(), "regex:pattern".to_string());
            }
        }
        types
    }
}
