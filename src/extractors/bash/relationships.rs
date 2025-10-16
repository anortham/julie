//! Relationship extraction for Bash
//!
//! Handles extraction of relationships between symbols (calls, definitions, usages).

use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use tree_sitter::Node;

impl super::BashExtractor {
    /// Extract relationships between functions and commands they call
    pub(super) fn extract_command_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // Extract relationships between functions and the commands they call
        if let Some(command_name_node) = self.find_command_name_node(node) {
            let command_name = self.base.get_node_text(&command_name_node);
            let command_symbol = symbols
                .iter()
                .find(|s| s.name == command_name && s.kind == SymbolKind::Function);

            if let Some(cmd_sym) = command_symbol {
                // Find the parent function that calls this command
                let mut current = node.parent();
                while let Some(parent_node) = current {
                    if parent_node.kind() == "function_definition" {
                        if let Some(func_name_node) = self.find_name_node(parent_node) {
                            let func_name = self.base.get_node_text(&func_name_node);
                            let func_symbol = symbols
                                .iter()
                                .find(|s| s.name == func_name && s.kind == SymbolKind::Function);

                            if let Some(func_sym) = func_symbol {
                                if func_sym.id != cmd_sym.id {
                                    let relationship = self.base.create_relationship(
                                        func_sym.id.clone(),
                                        cmd_sym.id.clone(),
                                        RelationshipKind::Calls,
                                        &node,
                                        Some(1.0),
                                        None,
                                    );
                                    relationships.push(relationship);
                                }
                            }
                        }
                        break;
                    }
                    current = parent_node.parent();
                }
            }
        }
    }

    /// Extract relationships for command substitutions (for future use)
    pub(super) fn extract_command_substitution_relationships(
        &mut self,
        _node: Node,
        _symbols: &[Symbol],
        _relationships: &mut Vec<Relationship>,
    ) {
        // Extract relationships for command substitutions $(command) or `command`
        // These show data flow dependencies
        // Currently not implemented - available for future enhancement
    }

    /// Extract relationships for file redirections and pipes (for future use)
    pub(super) fn extract_file_relationships(
        &mut self,
        _node: Node,
        _symbols: &[Symbol],
        _relationships: &mut Vec<Relationship>,
    ) {
        // Extract relationships for file redirections and pipes
        // These show data flow between commands
        // Currently not implemented - available for future enhancement
    }
}
