/// Razor (.cshtml) language extractor with C# code blocks and HTML templates
///
/// This extractor handles Razor files which contain:
/// - Razor-specific directives (@page, @model, @using, etc.)
/// - C# code blocks (@code, @functions, @{...})
/// - HTML elements and Razor components
/// - Data bindings (@bind-Value)
/// - Event handlers (@onclick, etc.)
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::{Node, Tree};

// Module declarations
mod csharp;
mod directives;
mod helpers;
mod html;
mod identifiers;
mod relationships;
mod stubs;
mod type_inference;

pub struct RazorExtractor {
    base: BaseExtractor,
}

impl RazorExtractor {
    /// Create a new Razor extractor
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

    /// Extract symbols from the Razor file
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Visit a node and extract symbols recursively
    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        // Handle ERROR nodes by falling back to text-based extraction
        if node.kind() == "ERROR" {
            self.extract_from_text_content(node, symbols, parent_id.as_deref());
            return;
        }

        if !self.is_valid_node(&node) {
            return;
        }

        let mut symbol = None;
        let node_type = node.kind();

        match node_type {
            "razor_directive"
            | "razor_inject_directive"
            | "razor_using_directive"
            | "razor_page_directive"
            | "razor_namespace_directive"
            | "razor_model_directive"
            | "razor_attribute_directive"
            | "razor_inherits_directive"
            | "razor_implements_directive"
            | "razor_addtaghelper_directive" => {
                symbol = self.extract_directive(node, parent_id.as_deref());
            }
            "at_namespace" | "at_inherits" | "at_implements" => {
                symbol = self.extract_token_directive(node, parent_id.as_deref());
            }
            "razor_section" => {
                symbol = self.extract_section(node, parent_id.as_deref());
            }
            "razor_block" => {
                symbol = self.extract_code_block(node, parent_id.as_deref());
                // Extract C# symbols from within the block
                self.extract_csharp_symbols(
                    node,
                    symbols,
                    symbol
                        .as_ref()
                        .map(|s| s.id.as_str())
                        .or(parent_id.as_deref()),
                );
                // Don't visit children since we already extracted them
                return;
            }
            "razor_expression" | "razor_implicit_expression" => {
                symbol = self.extract_expression(node, parent_id.as_deref());
            }
            "html_element" | "element" => {
                symbol = self.extract_html_element(node, parent_id.as_deref());
                // Also extract binding attributes from HTML elements
                self.extract_binding_attributes_from_element(node, symbols, parent_id.as_deref());
                // Create external component symbols for uppercase tag names (standard approach)
                self.create_external_component_symbols_if_needed(node, symbols);
            }
            "razor_component" => {
                symbol = self.extract_component(node, parent_id.as_deref());
                // Also create external component symbols for razor components
                self.create_external_component_symbols_if_needed(node, symbols);
            }
            "csharp_code" => {
                self.extract_csharp_symbols(node, symbols, parent_id.as_deref());
            }
            "using_directive" => {
                symbol = self.extract_using(node, parent_id.as_deref());
            }
            "namespace_declaration" => {
                symbol = self.extract_namespace(node, parent_id.as_deref());
            }
            "class_declaration" => {
                symbol = self.extract_class(node, parent_id.as_deref());
            }
            "method_declaration" => {
                symbol = self.extract_method(node, parent_id.as_deref());
            }
            "property_declaration" => {
                symbol = self.extract_property(node, parent_id.as_deref());
            }
            "field_declaration" => {
                symbol = self.extract_field(node, parent_id.as_deref());
            }
            "local_function_statement" => {
                symbol = self.extract_local_function(node, parent_id.as_deref());
            }
            "local_declaration_statement" => {
                symbol = self.extract_local_variable(node, parent_id.as_deref());
            }
            "assignment_expression" => {
                symbol = self.extract_assignment(node, parent_id.as_deref());
            }
            "invocation_expression" => {
                symbol = self.extract_invocation(node, parent_id.as_deref());
            }
            "razor_html_attribute" => {
                symbol = self.extract_html_attribute(node, parent_id.as_deref(), symbols);
            }
            "attribute" => {
                symbol = self.extract_razor_attribute(node, parent_id.as_deref());
            }
            _ => {}
        }

        let current_parent_id = if let Some(sym) = &symbol {
            symbols.push(sym.clone());
            Some(sym.id.clone())
        } else {
            parent_id
        };

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract symbols from ERROR nodes using regex-based text parsing
    fn extract_from_text_content(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let content = self.base.get_node_text(&node);

        // Extract Razor directives from text
        use regex::Regex;

        // Look for @inherits directive
        let inherits_regex = Regex::new(r"@inherits\s+(\S+)").unwrap();
        if let Some(captures) = inherits_regex.captures(&content) {
            if let Some(base_class) = captures.get(1) {
                let symbol = self.base.create_symbol(
                    &node,
                    format!("inherits {}", base_class.as_str()),
                    SymbolKind::Import,
                    SymbolOptions {
                        signature: Some(format!("@inherits {}", base_class.as_str())),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: None,
                    },
                );
                symbols.push(symbol);
            }
        }

        // Look for HTML elements/components
        let component_regex = Regex::new(r"<(\w+)").unwrap();
        for captures in component_regex.captures_iter(&content) {
            if let Some(tag_name) = captures.get(1) {
                let tag = tag_name.as_str();
                // Only extract custom components (starting with uppercase)
                if tag.chars().next().unwrap_or('a').is_uppercase() {
                    let symbol = self.base.create_symbol(
                        &node,
                        tag.to_string(),
                        SymbolKind::Class,
                        SymbolOptions {
                            signature: Some(format!("<{}>", tag)),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: None,
                        },
                    );
                    symbols.push(symbol);
                }
            }
        }

        // Look for @rendermode directives
        let rendermode_regex = Regex::new(r#"@rendermode="([^"]+)""#).unwrap();
        for captures in rendermode_regex.captures_iter(&content) {
            if let Some(mode) = captures.get(1) {
                let symbol = self.base.create_symbol(
                    &node,
                    format!("rendermode {}", mode.as_str()),
                    SymbolKind::Property,
                    SymbolOptions {
                        signature: Some(format!("@rendermode=\"{}\"", mode.as_str())),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: None,
                    },
                );
                symbols.push(symbol);
            }
        }
    }
}
