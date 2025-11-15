pub(crate) mod classes;
pub(crate) mod flags;
pub(crate) mod groups;
pub(crate) mod helpers;
pub(crate) mod identifiers;
mod patterns;
pub(crate) mod signatures;

use crate::extractors::base::{
    BaseExtractor, Identifier, Relationship, Symbol, SymbolKind, Visibility,
};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct RegexExtractor {
    pub(crate) base: BaseExtractor,
}

impl RegexExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
        }
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
            "pattern" | "regex" | "expression" => {
                patterns::extract_pattern(&mut self.base, node, parent_id.clone())
            }
            "character_class" => {
                patterns::extract_character_class(&mut self.base, node, parent_id.clone())
            }
            "group" | "capturing_group" | "non_capturing_group" | "named_capturing_group" => {
                patterns::extract_group(&mut self.base, node, parent_id.clone())
            }
            "quantifier" | "quantified_expression" => {
                patterns::extract_quantifier(&mut self.base, node, parent_id.clone())
            }
            "anchor" | "start_assertion" | "end_assertion" | "word_boundary_assertion" => {
                patterns::extract_anchor(&mut self.base, node, parent_id.clone())
            }
            "lookahead_assertion"
            | "lookbehind_assertion"
            | "positive_lookahead"
            | "negative_lookahead"
            | "positive_lookbehind"
            | "negative_lookbehind" => {
                patterns::extract_lookaround(&mut self.base, node, parent_id.clone())
            }
            "alternation" | "disjunction" => {
                patterns::extract_alternation(&mut self.base, node, parent_id.clone())
            }
            "character_escape" | "predefined_character_class" => {
                patterns::extract_predefined_class(&mut self.base, node, parent_id.clone())
            }
            "unicode_property" | "unicode_category" => {
                patterns::extract_unicode_property(&mut self.base, node, parent_id.clone())
            }
            "backreference" => {
                patterns::extract_backreference(&mut self.base, node, parent_id.clone())
            }
            "conditional" => patterns::extract_conditional(&mut self.base, node, parent_id.clone()),
            // NOTE: "atomic_group" and "comment" branches removed (2025-10-31)
            // Tree-sitter regex parser doesn't generate these node types:
            // - Atomic groups (?>...) are parsed as ERROR nodes
            // - Inline comments (?#...) are parsed as ERROR nodes
            // Functions extract_atomic_group() and extract_comment() were unreachable dead code
            "literal" | "character" => {
                patterns::extract_literal(&mut self.base, node, parent_id.clone())
            }
            _ => {
                if helpers::is_regex_pattern(node.kind()) {
                    patterns::extract_generic_pattern(&mut self.base, node, parent_id.clone())
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

    fn extract_patterns_from_text(&mut self, text: &str, symbols: &mut Vec<Symbol>) {
        let lines: Vec<&str> = text.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
                continue;
            }

            // Clean the line - remove comments and extra whitespace
            let clean_line = helpers::clean_regex_line(line);
            if clean_line.is_empty() {
                continue;
            }

            // Extract meaningful regex patterns
            if helpers::is_valid_regex_pattern(&clean_line) {
                let symbol_kind = helpers::determine_pattern_kind(&clean_line);
                let signature = signatures::build_pattern_signature(&clean_line);

                let metadata = patterns::create_metadata(&[
                    ("type", "text-pattern"),
                    ("pattern", &clean_line),
                    ("lineNumber", &(i + 1).to_string()),
                    (
                        "complexity",
                        &helpers::calculate_complexity(&clean_line).to_string(),
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
                    content_type: None,
                };
                symbols.push(symbol);
            }
        }
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

    /// Extract all identifier usages (backreferences and named groups)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }
}
