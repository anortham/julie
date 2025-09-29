use crate::extractors::base::{
    BaseExtractor, Relationship, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

// Static regex patterns compiled once for performance
static TEMPLATE_START_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<template(\s+[^>]*)?>").unwrap());
static SCRIPT_START_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<script(\s+[^>]*)?>").unwrap());
static STYLE_START_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<style(\s+[^>]*)?>").unwrap());
static SECTION_END_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^</(template|script|style)>").unwrap());
static LANG_ATTR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"lang=["']?([^"'\s>]+)"#).unwrap());
static COMPONENT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"name\s*:\s*['"`]([^'"`]+)['"`]"#).unwrap());
static DATA_FUNCTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*data\s*\(\s*\)\s*\{").unwrap());
static METHODS_OBJECT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*methods\s*:\s*\{").unwrap());
static COMPUTED_OBJECT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*computed\s*:\s*\{").unwrap());
static PROPS_OBJECT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*props\s*:\s*\{").unwrap());
static FUNCTION_DEF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\([^)]*\)\s*\{").unwrap());
static COMPONENT_USAGE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<([A-Z][a-zA-Z0-9-]*)").unwrap());
static DIRECTIVE_USAGE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s(v-[a-zA-Z-]+)=").unwrap());
static CSS_CLASS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.([a-zA-Z_-][a-zA-Z0-9_-]*)\s*\{").unwrap());

/// Vue Single File Component (SFC) Extractor
///
/// Parses .vue files by extracting template, script, and style sections
/// and delegating to appropriate existing parsers.
///
/// Port of Miller's Vue extractor with comprehensive Vue SFC feature support
pub struct VueExtractor {
    base: BaseExtractor,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VueSection {
    section_type: String, // "template", "script", "style"
    content: String,
    start_line: usize,
    #[allow(dead_code)]
    end_line: usize,
    #[allow(dead_code)]
    lang: Option<String>, // e.g., 'ts', 'scss'
}

impl VueExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    /// Extract all symbols from Vue SFC - doesn't use tree-sitter
    /// Port of Miller's extractSymbols logic
    pub fn extract_symbols(&mut self, _tree: Option<&tree_sitter::Tree>) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Parse Vue SFC structure - following Miller's approach
        match self.parse_vue_sfc(&self.base.content.clone()) {
            Ok(sections) => {
                // Extract symbols from each section
                for section in &sections {
                    let section_symbols = self.extract_section_symbols(section);
                    symbols.extend(section_symbols);
                }

                // Add component-level symbol - following Miller's logic
                if let Some(component_name) = self.extract_component_name(&sections) {
                    let component_symbol = self.create_symbol_manual(
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
        _tree: Option<&tree_sitter::Tree>,
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

    /// Parse Vue SFC structure to extract template, script, and style sections
    /// Port of Miller's parseVueSFC logic
    fn parse_vue_sfc(&self, content: &str) -> Result<Vec<VueSection>, Box<dyn std::error::Error>> {
        let mut sections = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let mut current_section: Option<VueSectionBuilder> = None;
        let mut section_content = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check for section start - following Miller's regex patterns
            let template_match = TEMPLATE_START_RE.captures(trimmed);
            let script_match = SCRIPT_START_RE.captures(trimmed);
            let style_match = STYLE_START_RE.captures(trimmed);

            if template_match.is_some() || script_match.is_some() || style_match.is_some() {
                // End previous section
                if let Some(section) = current_section.take() {
                    sections.push(section.build(section_content.join("\n"), i));
                }

                // Start new section
                let section_type = if template_match.is_some() {
                    "template"
                } else if script_match.is_some() {
                    "script"
                } else {
                    "style"
                };

                let attrs = template_match
                    .or(script_match)
                    .or(style_match)
                    .and_then(|m| m.get(1))
                    .map(|m| m.as_str())
                    .unwrap_or("");

                let lang = LANG_ATTR_RE.captures(attrs)
                    .and_then(|m| m.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| match section_type {
                        "script" => "js".to_string(),
                        "style" => "css".to_string(),
                        _ => "html".to_string(),
                    });

                current_section = Some(VueSectionBuilder {
                    section_type: section_type.to_string(),
                    start_line: i + 1,
                    lang: Some(lang),
                });
                section_content.clear();
                continue;
            }

            // Check for section end
            if SECTION_END_RE.is_match(trimmed)
            {
                if let Some(section) = current_section.take() {
                    sections.push(section.build(section_content.join("\n"), i));
                    section_content.clear();
                }
                continue;
            }

            // Add content to current section
            if current_section.is_some() {
                section_content.push(line.to_string());
            }
        }

        // Handle unclosed section - following Miller's logic
        if let Some(section) = current_section {
            sections.push(section.build(section_content.join("\n"), lines.len()));
        }

        Ok(sections)
    }

    /// Helper to create symbols manually (without Parser.SyntaxNode)
    /// Port of Miller's createSymbolManual logic
    fn create_symbol_manual(
        &self,
        name: &str,
        kind: SymbolKind,
        start_line: usize,
        start_column: usize,
        end_line: usize,
        end_column: usize,
        signature: Option<String>,
        documentation: Option<String>,
        metadata: Option<HashMap<String, Value>>,
    ) -> Symbol {
        let options = SymbolOptions {
            signature,
            doc_comment: documentation,
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata,
        };

        // Generate ID similar to Miller's approach
        let id = format!("{}:{}:{}", name, start_line, start_column);

        Symbol {
            id,
            name: name.to_string(),
            kind,
            language: self.base.language.clone(),
            file_path: self.base.file_path.clone(),
            start_line: start_line as u32,
            start_column: start_column as u32,
            end_line: end_line as u32,
            end_column: end_column as u32,
            start_byte: 0, // Not available without tree-sitter node
            end_byte: 0,   // Not available without tree-sitter node
            signature: options.signature,
            doc_comment: options.doc_comment,
            visibility: options.visibility,
            parent_id: options.parent_id,
            metadata: Some(options.metadata.unwrap_or_default()),
            semantic_group: None, // Vue components don't have cross-language groups yet
            confidence: None,     // Will be set during validation
            code_context: None,   // Will be populated during context extraction
        }
    }

    /// Extract symbols from a specific section using appropriate parser
    /// Port of Miller's extractSectionSymbols logic
    fn extract_section_symbols(&self, section: &VueSection) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        match section.section_type.as_str() {
            "script" => {
                // Extract basic Vue component structure - following Miller's approach
                symbols.extend(self.extract_script_symbols_basic(section));
            }
            "template" => {
                // Extract template symbols (components, directives, etc.)
                symbols.extend(self.extract_template_symbols(section));
            }
            "style" => {
                // Extract CSS class names, etc.
                symbols.extend(self.extract_style_symbols(section));
            }
            _ => {}
        }

        symbols
    }

    /// Basic script symbol extraction (without full tree-sitter parsing)
    /// Port of Miller's extractScriptSymbolsBasic logic
    fn extract_script_symbols_basic(&self, section: &VueSection) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines: Vec<&str> = section.content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let actual_line = section.start_line + i;

            // Extract Vue component options - following Miller's patterns
            {
                let data_regex = &*DATA_FUNCTION_RE;
                if data_regex.is_match(line) {
                    symbols.push(self.create_symbol_manual(
                        "data",
                        SymbolKind::Function,
                        actual_line,
                        1,
                        actual_line,
                        5,
                        Some("data()".to_string()),
                        Some("Vue component data".to_string()),
                        None,
                    ));
                }
            }

            {
                let methods_regex = &*METHODS_OBJECT_RE;
                if methods_regex.is_match(line) {
                    symbols.push(self.create_symbol_manual(
                        "methods",
                        SymbolKind::Property,
                        actual_line,
                        1,
                        actual_line,
                        8,
                        Some("methods: {}".to_string()),
                        Some("Vue component methods".to_string()),
                        None,
                    ));
                }
            }

            {
                let computed_regex = &*COMPUTED_OBJECT_RE;
                if computed_regex.is_match(line) {
                    symbols.push(self.create_symbol_manual(
                        "computed",
                        SymbolKind::Property,
                        actual_line,
                        1,
                        actual_line,
                        9,
                        Some("computed: {}".to_string()),
                        Some("Vue computed properties".to_string()),
                        None,
                    ));
                }
            }

            {
                let props_regex = &*PROPS_OBJECT_RE;
                if props_regex.is_match(line) {
                    symbols.push(self.create_symbol_manual(
                        "props",
                        SymbolKind::Property,
                        actual_line,
                        1,
                        actual_line,
                        6,
                        Some("props: {}".to_string()),
                        Some("Vue component props".to_string()),
                        None,
                    ));
                }
            }

            // Extract function definitions - following Miller's pattern
            {
                let func_regex = &*FUNCTION_DEF_RE;
                if let Some(captures) = func_regex.captures(line) {
                    if let Some(func_name) = captures.get(1) {
                        let name = func_name.as_str();
                        let start_col = line.find(name).unwrap_or(0) + 1;
                        symbols.push(self.create_symbol_manual(
                            name,
                            SymbolKind::Method,
                            actual_line,
                            start_col,
                            actual_line,
                            start_col + name.len(),
                            Some(format!("{}()", name)),
                            Some("Vue component method".to_string()),
                            None,
                        ));
                    }
                }
            }
        }

        symbols
    }

    /// Extract template symbols (component usage, directives, etc.)
    /// Port of Miller's extractTemplateSymbols logic
    fn extract_template_symbols(&self, section: &VueSection) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines: Vec<&str> = section.content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let actual_line = section.start_line + i;

            // Extract component usage - following Miller's pattern
            {
                let component_regex = &*COMPONENT_USAGE_RE;
                for captures in component_regex.captures_iter(line) {
                    if let Some(component_name) = captures.get(1) {
                        let name = component_name.as_str();
                        let start_col = component_name.start() + 1;
                        symbols.push(self.create_symbol_manual(
                            name,
                            SymbolKind::Class,
                            actual_line,
                            start_col,
                            actual_line,
                            start_col + name.len(),
                            Some(format!("<{}>", name)),
                            Some("Vue component usage".to_string()),
                            None,
                        ));
                    }
                }
            }

            // Extract directives - following Miller's pattern
            {
                let directive_regex = &*DIRECTIVE_USAGE_RE;
                for captures in directive_regex.captures_iter(line) {
                    if let Some(directive_name) = captures.get(1) {
                        let name = directive_name.as_str();
                        let start_col = directive_name.start() + 1;
                        symbols.push(self.create_symbol_manual(
                            name,
                            SymbolKind::Property,
                            actual_line,
                            start_col,
                            actual_line,
                            start_col + name.len(),
                            Some(name.to_string()),
                            Some("Vue directive".to_string()),
                            None,
                        ));
                    }
                }
            }
        }

        symbols
    }

    /// Extract style symbols (class names, etc.)
    /// Port of Miller's extractStyleSymbols logic
    fn extract_style_symbols(&self, section: &VueSection) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let lines: Vec<&str> = section.content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let actual_line = section.start_line + i;

            // Extract CSS class names - following Miller's pattern
            {
                let class_regex = &*CSS_CLASS_RE;
                for captures in class_regex.captures_iter(line) {
                    if let Some(class_name) = captures.get(1) {
                        let name = class_name.as_str();
                        let start_col = class_name.start() + 1;
                        symbols.push(self.create_symbol_manual(
                            name,
                            SymbolKind::Property,
                            actual_line,
                            start_col,
                            actual_line,
                            start_col + name.len(),
                            Some(format!(".{}", name)),
                            Some("CSS class".to_string()),
                            None,
                        ));
                    }
                }
            }
        }

        symbols
    }

    /// Extract component name from file path or script content
    /// Port of Miller's extractComponentName logic
    fn extract_component_name(&self, sections: &[VueSection]) -> Option<String> {
        // Try to extract from script section first - following Miller's logic
        for section in sections {
            if section.section_type == "script" {
                {
                    let name_regex = &*COMPONENT_NAME_RE;
                    if let Some(captures) = name_regex.captures(&section.content) {
                        if let Some(name_match) = captures.get(1) {
                            return Some(name_match.as_str().to_string());
                        }
                    }
                }
            }
        }

        // Fall back to file name - following Miller's logic
        let file_name = self
            .base
            .file_path
            .split('/')
            .next_back()
            .and_then(|name| name.strip_suffix(".vue"));

        if let Some(file_name) = file_name {
            // Convert kebab-case to PascalCase - following Miller's approach
            let pascal_case = file_name
                .split('-')
                .map(|part| {
                    let mut chars: Vec<char> = part.chars().collect();
                    if !chars.is_empty() {
                        chars[0] = chars[0].to_uppercase().next().unwrap_or(chars[0]);
                    }
                    chars.into_iter().collect::<String>()
                })
                .collect::<Vec<String>>()
                .join("");

            if !pascal_case.is_empty() {
                return Some(pascal_case);
            }
        }

        Some("VueComponent".to_string())
    }
}

/// Helper struct for building VueSection during parsing
#[derive(Debug)]
struct VueSectionBuilder {
    section_type: String,
    start_line: usize,
    lang: Option<String>,
}

impl VueSectionBuilder {
    fn build(self, content: String, end_line: usize) -> VueSection {
        VueSection {
            section_type: self.section_type,
            content,
            start_line: self.start_line,
            end_line,
            lang: self.lang,
        }
    }
}
