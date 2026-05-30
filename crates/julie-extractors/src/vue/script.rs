// Vue script section symbol extraction
//
// Responsible for extracting Vue component options from the <script> section
// Handles data(), methods, computed, props, and function definitions

use super::helpers::{
    COMPUTED_OBJECT_RE, DATA_FUNCTION_RE, FUNCTION_DEF_RE, METHODS_OBJECT_RE, PROPS_OBJECT_RE,
};
pub(super) use super::manual_symbols::create_symbol_manual;
use super::parsing::VueSection;
use crate::base::{BaseExtractor, Symbol, SymbolKind};
use crate::test_detection::is_test_symbol;
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

/// Extract symbols from script section
pub(super) fn extract_script_symbols(base: &BaseExtractor, section: &VueSection) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = section.content.lines().collect();

    if let Some(tree) = parse_script_section(section) {
        extract_options_api_symbols(base, section, tree.root_node(), &mut symbols);
    }

    if !symbols.is_empty() {
        return symbols;
    }

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

                // Test detection (Category 3: name + path, empty annotation keys)
                let metadata =
                    if is_test_symbol("vue", name, &base.file_path, &SymbolKind::Method, &[], None)
                    {
                        let mut m = HashMap::new();
                        m.insert("is_test".to_string(), Value::Bool(true));
                        Some(m)
                    } else {
                        None
                    };

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
                    metadata,
                ));
            }
        }
    }

    symbols
}

fn parse_script_section(section: &VueSection) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    let lang = section.lang.as_deref().unwrap_or("js");
    let tree_sitter_lang = if lang == "ts" || lang == "typescript" {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_javascript::LANGUAGE.into()
    };

    parser.set_language(&tree_sitter_lang).ok()?;
    parser.parse(&section.content, None)
}

fn extract_options_api_symbols(
    base: &BaseExtractor,
    section: &VueSection,
    node: Node,
    symbols: &mut Vec<Symbol>,
) {
    if node.kind() == "pair" {
        extract_options_pair(base, section, node, symbols);
    } else if node.kind() == "method_definition" {
        extract_options_method(base, section, node, symbols);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_options_api_symbols(base, section, child, symbols);
    }
}

fn extract_options_method(
    base: &BaseExtractor,
    section: &VueSection,
    node: Node,
    symbols: &mut Vec<Symbol>,
) {
    let Some(name_node) = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("key"))
    else {
        return;
    };
    let name = node_text(&name_node, &section.content);
    if name == "data" {
        push_node_symbol(
            base,
            section,
            &name,
            SymbolKind::Function,
            name_node,
            symbols,
        );
        extract_data_return_symbols(base, section, node, symbols);
    }
}

fn extract_options_pair(
    base: &BaseExtractor,
    section: &VueSection,
    node: Node,
    symbols: &mut Vec<Symbol>,
) {
    let Some(key_node) = node.child_by_field_name("key") else {
        return;
    };
    let key_text = node_text(&key_node, &section.content);
    let key = key_text.trim_matches(['\'', '"']);
    let value_node = node.child_by_field_name("value");

    match key {
        "props" => {
            push_node_symbol(base, section, key, SymbolKind::Property, key_node, symbols);
            if let Some(value) = value_node {
                extract_object_member_symbols(base, section, value, SymbolKind::Property, symbols);
            }
        }
        "emits" => {
            push_node_symbol(base, section, key, SymbolKind::Property, key_node, symbols);
            if let Some(value) = value_node {
                extract_emit_symbols(base, section, value, symbols);
            }
        }
        "data" => {
            push_node_symbol(base, section, key, SymbolKind::Function, key_node, symbols);
            extract_data_return_symbols(base, section, node, symbols);
        }
        "computed" => {
            push_node_symbol(base, section, key, SymbolKind::Property, key_node, symbols);
            if let Some(value) = value_node {
                extract_object_member_symbols(base, section, value, SymbolKind::Method, symbols);
            }
        }
        "methods" => {
            push_node_symbol(base, section, key, SymbolKind::Property, key_node, symbols);
            if let Some(value) = value_node {
                extract_object_member_symbols(base, section, value, SymbolKind::Method, symbols);
            }
        }
        _ => {}
    }
}

fn extract_object_member_symbols(
    base: &BaseExtractor,
    section: &VueSection,
    node: Node,
    kind: SymbolKind,
    symbols: &mut Vec<Symbol>,
) {
    if matches!(node.kind(), "pair" | "method_definition") {
        if let Some(key_node) = node
            .child_by_field_name("key")
            .or_else(|| node.child_by_field_name("name"))
        {
            let name = node_text(&key_node, &section.content)
                .trim_matches(['\'', '"'])
                .to_string();
            push_node_symbol(base, section, &name, kind, key_node, symbols);
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_object_member_symbols(base, section, child, kind.clone(), symbols);
    }
}

fn extract_emit_symbols(
    base: &BaseExtractor,
    section: &VueSection,
    node: Node,
    symbols: &mut Vec<Symbol>,
) {
    if node.kind() == "string" {
        let name = node_text(&node, &section.content)
            .trim_matches(['\'', '"'])
            .to_string();
        if !name.is_empty() {
            push_node_symbol(base, section, &name, SymbolKind::Event, node, symbols);
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_emit_symbols(base, section, child, symbols);
    }
}

fn extract_data_return_symbols(
    base: &BaseExtractor,
    section: &VueSection,
    node: Node,
    symbols: &mut Vec<Symbol>,
) {
    if node.kind() == "return_statement" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "object" {
                extract_object_member_symbols(base, section, child, SymbolKind::Property, symbols);
            }
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_data_return_symbols(base, section, child, symbols);
    }
}

fn push_node_symbol(
    base: &BaseExtractor,
    section: &VueSection,
    name: &str,
    kind: SymbolKind,
    node: Node,
    symbols: &mut Vec<Symbol>,
) {
    let start_line = section.start_line + node.start_position().row + 1;
    let mut start_col = node.start_position().column + 1;
    let end_line = section.start_line + node.end_position().row + 1;
    let mut end_col = node.end_position().column + 1;
    let text = node_text(&node, &section.content);
    if text.len() >= name.len() + 2
        && text.trim_matches(['\'', '"']) == name
        && matches!(text.as_bytes().first(), Some(b'\'' | b'"'))
        && matches!(text.as_bytes().last(), Some(b'\'' | b'"'))
    {
        start_col += 1;
        end_col = end_col.saturating_sub(1);
    }
    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), Value::String(format!("{:?}", kind)));

    if is_test_symbol("vue", name, &base.file_path, &kind, &[], None) {
        metadata.insert("is_test".to_string(), Value::Bool(true));
    }

    // Extract JSDoc comment from a preceding comment node in the tree-sitter tree.
    // The key node (e.g. "methods") is a child of the pair/method_definition; the
    // comment is a prev_named_sibling of that parent node inside the options object.
    let doc_comment = extract_node_doc_comment(&node, &section.content);

    symbols.push(create_symbol_manual(
        base,
        name,
        kind,
        start_line,
        start_col,
        end_line,
        end_col,
        Some(name.to_string()),
        doc_comment,
        Some(metadata),
    ));
}

/// Walk backward through preceding named siblings of the node's parent to collect
/// consecutive `comment` nodes. Returns the concatenated comment text or `None`.
fn extract_node_doc_comment(node: &Node, content: &str) -> Option<String> {
    // The node is a key/name node; its parent is the pair or method_definition
    // whose preceding siblings may be comments.
    let parent = node.parent()?;
    let mut current = parent.prev_named_sibling();
    let mut comments: Vec<String> = Vec::new();

    while let Some(sibling) = current {
        if sibling.kind() == "comment" {
            comments.push(node_text(&sibling, content));
            current = sibling.prev_named_sibling();
        } else {
            break;
        }
    }

    if comments.is_empty() {
        return None;
    }

    // Reverse so the topmost comment comes first
    comments.reverse();
    Some(comments.join("\n"))
}

fn node_text(node: &Node, content: &str) -> String {
    content
        .get(node.start_byte()..node.end_byte())
        .unwrap_or_default()
        .to_string()
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
