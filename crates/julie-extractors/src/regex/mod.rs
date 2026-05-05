pub(crate) mod classes;
pub(crate) mod flags;
pub(crate) mod groups;
pub(crate) mod helpers;
pub(crate) mod identifiers;
mod patterns;
mod relationships;
pub(crate) mod signatures;

use crate::base::{BaseExtractor, Identifier, Relationship, Symbol, SymbolKind};
use std::collections::{HashMap, HashSet};
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
        let referenced_capture_numbers =
            relationships::referenced_capture_numbers(&self.base, tree);
        let mut capture_index = 0;
        self.visit_node(
            tree.root_node(),
            &mut symbols,
            None,
            &referenced_capture_numbers,
            &mut capture_index,
        );
        symbols
    }

    fn visit_node(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
        referenced_capture_numbers: &HashSet<usize>,
        capture_index: &mut usize,
    ) -> Option<String> {
        let symbol = match node.kind() {
            // Top-level patterns: only extract if no parent (root-level)
            "pattern" | "regex" | "expression" => {
                if parent_id.is_none() {
                    patterns::extract_pattern(&mut self.base, node, parent_id.clone())
                } else {
                    None // Skip child patterns inside groups
                }
            }
            // Character classes: meaningful, always keep
            "character_class" => {
                patterns::extract_character_class(&mut self.base, node, parent_id.clone())
            }
            // Groups: only keep named capturing groups
            "named_capturing_group" => {
                *capture_index += 1;
                patterns::extract_group(&mut self.base, node, parent_id.clone()).map(
                    |mut symbol| {
                        add_capture_index(&mut symbol, *capture_index);
                        symbol
                    },
                )
            }
            // Keep anonymous capture groups only when numeric backrefs make them reference targets
            "anonymous_capturing_group" | "capturing_group" => {
                *capture_index += 1;
                if referenced_capture_numbers.contains(capture_index) {
                    patterns::extract_group(&mut self.base, node, parent_id.clone()).map(
                        |mut symbol| {
                            add_capture_index(&mut symbol, *capture_index);
                            symbol
                        },
                    )
                } else {
                    None
                }
            }
            // Skip unnamed/non-capturing groups (noise)
            "group" | "non_capturing_group" => None,
            // Skip quantifiers (noise)
            "quantifier" | "quantified_expression" => None,
            // Skip anchors (noise)
            "anchor" | "start_assertion" | "end_assertion" | "word_boundary_assertion" => None,
            // Lookarounds: semantically meaningful, keep
            "lookahead_assertion"
            | "lookbehind_assertion"
            | "positive_lookahead"
            | "negative_lookahead"
            | "positive_lookbehind"
            | "negative_lookbehind" => {
                patterns::extract_lookaround(&mut self.base, node, parent_id.clone())
            }
            // Skip alternation nodes (noise)
            "alternation" | "disjunction" => None,
            // Skip predefined character classes (noise - \d, \w, \s)
            "character_escape" | "predefined_character_class" => None,
            // Unicode properties: semantically meaningful, keep
            "unicode_property" | "unicode_category" => {
                patterns::extract_unicode_property(&mut self.base, node, parent_id.clone())
            }
            // Skip backreferences (noise - references, not definitions)
            "backreference" => None,
            // Conditionals: semantically meaningful, keep
            "conditional" => patterns::extract_conditional(&mut self.base, node, parent_id.clone()),
            // Skip individual literals/characters (noise)
            "literal" | "character" => None,
            _ => None,
        };

        let current_parent_id = if let Some(symbol) = symbol {
            let id = symbol.id.clone();
            symbols.push(symbol);
            Some(id)
        } else {
            // When a node is skipped (noise), its children inherit the grandparent's
            // parent_id. This "skip-through parenting" flattens the tree — e.g. a
            // character_class inside an unnamed group gets parented to the top-level
            // pattern, not the skipped group. This is the desired behavior.
            parent_id
        };

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(
                child,
                symbols,
                current_parent_id.clone(),
                referenced_capture_numbers,
                capture_index,
            );
        }

        current_parent_id
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(&self.base, tree, symbols)
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

fn add_capture_index(symbol: &mut Symbol, capture_index: usize) {
    let metadata = symbol.metadata.get_or_insert_with(HashMap::new);
    metadata.insert(
        "captureIndex".to_string(),
        serde_json::Value::Number((capture_index as u64).into()),
    );
}
