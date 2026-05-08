// Vue identifier extraction for LSP-quality find_references
//
// Parses the <script> section with JavaScript tree-sitter and extracts identifier usages
// Handles function calls, method calls, and member access patterns

use super::parsing::{VueSection, parse_vue_sfc};
use crate::base::{
    BaseExtractor, EmbeddedSpanOffset, Identifier, IdentifierKind, NormalizedSpan, Symbol,
    SymbolKind,
};
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
                    let byte_offset = section_byte_offset(&base.content, section.start_line);
                    let Some(offset) =
                        EmbeddedSpanOffset::from_host_byte(&base.content, byte_offset as usize)
                    else {
                        continue;
                    };

                    // CRITICAL: We need to use the script content, not the full Vue SFC content
                    // for node text, then remap spans back to the host Vue file.
                    walk_tree_for_identifiers_with_content(
                        base,
                        tree.root_node(),
                        &symbol_map,
                        &section.content,
                        offset,
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
    offset: EmbeddedSpanOffset,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node_with_content(base, node, symbol_map, script_content, offset);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers_with_content(base, child, symbol_map, script_content, offset);
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
    offset: EmbeddedSpanOffset,
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

                        create_identifier_with_offset(
                            base,
                            &function_node,
                            &node,
                            name,
                            IdentifierKind::Call,
                            symbol_map,
                            offset,
                        );
                    }
                    "member_expression" => {
                        // Method call: obj.method()
                        // Extract the rightmost identifier (the method name)
                        if let Some(property_node) = function_node.child_by_field_name("property") {
                            let name = get_node_text_from_content(&property_node, script_content);

                            create_identifier_with_offset(
                                base,
                                &property_node,
                                &node,
                                name,
                                IdentifierKind::Call,
                                symbol_map,
                                offset,
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

                create_identifier_with_offset(
                    base,
                    &property_node,
                    &node,
                    name,
                    IdentifierKind::MemberAccess,
                    symbol_map,
                    offset,
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

/// Create an identifier from a script-section node and remap it to the host Vue file.
fn create_identifier_with_offset(
    base: &mut BaseExtractor,
    node: &Node,
    containing_node: &Node,
    name: String,
    kind: IdentifierKind,
    symbol_map: &HashMap<String, &Symbol>,
    offset: EmbeddedSpanOffset,
) {
    let span = offset.apply(NormalizedSpan::from_node(node));
    let containing_span = offset.apply(NormalizedSpan::from_node(containing_node));
    let containing_symbol_id =
        find_containing_symbol_id_for_span(base, containing_span, symbol_map);
    let code_context = base.extract_code_context(
        span.start_line.saturating_sub(1) as usize,
        span.end_line.saturating_sub(1) as usize,
    );

    let identifier = Identifier {
        id: base.generate_id_for_span(&name, &span),
        name,
        kind,
        language: base.language.clone(),
        file_path: base.file_path.clone(),
        start_line: span.start_line,
        start_column: span.start_column,
        end_line: span.end_line,
        end_column: span.end_column,
        start_byte: span.start_byte,
        end_byte: span.end_byte,
        containing_symbol_id,
        target_symbol_id: None,
        confidence: 1.0,
        code_context,
    };

    base.identifiers.push(identifier);
}

fn section_byte_offset(content: &str, start_line: usize) -> u32 {
    content
        .split_inclusive('\n')
        .take(start_line)
        .map(str::len)
        .sum::<usize>() as u32
}

fn find_containing_symbol_id_for_span(
    base: &BaseExtractor,
    span: NormalizedSpan,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let mut containing_symbols: Vec<&Symbol> = symbol_map
        .values()
        .copied()
        .filter(|symbol| symbol.file_path == base.file_path && symbol_contains_span(symbol, span))
        .collect();

    if containing_symbols.is_empty() {
        return None;
    }

    containing_symbols.sort_by(|a, b| {
        let priority_a = symbol_containment_priority(&a.kind);
        let priority_b = symbol_containment_priority(&b.kind);
        if priority_a != priority_b {
            return priority_a.cmp(&priority_b);
        }

        let size_a = a.end_byte - a.start_byte;
        let size_b = b.end_byte - b.start_byte;
        size_a.cmp(&size_b)
    });

    Some(containing_symbols[0].id.clone())
}

fn symbol_contains_span(symbol: &Symbol, span: NormalizedSpan) -> bool {
    let pos_line = span.start_line;
    let pos_column = span.start_column;
    let line_contains = symbol.start_line <= pos_line && symbol.end_line >= pos_line;
    let col_contains = if pos_line == symbol.start_line && pos_line == symbol.end_line {
        symbol.start_column <= pos_column && symbol.end_column >= pos_column
    } else if pos_line == symbol.start_line {
        symbol.start_column <= pos_column
    } else if pos_line == symbol.end_line {
        symbol.end_column >= pos_column
    } else {
        true
    };

    line_contains && col_contains
}

fn symbol_containment_priority(kind: &SymbolKind) -> u32 {
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => 1,
        SymbolKind::Class | SymbolKind::Interface => 2,
        SymbolKind::Namespace => 3,
        SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Property => 10,
        _ => 5,
    }
}
