// CSS Extractor - Implementation of css-extractor.ts
//
// Extracts CSS symbols including:
// - Selectors (element, class, ID, attribute, pseudo)
// - At-rules (@media, @keyframes, @import, @supports, etc.)
// - Custom properties (CSS variables)
// - Modern CSS features (Grid, Flexbox, Container Queries)

mod animations;
mod at_rules;
mod helpers;
mod identifiers;
mod media;
mod properties;
mod rules;

use crate::extractors::base::{BaseExtractor, Identifier, Symbol};
use animations::AnimationExtractor;
use at_rules::AtRuleExtractor;
use identifiers::IdentifierExtractor;
use media::MediaExtractor;
use properties::PropertyExtractor;
use rules::RuleExtractor;
use tree_sitter::Tree;

pub struct CSSExtractor {
    base: BaseExtractor,
}

impl CSSExtractor {
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
        symbols
    }

    /// Main tree traversal - Implementation of visitNode function
    fn visit_node(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let mut current_parent_id = parent_id;

        match node.kind() {
            "rule_set" => {
                if let Some(rule_symbol) =
                    RuleExtractor::extract_rule(&mut self.base, node, current_parent_id.as_deref())
                {
                    current_parent_id = Some(rule_symbol.id.clone());
                    symbols.push(rule_symbol);
                }
            }
            "at_rule" | "import_statement" | "charset_statement" | "namespace_statement" => {
                if let Some(at_rule_symbol) = AtRuleExtractor::extract_at_rule(
                    &mut self.base,
                    node,
                    current_parent_id.as_deref(),
                ) {
                    current_parent_id = Some(at_rule_symbol.id.clone());
                    symbols.push(at_rule_symbol);
                }
            }
            "keyframes_statement" => {
                if let Some(keyframes_symbol) = AnimationExtractor::extract_keyframes_rule(
                    &mut self.base,
                    node,
                    current_parent_id.as_deref(),
                ) {
                    current_parent_id = Some(keyframes_symbol.id.clone());
                    symbols.push(keyframes_symbol);
                }
                // Also extract the animation name as a separate symbol
                if let Some(animation_symbol) = AnimationExtractor::extract_animation_name(
                    &mut self.base,
                    node,
                    current_parent_id.as_deref(),
                ) {
                    symbols.push(animation_symbol);
                }
                // Also extract individual keyframes
                AnimationExtractor::extract_keyframes(
                    &mut self.base,
                    node,
                    symbols,
                    current_parent_id.as_deref(),
                );
            }
            "keyframe_block_list" => {
                // Handle keyframes content
                AnimationExtractor::extract_keyframes(
                    &mut self.base,
                    node,
                    symbols,
                    current_parent_id.as_deref(),
                );
            }
            "media_statement" => {
                if let Some(media_symbol) = MediaExtractor::extract_media_rule(
                    &mut self.base,
                    node,
                    current_parent_id.as_deref(),
                ) {
                    current_parent_id = Some(media_symbol.id.clone());
                    symbols.push(media_symbol);
                }
            }
            "supports_statement" => {
                if let Some(supports_symbol) = PropertyExtractor::extract_supports_rule(
                    &mut self.base,
                    node,
                    current_parent_id.as_deref(),
                ) {
                    current_parent_id = Some(supports_symbol.id.clone());
                    symbols.push(supports_symbol);
                }
            }
            "property_name" => {
                // CSS custom properties (variables)
                let property_text = self.base.get_node_text(&node);
                if property_text.starts_with("--") {
                    if let Some(custom_prop) = PropertyExtractor::extract_custom_property(
                        &mut self.base,
                        node,
                        current_parent_id.as_deref(),
                    ) {
                        symbols.push(custom_prop);
                    }
                }
            }
            _ => {}
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    // ========================================================================
    // Identifier Extraction (for LSP-quality find_references)
    // ========================================================================

    /// Extract all identifier usages (CSS functions, class/id selectors)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        IdentifierExtractor::extract_identifiers(&mut self.base, tree, symbols)
    }
}
