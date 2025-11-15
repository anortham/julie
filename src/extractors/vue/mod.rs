// Vue Single File Component (SFC) Extractor
//
// Parses .vue files by extracting template, script, and style sections
// and delegating to appropriate parsers for each section.
//
// Implementation of Vue extractor with comprehensive Vue SFC feature support

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol, SymbolKind};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Tree;

// Private modules
mod component;
mod helpers;
mod identifiers;
pub(crate) mod parsing;
mod script;
mod style;
mod template;

// Public re-exports
pub use crate::extractors::base::{IdentifierKind, RelationshipKind};

use parsing::{VueSection, parse_vue_sfc};
use script::create_symbol_manual;

/// Vue Single File Component (SFC) Extractor
///
/// Parses .vue files by extracting template, script, and style sections
/// and delegating to appropriate existing parsers.
pub struct VueExtractor {
    base: BaseExtractor,
}

impl VueExtractor {
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

    /// Extract all symbols from Vue SFC - doesn't use tree-sitter
    /// Implementation of extractSymbols logic
    pub fn extract_symbols(&mut self, _tree: Option<&Tree>) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Parse Vue SFC structure - following standard approach
        match parse_vue_sfc(&self.base.content.clone()) {
            Ok(sections) => {
                // Extract symbols from each section
                for section in &sections {
                    let section_symbols = self.extract_section_symbols(section);
                    symbols.extend(section_symbols);
                }

                // Add component-level symbol - following reference logic
                if let Some(component_name) =
                    component::extract_component_name(&self.base.file_path, &sections)
                {
                    // Try to extract HTML comment from the beginning of the file
                    let doc_comment = extract_component_doc_comment(&self.base.content);

                    let component_symbol = create_symbol_manual(
                        &self.base,
                        &component_name,
                        SymbolKind::Class,
                        1,
                        1,
                        self.base.content.lines().count(),
                        1,
                        Some(format!("<{} />", component_name)),
                        doc_comment.or_else(|| {
                            Some(format!("Vue Single File Component: {}", component_name))
                        }),
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
        // implementation returns empty for now - follow the same approach
        Vec::new()
    }

    /// Infer types from Vue SFC
    pub fn infer_types(&mut self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            let metadata = &symbol.metadata;
            // Check for returnType (from methods/functions)
            if let Some(return_type) = metadata.as_ref().and_then(|m| m.get("returnType")) {
                if let Some(type_str) = return_type.as_str() {
                    types.insert(symbol.id.clone(), type_str.to_string());
                }
            }
            // Check for propertyType (from props/data)
            else if let Some(property_type) = metadata.as_ref().and_then(|m| m.get("propertyType")) {
                if let Some(type_str) = property_type.as_str() {
                    types.insert(symbol.id.clone(), type_str.to_string());
                }
            }
            // Check for type field (generic type info)
            else if let Some(type_val) = metadata.as_ref().and_then(|m| m.get("type")) {
                if let Some(type_str) = type_val.as_str() {
                    // Only include if it's an actual type, not a kind descriptor
                    if !matches!(type_str, "function" | "property" | "method") {
                        types.insert(symbol.id.clone(), type_str.to_string());
                    }
                }
            }
        }
        types
    }

    /// Extract symbols from a specific section using appropriate parser
    /// Implementation of extractSectionSymbols logic
    fn extract_section_symbols(&self, section: &VueSection) -> Vec<Symbol> {
        match section.section_type.as_str() {
            "script" => {
                // Extract basic Vue component structure - following standard approach
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

/// Extract HTML comment from the beginning of a Vue file (component-level doc)
/// Looks for HTML comments at the very start of the file before any tags
fn extract_component_doc_comment(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut comments = Vec::new();

    for line in lines.iter() {
        let trimmed = line.trim();

        // Stop if we hit a non-comment, non-empty line (like <template>)
        if !trimmed.is_empty()
            && !trimmed.starts_with("<!--")
            && !trimmed.starts_with("-->")
            && !trimmed.starts_with("*")
        {
            break;
        }

        // Collect comment lines
        if trimmed.starts_with("<!--")
            || trimmed.starts_with("-->")
            || (trimmed.starts_with("*") && !comments.is_empty())
        {
            comments.push(*line);
        } else if trimmed.is_empty() && !comments.is_empty() {
            // Include empty lines within comment blocks
            comments.push(*line);
        }
    }

    if comments.is_empty() {
        None
    } else {
        Some(comments.join("\n"))
    }
}
