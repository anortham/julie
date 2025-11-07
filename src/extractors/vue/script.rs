// Vue script section symbol extraction
//
// Responsible for extracting Vue component options from the <script> section
// Handles data(), methods, computed, props, and function definitions

use super::helpers::{
    COMPUTED_OBJECT_RE, DATA_FUNCTION_RE, FUNCTION_DEF_RE, METHODS_OBJECT_RE, PROPS_OBJECT_RE,
};
use super::parsing::VueSection;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind};
use serde_json::Value;
use std::collections::HashMap;

/// Extract symbols from script section
pub(super) fn extract_script_symbols(base: &BaseExtractor, section: &VueSection) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = section.content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let actual_line = section.start_line + i;

        // Extract doc comment for this line (look backward from current line)
        let doc_comment = find_doc_comment_before(&lines, i);

        // Extract Vue component options - following standard patterns
        if DATA_FUNCTION_RE.is_match(line) {
            symbols.push(create_symbol_manual(
                base,
                "data",
                SymbolKind::Function,
                actual_line,
                1,
                actual_line,
                5,
                Some("data()".to_string()),
                doc_comment.clone(),
                None,
            ));
        }

        if METHODS_OBJECT_RE.is_match(line) {
            symbols.push(create_symbol_manual(
                base,
                "methods",
                SymbolKind::Property,
                actual_line,
                1,
                actual_line,
                8,
                Some("methods: {}".to_string()),
                doc_comment.clone(),
                None,
            ));
        }

        if COMPUTED_OBJECT_RE.is_match(line) {
            symbols.push(create_symbol_manual(
                base,
                "computed",
                SymbolKind::Property,
                actual_line,
                1,
                actual_line,
                9,
                Some("computed: {}".to_string()),
                doc_comment.clone(),
                None,
            ));
        }

        if PROPS_OBJECT_RE.is_match(line) {
            symbols.push(create_symbol_manual(
                base,
                "props",
                SymbolKind::Property,
                actual_line,
                1,
                actual_line,
                6,
                Some("props: {}".to_string()),
                doc_comment.clone(),
                None,
            ));
        }

        // Extract function definitions - following pattern
        if let Some(captures) = FUNCTION_DEF_RE.captures(line) {
            if let Some(func_name) = captures.get(1) {
                let name = func_name.as_str();
                let start_col = line.find(name).unwrap_or(0) + 1;
                symbols.push(create_symbol_manual(
                    base,
                    name,
                    SymbolKind::Method,
                    actual_line,
                    start_col,
                    actual_line,
                    start_col + name.len(),
                    Some(format!("{}()", name)),
                    doc_comment.clone(),
                    None,
                ));
            }
        }
    }

    symbols
}

/// Find doc comment before a given line index
/// Looks backward through the lines and collects consecutive comment lines
/// This is used for JSDoc-style comments in script sections
pub(super) fn find_doc_comment_before(lines: &[&str], current_idx: usize) -> Option<String> {
    if current_idx == 0 {
        return None;
    }

    let mut comments = Vec::new();
    let mut idx = current_idx - 1;

    // Look backward for comment lines
    loop {
        let line = lines[idx].trim();

        if is_doc_comment_line(line) {
            comments.push(lines[idx]);
            if idx == 0 {
                break;
            }
            idx -= 1;
        } else if line.is_empty() {
            // Skip empty lines
            if idx == 0 {
                break;
            }
            idx -= 1;
        } else {
            // Stop at non-comment, non-empty line
            break;
        }
    }

    if comments.is_empty() {
        None
    } else {
        // Reverse to get original order (top to bottom)
        comments.reverse();
        Some(comments.join("\n"))
    }
}

/// Check if a line is a doc comment line (JSDoc style)
fn is_doc_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("/**")
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("*")
}

/// Helper to create symbols manually (without Parser.SyntaxNode)
/// Implementation of createSymbolManual logic
#[allow(clippy::too_many_arguments)] // Matches API for compatibility
pub(super) fn create_symbol_manual(
    base: &BaseExtractor,
    name: &str,
    kind: SymbolKind,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
    signature: Option<String>,
    documentation: Option<String>,
    metadata: Option<HashMap<String, Value>>,
) -> Symbol {
    use crate::extractors::base::{SymbolOptions, Visibility};

    let options = SymbolOptions {
        signature,
        doc_comment: documentation,
        visibility: Some(Visibility::Public),
        parent_id: None,
        metadata,
    };

    // Generate ID similar to standard approach
    let id = format!("{}:{}:{}", name, start_line, start_column);

    Symbol {
        id,
        name: name.to_string(),
        kind,
        language: base.language.clone(),
        file_path: base.file_path.clone(),
        start_line: start_line as u32,
        start_column: start_column as u32,
        end_line: end_line as u32,
        end_column: end_column as u32,
        start_byte: 0, // Not available without tree-sitter node
        end_byte: 0,   // Not available without tree-sitter node
        signature: options.signature,
        doc_comment: options.doc_comment,
        visibility: options.visibility,
        parent_id: options.parent_id,
        metadata: Some(options.metadata.unwrap_or_default()),
        semantic_group: None, // Vue components don't have cross-language groups yet
        confidence: None,     // Will be set during validation
        code_context: None,   // Will be populated during context extraction
        content_type: None,
    }
}
