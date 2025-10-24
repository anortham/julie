//! Command extraction for Bash
//!
//! Handles extraction of external command calls, with special focus on
//! cross-language tracing (Python, Node.js, Docker, kubectl, etc.).

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

impl super::BashExtractor {
    /// Extract external command calls (critical for cross-language tracing!)
    pub(super) fn extract_command(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let command_name_node = self.find_command_name_node(node)?;
        let command_name = self.base.get_node_text(&command_name_node);

        // Focus on commands that call other programs/languages
        let cross_language_commands = [
            "python",
            "python3",
            "node",
            "npm",
            "bun",
            "deno",
            "go",
            "cargo",
            "rustc",
            "java",
            "javac",
            "mvn",
            "dotnet",
            "php",
            "ruby",
            "gem",
            "docker",
            "kubectl",
            "helm",
            "terraform",
            "git",
            "curl",
            "wget",
            "ssh",
            "scp",
        ];

        let is_interesting = cross_language_commands.contains(&command_name.as_str())
            || command_name.starts_with("./")
            || command_name.contains('/');

        if is_interesting {
            let options = SymbolOptions {
                signature: Some(self.extract_command_signature(node)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: self.base.find_doc_comment(&node),
                ..Default::default()
            };

            Some(
                self.base
                    .create_symbol(&node, command_name, SymbolKind::Function, options),
            )
        } else {
            None
        }
    }

    /// Extract control flow constructs (for, while, if)
    pub(super) fn extract_control_flow(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract control flow constructs for understanding script logic
        let control_type = node.kind().replace("_statement", "");
        let name = format!("{} block", control_type);

        let options = SymbolOptions {
            signature: Some(self.extract_control_flow_signature(node)),
            visibility: Some(Visibility::Private),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: self.base.find_doc_comment(&node),
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Method, options),
        )
    }
}
