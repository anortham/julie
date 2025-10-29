/// Razor-specific directive extraction (e.g., @page, @model, @using, @inject)
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

impl super::RazorExtractor {
    /// Extract Razor directives (@page, @model, @using, etc.)
    pub(super) fn extract_directive(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let directive_name = self.extract_directive_name(node);
        let directive_value = self.extract_directive_value(node);

        let mut signature = format!("@{}", directive_name);
        if let Some(value) = &directive_value {
            signature.push_str(&format!(" {}", value));
        }

        let symbol_kind = self.get_directive_symbol_kind(&directive_name);

        // For certain directives, use the value as the symbol name
        let symbol_name = match directive_name.as_str() {
            "using" => directive_value
                .clone()
                .unwrap_or_else(|| format!("@{}", directive_name)),
            "inject" => {
                // Extract property name from "@inject IService PropertyName"
                if let Some(value) = &directive_value {
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 2 {
                        parts.last().unwrap().to_string()
                    } else {
                        format!("@{}", directive_name)
                    }
                } else {
                    format!("@{}", directive_name)
                }
            }
            _ => format!("@{}", directive_name),
        };

        // Extract Razor doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            symbol_name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-directive".to_string()),
                    );
                    metadata.insert(
                        "directiveName".to_string(),
                        serde_json::Value::String(directive_name.clone()),
                    );
                    if let Some(value) = directive_value {
                        metadata.insert(
                            "directiveValue".to_string(),
                            serde_json::Value::String(value),
                        );
                    }
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Extract directive name from node kind or text
    pub(super) fn extract_directive_name(&self, node: Node) -> String {
        match node.kind() {
            "razor_page_directive" => "page".to_string(),
            "razor_model_directive" => "model".to_string(),
            "razor_using_directive" => "using".to_string(),
            "razor_inject_directive" => "inject".to_string(),
            "razor_attribute_directive" => "attribute".to_string(),
            "razor_namespace_directive" => "namespace".to_string(),
            "razor_inherits_directive" => "inherits".to_string(),
            "razor_implements_directive" => "implements".to_string(),
            "razor_addtaghelper_directive" => "addTagHelper".to_string(),
            _ => {
                let text = self.base.get_node_text(&node);
                if text.contains("@addTagHelper") {
                    "addTagHelper".to_string()
                } else if let Some(captures) = regex::Regex::new(r"@(\w+)").unwrap().captures(&text)
                {
                    captures[1].to_string()
                } else {
                    "unknown".to_string()
                }
            }
        }
    }

    /// Extract directive value from node
    pub(super) fn extract_directive_value(&self, node: Node) -> Option<String> {
        match node.kind() {
            "razor_page_directive" => self
                .find_child_by_type(node, "string_literal")
                .map(|n| self.base.get_node_text(&n)),
            "razor_model_directive" | "razor_inherits_directive" | "razor_implements_directive" => {
                self.find_child_by_type(node, "identifier")
                    .map(|n| self.base.get_node_text(&n))
            }
            "razor_using_directive" | "razor_namespace_directive" => self
                .find_child_by_types(node, &["qualified_name", "identifier"])
                .map(|n| self.base.get_node_text(&n)),
            "razor_inject_directive" => self
                .find_child_by_type(node, "variable_declaration")
                .map(|n| self.base.get_node_text(&n)),
            "razor_attribute_directive" => self
                .find_child_by_type(node, "attribute_list")
                .map(|n| self.base.get_node_text(&n)),
            "razor_addtaghelper_directive" => {
                let text = self.base.get_node_text(&node);
                regex::Regex::new(r"@addTagHelper\s+(.+)")
                    .unwrap()
                    .captures(&text)
                    .map(|captures| captures[1].trim().to_string())
            }
            _ => {
                let text = self.base.get_node_text(&node);
                if text.contains("@addTagHelper") {
                    regex::Regex::new(r"@addTagHelper\s+(.+)")
                        .unwrap()
                        .captures(&text)
                        .map(|captures| captures[1].trim().to_string())
                } else {
                    regex::Regex::new(r"@\w+\s+(.*)")
                        .unwrap()
                        .captures(&text)
                        .map(|captures| captures[1].trim().to_string())
                }
            }
        }
    }

    /// Map directive name to symbol kind
    pub(super) fn get_directive_symbol_kind(&self, directive_name: &str) -> SymbolKind {
        match directive_name.to_lowercase().as_str() {
            "model" | "layout" => SymbolKind::Class,
            "page" | "using" | "namespace" => SymbolKind::Import,
            "inherits" | "implements" => SymbolKind::Interface,
            "inject" | "attribute" => SymbolKind::Property,
            "code" | "functions" => SymbolKind::Function,
            _ => SymbolKind::Variable,
        }
    }

    /// Extract token-based directives (@inherits, @namespace, @implements)
    pub(super) fn extract_token_directive(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let directive_type = node.kind().replace("at_", "");
        let directive_name = format!("@{}", directive_type);

        // Look for the directive value in siblings
        let directive_value = if let Some(parent) = node.parent() {
            let text = self.base.get_node_text(&parent);
            regex::Regex::new(&format!(r"@{}\s+(\S+)", directive_type))
                .unwrap()
                .captures(&text)
                .map(|captures| captures[1].to_string())
        } else {
            None
        };

        let signature = if let Some(ref value) = directive_value {
            format!("{} {}", directive_name, value)
        } else {
            directive_name.clone()
        };

        let symbol_kind = self.get_directive_symbol_kind(&directive_type);

        // Extract Razor doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            directive_name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-token-directive".to_string()),
                    );
                    metadata.insert(
                        "directiveType".to_string(),
                        serde_json::Value::String(directive_type.clone()),
                    );
                    if let Some(value) = directive_value {
                        metadata.insert(
                            "directiveValue".to_string(),
                            serde_json::Value::String(value),
                        );
                    }
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Extract section (@section) directives
    pub(super) fn extract_section(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let identifier_node = self.find_child_by_type(node, "identifier")?;
        let section_name = self.base.get_node_text(&identifier_node);
        let signature = format!("@section {}", section_name);

        // Extract Razor doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            section_name,
            SymbolKind::Module,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-section".to_string()),
                    );
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Extract code blocks (@code, @functions, @{...})
    pub(super) fn extract_code_block(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let block_type = self.get_code_block_type(node);
        let content = self.base.get_node_text(&node);
        // Safely truncate UTF-8 string at character boundary
        let truncated_content = BaseExtractor::truncate_string(&content, 50);

        let signature = format!("@{{ {} }}", truncated_content);

        // Extract Razor doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            format!("{}Block", block_type),
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-code-block".to_string()),
                    );
                    metadata.insert(
                        "blockType".to_string(),
                        serde_json::Value::String(block_type.clone()),
                    );
                    metadata.insert(
                        "content".to_string(),
                        serde_json::Value::String(content[..content.len().min(200)].to_string()),
                    );
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Determine code block type from node content
    pub(super) fn get_code_block_type(&self, node: Node) -> String {
        let text = self.base.get_node_text(&node);
        if text.contains("@code") {
            "code".to_string()
        } else if text.contains("@functions") {
            "functions".to_string()
        } else if text.contains("@{") {
            "expression".to_string()
        } else {
            "block".to_string()
        }
    }

    /// Extract Razor expressions (@variable, @(expression))
    pub(super) fn extract_expression(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let expression = self.base.get_node_text(&node);
        let variable_name = self
            .extract_variable_from_expression(&expression)
            .unwrap_or_else(|| "expression".to_string());

        // Extract Razor doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            variable_name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(format!("@{}", expression)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-expression".to_string()),
                    );
                    metadata.insert(
                        "expression".to_string(),
                        serde_json::Value::String(expression.clone()),
                    );
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Extract variable name from expression
    pub(super) fn extract_variable_from_expression(&self, expression: &str) -> Option<String> {
        regex::Regex::new(r"(\w+)")
            .unwrap()
            .captures(expression)
            .map(|captures| captures[1].to_string())
    }
}
