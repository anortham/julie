//! PowerShell language extractor - Implementation of PowerShell extractor
//! Handles PowerShell-specific constructs for Windows/Azure DevOps
//!
//! Provides symbol extraction for:
//! - Functions (simple and advanced with [CmdletBinding()])
//! - Variables (scoped, environment, automatic variables)
//! - Classes, methods, properties, and enums (PowerShell 5.0+)
//! - Azure PowerShell cmdlets and Windows management commands
//! - Module imports, exports, and using statements
//! - Parameter definitions with attributes and validation
//! - Cross-platform DevOps tool calls (docker, kubectl, az CLI)
//!
//! Special focus on Windows/Azure DevOps tracing to complement Bash for complete
//! cross-platform deployment automation coverage.

pub mod classes;
pub mod commands;
pub mod documentation;
pub mod functions;
pub mod helpers;
pub mod identifiers;
pub mod imports;
pub mod relationships;
pub mod types;
pub mod variables;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use tree_sitter::Tree;

/// PowerShell language extractor that handles PowerShell-specific constructs for Windows/Azure DevOps
pub struct PowerShellExtractor {
    pub base: BaseExtractor,
}

impl PowerShellExtractor {
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

    /// Extract all symbols from the PowerShell AST
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree_for_symbols(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Walk the tree and extract symbols recursively
    fn walk_tree_for_symbols(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let mut current_parent_id = parent_id;

        if let Some(symbol) = self.extract_symbol_from_node(node, current_parent_id.as_deref()) {
            // If this is a function, extract its parameters
            if symbol.kind == crate::extractors::base::SymbolKind::Function {
                let parameters =
                    functions::extract_function_parameters(&mut self.base, node, &symbol.id);
                symbols.extend(parameters);
            }

            current_parent_id = Some(symbol.id.clone());
            symbols.push(symbol);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_symbols(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract a symbol from a single node based on its kind
    fn extract_symbol_from_node(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        match node.kind() {
            "function_statement" => functions::extract_function(&mut self.base, node, parent_id),
            "param_block" => functions::extract_advanced_function(&mut self.base, node, parent_id),
            "configuration" => self.extract_configuration(node, parent_id),
            "ERROR" => self.extract_error_node(node, parent_id),
            "assignment_expression" => variables::extract_variable(&mut self.base, node, parent_id),
            "variable" => variables::extract_variable_reference(&mut self.base, node, parent_id),
            "class_statement" => classes::extract_class(&mut self.base, node, parent_id),
            "class_method_definition" => classes::extract_method(&mut self.base, node, parent_id),
            "class_property_definition" => {
                classes::extract_property(&mut self.base, node, parent_id)
            }
            "enum_statement" => classes::extract_enum(&mut self.base, node, parent_id),
            "enum_member" => classes::extract_enum_member(&mut self.base, node, parent_id),
            "import_statement" | "using_statement" => {
                imports::extract_import(&mut self.base, node, parent_id)
            }
            "command" | "command_expression" | "pipeline" => {
                // Check for dot sourcing, DSC configuration, or regular commands
                let node_text = self.base.get_node_text(&node);
                if node_text.starts_with(".") {
                    imports::extract_dot_sourcing(&mut self.base, node, parent_id)
                } else if helpers::find_command_name_node(node)
                    .map(|cn| self.base.get_node_text(&cn) == "Configuration")
                    .unwrap_or(false)
                {
                    commands::extract_dsc_configuration(&mut self.base, node, parent_id)
                } else if helpers::find_command_name_node(node)
                    .map(|cn| {
                        let name = self.base.get_node_text(&cn);
                        matches!(
                            name.as_str(),
                            "Import-Module" | "Export-ModuleMember" | "using"
                        )
                    })
                    .unwrap_or(false)
                {
                    if let Some(command_name_node) = helpers::find_command_name_node(node) {
                        let cmd_name = self.base.get_node_text(&command_name_node);
                        imports::extract_import_command(&mut self.base, node, &cmd_name, parent_id)
                    } else {
                        None
                    }
                } else {
                    commands::extract_command(&mut self.base, node, parent_id)
                }
            }
            _ => None,
        }
    }

    /// Extract configuration statement
    fn extract_configuration(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let name_node = helpers::find_configuration_name_node(node)?;
        let name = self.base.get_node_text(&name_node);

        let signature = format!("Configuration {}", name);
        let doc_comment = Some("PowerShell DSC Configuration".to_string());

        Some(self.base.create_symbol(
            &node,
            name,
            crate::extractors::base::SymbolKind::Function, // DSC configurations are treated as functions for symbol purposes
            crate::extractors::base::SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment,
            },
        ))
    }

    /// Extract configuration/function from ERROR nodes
    fn extract_error_node(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let node_text = self.base.get_node_text(&node);

        // Check if this ERROR node contains a DSC configuration
        if node_text.contains("Configuration ") {
            if let Some((name, signature)) =
                commands::extract_configuration_from_error(&self.base, &node_text)
            {
                return Some(self.base.create_symbol(
                    &node,
                    name,
                    crate::extractors::base::SymbolKind::Function,
                    crate::extractors::base::SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(crate::extractors::base::Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: Some("PowerShell DSC Configuration".to_string()),
                    },
                ));
            }
        }

        // Also check for function definitions that might be in ERROR nodes
        if node_text.contains("function ") {
            if let Some((name, signature)) =
                commands::extract_function_from_error(&self.base, &node_text)
            {
                return Some(self.base.create_symbol(
                    &node,
                    name,
                    crate::extractors::base::SymbolKind::Function,
                    crate::extractors::base::SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(crate::extractors::base::Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: None,
                    },
                ));
            }
        }

        None
    }

    /// Extract relationships between symbols
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        relationships::walk_tree_for_relationships(
            &self.base,
            tree.root_node(),
            symbols,
            &mut relationships,
        );
        relationships
    }

    /// Infer types for symbols
    pub fn infer_types(&self, symbols: &[Symbol]) -> std::collections::HashMap<String, String> {
        types::infer_types(symbols)
    }

    /// Extract identifiers (function calls, member access, etc.)
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }
}
