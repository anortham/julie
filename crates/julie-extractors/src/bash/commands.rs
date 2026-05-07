//! Command extraction for Bash
//!
//! Handles Bash import-style command symbols like `alias` and `source`.
//! External command calls stay in identifiers and relationships.

use crate::base::{Symbol, SymbolKind, SymbolOptions, UnresolvedTarget, Visibility};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::Node;

const SHELL_BUILTINS: &[&str] = &[
    "echo", "printf", "cd", "pwd", "ls", "mkdir", "rmdir", "rm", "cp", "mv", "cat", "grep", "sed",
    "awk", "find", "test", "[", "]", "return", "exit", "export", "declare", "local", "readonly",
    "unset", "set", "shopt", "read", "eval", "source", ".", "exec", "command", "type", "which",
    "alias", "unalias", "true", "false", ":", "!", "history", "if", "then", "else", "elif", "fi",
    "case", "esac", "for", "while", "until", "do", "done", "break", "continue",
];

impl super::BashExtractor {
    /// Extract command-like symbols from Bash.
    ///
    /// External commands are handled as identifiers and relationships, not
    /// fake Function symbols. Bash-specific import-style commands stay as
    /// searchable symbols.
    pub(super) fn extract_command(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let command_name_node = self.find_command_name_node(node)?;
        let command_name = self.base.get_node_text(&command_name_node);
        let command_text = self.base.get_node_text(&node);

        if command_name == "alias" {
            return extract_alias_symbol(self, &node, parent_id, &command_text);
        }

        if is_import_command(&command_name) {
            return extract_source_symbol(self, &node, parent_id, &command_name, &command_text);
        }

        None
    }
}

pub(super) fn is_shell_builtin(name: &str) -> bool {
    SHELL_BUILTINS.contains(&name)
}

pub(super) fn is_import_command(name: &str) -> bool {
    matches!(name, "source" | ".")
}

pub(super) fn extract_source_target(
    command_name: &str,
    command_text: &str,
) -> Option<UnresolvedTarget> {
    let rest = command_text.strip_prefix(command_name)?.trim_start();
    let source_path = extract_first_shell_argument(rest)?;
    let clean_path = strip_shell_quotes(&source_path).to_string();
    let terminal_name = source_symbol_name(&clean_path)?;

    Some(UnresolvedTarget {
        display_name: clean_path,
        terminal_name,
        receiver: None,
        namespace_path: Vec::new(),
        import_context: Some(command_name.to_string()),
    })
}

fn extract_alias_symbol(
    extractor: &mut super::BashExtractor,
    node: &Node,
    parent_id: Option<&str>,
    command_text: &str,
) -> Option<Symbol> {
    let rest = command_text.strip_prefix("alias")?.trim_start();
    let (name_part, target_part) = rest.split_once('=')?;
    let alias_name = name_part.split_whitespace().next()?.trim();
    if alias_name.is_empty() || alias_name.starts_with('-') {
        return None;
    }

    let alias_target = strip_shell_quotes(target_part.trim()).to_string();
    if alias_target.is_empty() {
        return None;
    }
    let visibility = if parent_id.is_some() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    let mut metadata = HashMap::new();
    metadata.insert("aliasTarget".to_string(), json!(alias_target));
    metadata.insert("command".to_string(), json!("alias"));

    Some(extractor.base.create_symbol(
        node,
        alias_name.to_string(),
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(command_text.trim().to_string()),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(node),
            annotations: Vec::new(),
        },
    ))
}

fn extract_source_symbol(
    extractor: &mut super::BashExtractor,
    node: &Node,
    parent_id: Option<&str>,
    command_name: &str,
    command_text: &str,
) -> Option<Symbol> {
    let target = extract_source_target(command_name, command_text)?;
    let source_path = target.display_name.clone();
    let source_name = target.terminal_name.clone();
    let visibility = if parent_id.is_some() {
        Visibility::Private
    } else {
        Visibility::Public
    };
    let mut metadata = HashMap::new();
    metadata.insert("sourcePath".to_string(), json!(source_path));
    metadata.insert("sourceCommand".to_string(), json!(command_name));

    Some(extractor.base.create_symbol(
        node,
        source_name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(command_text.trim().to_string()),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(node),
            annotations: Vec::new(),
        },
    ))
}

fn extract_first_shell_argument(text: &str) -> Option<String> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }

    let mut chars = text.chars();
    let first = chars.next()?;
    if first == '"' || first == '\'' {
        let remainder = &text[first.len_utf8()..];
        let closing = remainder.find(first)?;
        Some(remainder[..closing].to_string())
    } else {
        Some(text.split_whitespace().next()?.to_string())
    }
}

fn strip_shell_quotes(text: &str) -> &str {
    let text = text.trim();
    if text.len() >= 2 {
        let bytes = text.as_bytes();
        let first = bytes[0];
        let last = bytes[text.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &text[1..text.len() - 1];
        }
    }
    text
}

fn source_symbol_name(source_path: &str) -> Option<String> {
    let path = Path::new(source_path);
    let file_name = path.file_name().and_then(|name| name.to_str())?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or(file_name);

    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}
