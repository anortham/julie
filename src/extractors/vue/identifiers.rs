// Vue identifier extraction for LSP-quality find_references
//
// Parses the <script> section with JavaScript tree-sitter and extracts identifier usages
// Handles function calls, method calls, and member access patterns

use super::parsing::{VueSection, parse_vue_sfc};
use crate::extractors::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

/// Extract all identifier usages (function calls, member access, etc.)
/// Vue-specific: Parses <script> section with JavaScript tree-sitter
pub(super) fn extract_identifiers(base: &mut BaseExtractor, symbols: &[Symbol]) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Parse Vue SFC to extract script section
    if let Ok(sections) = parse_vue_sfc(&base.content.clone()) {
        for section in &sections {
            if section.section_type == "script" {
                // Parse script section with JavaScript tree-sitter
                if let Some(tree) = parse_script_section(section) {
                    // CRITICAL: We need to use the script content, not the full Vue SFC content
                    // Walk the JavaScript tree and extract identifiers
                    walk_tree_for_identifiers_with_content(
                        base,
                        tree.root_node(),
                        &symbol_map,
                        &section.content,
                        section.start_line,
                    );
                }
            }
        }
    }

    // Return the collected identifiers
    base.identifiers.clone()
}

/// Parse script section with JavaScript tree-sitter parser
fn parse_script_section(section: &VueSection) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();

    // Determine language based on lang attribute
    let lang = section.lang.as_deref().unwrap_or("js");

    // Use JavaScript/TypeScript tree-sitter parser
    let tree_sitter_lang = if lang == "ts" || lang == "typescript" {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_javascript::LANGUAGE.into()
    };

    parser.set_language(&tree_sitter_lang).ok()?;
    parser.parse(&section.content, None)
}

/// Recursively walk tree extracting identifiers from each node
/// With script content and line offset for correct text extraction
fn walk_tree_for_identifiers_with_content(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    script_content: &str,
    start_line_offset: usize,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node_with_content(
        base,
        node,
        symbol_map,
        script_content,
        start_line_offset,
    );

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers_with_content(
            base,
            child,
            symbol_map,
            script_content,
            start_line_offset,
        );
    }
}

/// Extract identifier from a single node based on its kind
/// Uses JavaScript tree-sitter node types: call_expression, member_expression
/// With script content for correct text extraction
fn extract_identifier_from_node_with_content(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    script_content: &str,
    start_line_offset: usize,
) {
    match node.kind() {
        // Function/method calls: foo(), bar.baz()
        "call_expression" => {
            // The function being called is in the "function" field
            if let Some(function_node) = node.child_by_field_name("function") {
                match function_node.kind() {
                    "identifier" => {
                        // Simple function call: foo()
                        let name = get_node_text_from_content(&function_node, script_content);
                        let containing_symbol_id =
                            find_containing_symbol_id(base, node, symbol_map);

                        create_identifier_with_offset(
                            base,
                            &function_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                            start_line_offset,
                        );
                    }
                    "member_expression" => {
                        // Method call: obj.method()
                        // Extract the rightmost identifier (the method name)
                        if let Some(property_node) = function_node.child_by_field_name("property") {
                            let name = get_node_text_from_content(&property_node, script_content);
                            let containing_symbol_id =
                                find_containing_symbol_id(base, node, symbol_map);

                            create_identifier_with_offset(
                                base,
                                &property_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                                start_line_offset,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        // Member access: object.field
        "member_expression" => {
            // Only extract if it's NOT part of a call_expression
            // (we handle those in the call_expression case above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    return; // Skip - handled by call_expression
                }
            }

            // Extract the rightmost identifier (the property name)
            if let Some(property_node) = node.child_by_field_name("property") {
                let name = get_node_text_from_content(&property_node, script_content);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                create_identifier_with_offset(
                    base,
                    &property_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                    start_line_offset,
                );
            }
        }

        _ => {
            // Skip other node types for now
        }
    }
}

/// Get node text from script content (not full Vue SFC)
fn get_node_text_from_content(node: &Node, content: &str) -> String {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    content[start_byte..end_byte].to_string()
}

/// Create identifier with line offset adjustment
/// Uses content swap to correctly extract from script section
fn create_identifier_with_offset(
    base: &mut BaseExtractor,
    node: &Node,
    name: String,
    kind: IdentifierKind,
    containing_symbol_id: Option<String>,
    line_offset: usize,
) {
    // Temporarily swap the base content with script content
    // This allows create_identifier to extract text correctly using script-relative byte positions
    let original_content = std::mem::take(&mut base.content);

    // Get script content from the original content
    if let Ok(sections) = parse_vue_sfc(&original_content) {
        for section in &sections {
            if section.section_type == "script" {
                // Set base.content to script content temporarily
                base.content = section.content.clone();

                // Create identifier with script content
                let mut identifier = base.create_identifier(node, name, kind, containing_symbol_id);

                // Adjust line numbers for the script section offset
                identifier.start_line += line_offset as u32;
                identifier.end_line += line_offset as u32;

                // Restore original content
                base.content = original_content;

                // Replace the last identifier (which was just added by create_identifier)
                if let Some(last) = base.identifiers.last_mut() {
                    *last = identifier;
                }

                return;
            }
        }
    }

    // Restore original content if we didn't find script section
    base.content = original_content;
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE, not all files
    // Bug was: searching all symbols in DB caused wrong file symbols to match
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == base.file_path)
        .map(|&s| s.clone())
        .collect();

    base.find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
