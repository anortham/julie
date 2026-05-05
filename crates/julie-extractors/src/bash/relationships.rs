//! Relationship extraction for Bash
//!
//! Handles extraction of relationships between symbols (calls, definitions, usages).

use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol, SymbolKind,
    UnresolvedTarget,
};
use tree_sitter::Node;

impl super::BashExtractor {
    /// Extract relationships between functions and commands they call
    pub(super) fn extract_command_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let Some(command_name_node) = self.find_command_name_node(node) else {
            return;
        };
        let command_name = self.base.get_node_text(&command_name_node);
        let Some(caller_symbol) = self
            .base
            .find_containing_symbol(&node, symbols)
            .filter(|symbol| symbol.kind == SymbolKind::Function)
        else {
            return;
        };

        let unresolved_target = UnresolvedTarget::simple(command_name.clone());
        let scoped_index = ScopedSymbolIndex::new(symbols);

        match scoped_index.resolve_call_target(
            &unresolved_target.terminal_name,
            Some(caller_symbol),
            unresolved_target.receiver.as_deref(),
        ) {
            LocalTargetResolution::Resolved(called_symbol) => {
                if caller_symbol.id != called_symbol.id {
                    let relationship = self.base.create_relationship(
                        caller_symbol.id.clone(),
                        called_symbol.id.clone(),
                        RelationshipKind::Calls,
                        &node,
                        Some(0.95),
                        None,
                    );
                    relationships.push(relationship);
                }
            }
            LocalTargetResolution::Import(_)
            | LocalTargetResolution::Ambiguous
            | LocalTargetResolution::Missing
            | LocalTargetResolution::ReceiverQualified => {
                if !is_builtin_command(&command_name) {
                    let pending = self.base.create_pending_relationship(
                        caller_symbol.id.clone(),
                        unresolved_target,
                        RelationshipKind::Calls,
                        &node,
                        Some(caller_symbol.id.clone()),
                        Some(0.8),
                    );
                    self.add_structured_pending_relationship(pending);
                }
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
