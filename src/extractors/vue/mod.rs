// Vue Single File Component (SFC) Extractor
//
// Parses .vue files by extracting template, script, and style sections
// and delegating to appropriate parsers for each section.
//
// Port of Miller's Vue extractor with comprehensive Vue SFC feature support

use crate::extractors::base::{
    BaseExtractor, Identifier, Relationship, Symbol, SymbolKind,
};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Tree;

// Private modules
mod component;
mod helpers;
mod identifiers;
mod parsing;
mod script;
mod style;
mod template;

// Public re-exports
pub use crate::extractors::base::{IdentifierKind, RelationshipKind};

use parsing::{parse_vue_sfc, VueSection};
use script::create_symbol_manual;

/// Vue Single File Component (SFC) Extractor
///
/// Parses .vue files by extracting template, script, and style sections
/// and delegating to appropriate existing parsers.
pub struct VueExtractor {
    base: BaseExtractor,
}

impl VueExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    /// Extract all symbols from Vue SFC - doesn't use tree-sitter
    /// Port of Miller's extractSymbols logic
    pub fn extract_symbols(&mut self, _tree: Option<&Tree>) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Parse Vue SFC structure - following Miller's approach
        match parse_vue_sfc(&self.base.content.clone()) {
            Ok(sections) => {
                // Extract symbols from each section
                for section in &sections {
                    let section_symbols = self.extract_section_symbols(&section);
                    symbols.extend(section_symbols);
                }

                // Add component-level symbol - following Miller's logic
                if let Some(component_name) = component::extract_component_name(&self.base.file_path, &sections) {
                    let component_symbol = create_symbol_manual(
                        &self.base,
                        &component_name,
                        SymbolKind::Class,
                        1,
                        1,
                        self.base.content.lines().count(),
                        1,
                        Some(format!("<{} />", component_name)),
                        Some(format!("Vue Single File Component: {}", component_name)),
                        Some({
                            let mut metadata = HashMap::new();
                            metadata
                                .insert("type".to_string(), Value::String("vue-sfc".to_string()));
                            metadata.insert(
                                "sections".to_string(),
                                Value::String(
                                    sections
                                        .iter()
                                        .map(|s| s.section_type.clone())
                                        .collect::<Vec<_>>()
                                        .join(","),
                                ),
                            );
                            metadata
                        }),
                    );
                    symbols.push(component_symbol);
                }
            }
            Err(_e) => {
                // Error extracting Vue symbols - continue silently
            }
        }

        symbols
    }

    /// Extract relationships from Vue SFC
    pub fn extract_relationships(
        &mut self,
        _tree: Option<&Tree>,
        _symbols: &[Symbol],
    ) -> Vec<Relationship> {
        // Miller's implementation returns empty for now - follow the same approach
        Vec::new()
    }

    /// Infer types from Vue SFC
    pub fn infer_types(&mut self, _symbols: &[Symbol]) -> HashMap<String, String> {
        // Miller's implementation returns empty for now - follow the same approach
        HashMap::new()
    }

    /// Extract symbols from a specific section using appropriate parser
    /// Port of Miller's extractSectionSymbols logic
    fn extract_section_symbols(&self, section: &VueSection) -> Vec<Symbol> {
        match section.section_type.as_str() {
            "script" => {
                // Extract basic Vue component structure - following Miller's approach
                script::extract_script_symbols(&self.base, section)
            }
            "template" => {
                // Extract template symbols (components, directives, etc.)
                template::extract_template_symbols(&self.base, section)
            }
            "style" => {
                // Extract CSS class names, etc.
                style::extract_style_symbols(&self.base, section)
            }
            _ => Vec::new(),
        }
    }

    // ========================================================================
    // Identifier Extraction (for LSP-quality find_references)
    // ========================================================================

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Vue-specific: Parses <script> section with JavaScript tree-sitter
    pub fn extract_identifiers(&mut self, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, symbols)
    }
}
