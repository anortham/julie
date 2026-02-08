//! Relationship extraction for Bash
//!
//! Handles extraction of relationships between symbols (calls, definitions, usages).

use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind, PendingRelationship};
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
                            // Now check if the called command is in our symbol map
                            let command_symbol = symbols
                                .iter()
                                .find(|s| s.name == command_name && s.kind == SymbolKind::Function);

                            if let Some(cmd_sym) = command_symbol {
                                // Local function call - create resolved Relationship
                                if func_sym.id != cmd_sym.id {
                                    let relationship = self.base.create_relationship(
                                        func_sym.id.clone(),
                                        cmd_sym.id.clone(),
                                        RelationshipKind::Calls,
                                        &node,
                                        Some(0.95),
                                        None,
                                    );
                                    relationships.push(relationship);
                                }
                            } else if !is_builtin_command(&command_name) {
                                // Cross-file function call - create PendingRelationship
                                // (but skip built-in shell commands like echo, cd, etc.)
                                let pending = PendingRelationship {
                                    from_symbol_id: func_sym.id.clone(),
                                    callee_name: command_name.clone(),
                                    kind: RelationshipKind::Calls,
                                    file_path: self.base.file_path.clone(),
                                    line_number: (node.start_position().row + 1) as u32,
                                    confidence: 0.8,
                                };
                                self.add_pending_relationship(pending);
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

/// Check if a command is a built-in shell command
/// Built-in commands shouldn't create pending relationships since they're not user-defined functions
fn is_builtin_command(name: &str) -> bool {
    matches!(
        name,
        // Core shell builtins
        "echo" | "printf" | "cd" | "pwd" | "ls" | "mkdir" | "rmdir" | "rm" | "cp" | "mv" |
        "cat" | "grep" | "sed" | "awk" | "find" | "test" | "[" | "]" | "return" | "exit" |
        "export" | "declare" | "local" | "readonly" | "unset" | "set" | "shopt" |
        "read" | "eval" | "source" | "." | "exec" | "command" | "type" | "which" |
        "alias" | "unalias" | "true" | "false" | ":" | "!" | "history" |
        // Control flow
        "if" | "then" | "else" | "elif" | "fi" | "case" | "esac" | "for" | "while" |
        "until" | "do" | "done" | "break" | "continue" |
        // Common external commands often called from bash
        "bash" | "sh" | "zsh" | "ksh" | "python" | "python3" | "node" | "ruby" |
        "perl" | "java" | "go" | "rust" | "docker" | "kubectl" | "npm" | "yarn" |
        "git" | "curl" | "wget" | "tar" | "gzip" | "zip" | "unzip" | "ssh" | "scp" |
        "rsync" | "systemctl" | "service" | "sudo" | "su" | "chmod" | "chown" |
        "chgrp" | "kill" | "pkill" | "ps" | "top" | "htop" | "free" | "df" |
        "du" | "mount" | "umount" | "fdisk" | "parted" | "mkfs" | "fsck"
    )
}
