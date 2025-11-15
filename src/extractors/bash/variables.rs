//! Variable and declaration extraction for Bash
//!
//! Handles extraction of variable assignments, exports, readonly declarations,
//! and environment variable classification.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

impl super::BashExtractor {
    /// Extract a variable assignment (VAR=value)
    pub(super) fn extract_variable(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let name_node = self.find_variable_name_node(node)?;
        let name = self.base.get_node_text(&name_node);

        // Check if it's an environment variable or local variable
        let is_environment = self.is_environment_variable(node, &name);
        let is_exported = self.is_exported_variable(node);

        let options = SymbolOptions {
            signature: Some(self.extract_variable_signature(node)),
            visibility: if is_exported {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            },
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: self.base.find_doc_comment(&node),
            ..Default::default()
        };

        let symbol_kind = if is_environment {
            SymbolKind::Constant
        } else {
            SymbolKind::Variable
        };
        Some(self.base.create_symbol(&node, name, symbol_kind, options))
    }

    /// Extract variables from declaration commands (declare, export, readonly)
    pub(super) fn extract_declaration(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Handle declare, export, readonly commands
        let declaration_text = self.base.get_node_text(&node);
        let declaration_type = declaration_text.split_whitespace().next()?;

        // Look for variable assignments within the declaration
        let assignments = self.get_children_of_type(node, "variable_assignment");
        if let Some(assignment) = assignments.first() {
            let assignment = *assignment;
            let name_node = self.find_variable_name_node(assignment)?;
            let name = self.base.get_node_text(&name_node);

            // Check if it's readonly: either 'readonly' command or 'declare -r'
            let is_readonly = declaration_type == "readonly"
                || declaration_type.contains("readonly")
                || (declaration_type == "declare" && declaration_text.contains(" -r "));

            // Check if it's an environment variable (but not if it's readonly)
            let _is_environment = !is_readonly && self.is_environment_variable(assignment, &name);
            let is_exported = declaration_type == "export";

            let options = SymbolOptions {
                signature: Some(format!("{} {}", declaration_type, name)),
                visibility: if is_exported {
                    Some(Visibility::Public)
                } else {
                    Some(Visibility::Private)
                },
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: self.base.find_doc_comment(&node),
                ..Default::default()
            };

            let symbol_kind = if is_readonly {
                SymbolKind::Constant
            } else {
                SymbolKind::Variable
            };
            return Some(
                self.base
                    .create_symbol(&assignment, name, symbol_kind, options),
            );
        }

        None
    }

    /// Check if a variable name matches environment variable patterns
    pub(super) fn is_environment_variable(&self, _node: Node, name: &str) -> bool {
        // Common environment variables
        let env_vars = [
            "PATH",
            "HOME",
            "USER",
            "PWD",
            "SHELL",
            "TERM",
            "NODE_ENV",
            "PYTHON_PATH",
            "JAVA_HOME",
            "GOPATH",
            "DOCKER_HOST",
            "KUBECONFIG",
        ];

        env_vars.contains(&name)
            || regex::Regex::new(r"^[A-Z_][A-Z0-9_]*$")
                .unwrap()
                .is_match(name)
    }

    /// Check if a variable assignment is preceded by 'export'
    pub(super) fn is_exported_variable(&self, node: Node) -> bool {
        // Check if the assignment is preceded by 'export'
        let mut current = node.prev_named_sibling();
        while let Some(sibling) = current {
            let text = self.base.get_node_text(&sibling);
            if text == "export" {
                return true;
            }
            current = sibling.prev_named_sibling();
        }
        false
    }
}
