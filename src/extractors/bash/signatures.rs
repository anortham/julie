//! Signature extraction and documentation for Bash
//!
//! Provides functions for building signatures of functions, variables, commands,
//! and other Bash constructs.

use crate::extractors::base::BaseExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

impl super::BashExtractor {
    /// Build signature for a function definition
    pub(super) fn extract_function_signature(&self, node: Node) -> String {
        let name_node = self.find_name_node(node);
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());
        format!("function {}()", name)
    }

    /// Build signature for a variable assignment
    pub(super) fn extract_variable_signature(&self, node: Node) -> String {
        let name_node = self.find_variable_name_node(node);
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());

        // Get the full assignment text and extract value
        let full_text = self.base.get_node_text(&node);
        if let Some(equal_index) = full_text.find('=') {
            let value = full_text.get(equal_index + 1..).unwrap_or("").trim();
            if !value.is_empty() {
                return format!("{}={}", name, value);
            }
        }

        name
    }

    /// Build signature for a command (limited to 100 chars for readability)
    pub(super) fn extract_command_signature(&self, node: Node) -> String {
        // Get the full command with arguments
        let command_text = self.base.get_node_text(&node);

        // Limit length for readability - safely handle UTF-8
        BaseExtractor::truncate_string(&command_text, 97)
    }

    /// Build signature for control flow constructs (if, while, for)
    pub(super) fn extract_control_flow_signature(&self, node: Node) -> String {
        let control_type = node.kind().replace("_statement", "");

        // Try to extract the condition for if/while
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "test_command" | "condition") {
                let condition = self.base.get_node_text(&child);
                // Safely truncate UTF-8 string at character boundary
                let condition = BaseExtractor::truncate_string(&condition, 47);
                return format!("{} ({})", control_type, condition);
            }
        }

        format!("{} block", control_type)
    }

    /// Build documentation for a variable (annotations about readonly, exported, etc.)
    #[allow(dead_code)]
    pub(super) fn extract_variable_documentation(
        &self,
        _node: Node,
        is_environment: bool,
        is_exported: bool,
        is_readonly: bool,
    ) -> String {
        let mut annotations = Vec::new();

        if is_readonly {
            annotations.push("READONLY");
        }
        if is_environment {
            annotations.push("Environment Variable");
        }
        if is_exported {
            annotations.push("Exported");
        }

        if annotations.is_empty() {
            String::new()
        } else {
            format!("[{}]", annotations.join(", "))
        }
    }

    /// Build documentation for a command (identifies what external tool it is)
    #[allow(dead_code)]
    pub(super) fn get_command_documentation(&self, command_name: &str) -> String {
        let command_docs = [
            ("python", "[Python Interpreter Call]"),
            ("python3", "[Python 3 Interpreter Call]"),
            ("node", "[Node.js Runtime Call]"),
            ("npm", "[NPM Package Manager Call]"),
            ("bun", "[Bun Runtime Call]"),
            ("go", "[Go Command Call]"),
            ("cargo", "[Rust Cargo Call]"),
            ("java", "[Java Runtime Call]"),
            ("dotnet", "[.NET CLI Call]"),
            ("docker", "[Docker Container Call]"),
            ("kubectl", "[Kubernetes CLI Call]"),
            ("terraform", "[Infrastructure as Code Call]"),
            ("git", "[Version Control Call]"),
        ]
        .iter()
        .cloned()
        .collect::<HashMap<&str, &str>>();

        command_docs
            .get(command_name)
            .unwrap_or(&"[External Program Call]")
            .to_string()
    }
}
