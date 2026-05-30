// Vue identifier extraction for LSP-quality find_references
//
// Parses the <script> section with JavaScript tree-sitter and extracts identifier usages
// Handles function calls, method calls, and member access patterns

mod literals;
mod type_arguments;

use super::parsing::{VueSection, parse_vue_sfc};
use crate::base::{
    BaseExtractor, EmbeddedSpanOffset, Identifier, IdentifierKind, NormalizedSpan, Symbol,
    SymbolKind,
};
use literals::record_vue_call_arg_literals;
use std::collections::HashMap;
use tree_sitter::{Node, Parser};
use type_arguments::extract_vue_type_arguments;

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
            // Phase 3: capture string-literal call-arguments (config-free; the
            // carrier classification + gate happen in the src/ pipeline). Vue
            // parses the <script> with its own byte offsets, so this path decodes
            // from `script_content` and remaps spans to the host SFC via `offset`.
            record_vue_call_arg_literals(base, node, symbol_map, script_content, offset);
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

        // Heritage clause: `class Comp extends Base<User>`  (lang="ts" only).
        // TypeScript's grammar places the base-class name in an expression-context `value`
        // field as an `identifier` (not `type_identifier`), so the `type_identifier` arm
        // below does NOT fire for it.  A separate `type_arguments` field carries `<…>`.
        "extends_clause" => {
            let Some(value_node) = node.child_by_field_name("value") else {
                return;
            };
            let Some((name_node, name)) =
                terminal_identifier_from_content(value_node, script_content)
            else {
                return;
            };
            let identifier = create_identifier_with_offset(
                base,
                &name_node,
                &node,
                name,
                IdentifierKind::TypeUsage,
                symbol_map,
                offset,
            );
            if let Some(arg_list) = node.child_by_field_name("type_arguments") {
                let arguments = extract_vue_type_arguments(arg_list, script_content);
                base.record_type_arguments(&identifier, arguments);
            }
        }

        // Construction: `new Map<string, User>()`  (lang="ts" only).
        // The constructor name is an `identifier` (expression context), not a `type_identifier`,
        // so the arm below does NOT fire for it.  The `type_arguments` node sits as a named
        // child of `new_expression` alongside the constructor and argument list.
        "new_expression" => {
            let constructor_node = {
                let by_field = node.child_by_field_name("constructor");
                if by_field.is_some() {
                    by_field
                } else {
                    let mut cursor = node.walk();
                    node.named_children(&mut cursor)
                        .find(|c| !matches!(c.kind(), "arguments" | "type_arguments"))
                }
            };
            let Some(constructor_node) = constructor_node else {
                return;
            };
            let Some((name_node, name)) =
                terminal_identifier_from_content(constructor_node, script_content)
            else {
                return;
            };
            let identifier = create_identifier_with_offset(
                base,
                &name_node,
                &node,
                name,
                IdentifierKind::Call,
                symbol_map,
                offset,
            );
            let maybe_type_args = {
                let mut cursor = node.walk();
                node.named_children(&mut cursor)
                    .find(|c| c.kind() == "type_arguments")
            };
            if let Some(arg_list) = maybe_type_args {
                let arguments = extract_vue_type_arguments(arg_list, script_content);
                base.record_type_arguments(&identifier, arguments);
            }
        }

        // TypeScript type references in type positions: `const x: Foo`, `field: Foo<Bar>`.
        // Only fires when the script section uses lang="ts" — the JS grammar does not emit
        // `type_identifier` nodes. We filter declaration names and noise types as in the
        // standalone TypeScript extractor, then hook outermost `generic_type` parents.
        "type_identifier" => {
            if is_ts_type_declaration_name(&node) {
                return;
            }
            let name = get_node_text_from_content(&node, script_content);
            if is_ts_noise_type(&name) {
                return;
            }
            // Detect if this type_identifier is the `name` field of an outermost generic_type.
            // "Outermost" = the generic_type's parent is NOT itself `type_arguments`.
            let opt_arg_list = {
                if let Some(parent) = node.parent() {
                    if parent.kind() == "generic_type"
                        && !parent
                            .parent()
                            .map(|p| p.kind() == "type_arguments")
                            .unwrap_or(false)
                    {
                        let children: Vec<Node> = parent.children(&mut parent.walk()).collect();
                        children.into_iter().find(|c| c.kind() == "type_arguments")
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            let identifier = create_identifier_with_offset(
                base,
                &node,
                &node,
                name,
                IdentifierKind::TypeUsage,
                symbol_map,
                offset,
            );
            if let Some(arg_list) = opt_arg_list {
                let arguments = extract_vue_type_arguments(arg_list, script_content);
                base.record_type_arguments(&identifier, arguments);
            }
        }

        _ => {
            // Skip other node types for now
        }
    }
}

/// Resolve the terminal `identifier` / `type_identifier` / `property_identifier` inside
/// `node`, returning `(leaf_node, name_text)`.  Used by `extends_clause` and
/// `new_expression` arms where the base type can be a plain identifier or a member
/// expression (`Foo.Bar`).  Text is read from `script_content` (script-section bytes).
fn terminal_identifier_from_content<'a>(
    node: Node<'a>,
    script_content: &str,
) -> Option<(Node<'a>, String)> {
    match node.kind() {
        "identifier" | "property_identifier" | "type_identifier" => {
            let name = get_node_text_from_content(&node, script_content);
            Some((node, name))
        }
        "member_expression" => {
            let property = node.child_by_field_name("property")?;
            terminal_identifier_from_content(property, script_content)
        }
        _ => None,
    }
}

/// Get node text from script content (not full Vue SFC)
pub(super) fn get_node_text_from_content(node: &Node, content: &str) -> String {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();
    content[start_byte..end_byte].to_string()
}

/// Create an identifier from a script-section node and remap it to the host Vue file.
/// Returns the created identifier so callers can attach type-argument usages to it.
fn create_identifier_with_offset(
    base: &mut BaseExtractor,
    node: &Node,
    containing_node: &Node,
    name: String,
    kind: IdentifierKind,
    symbol_map: &HashMap<String, &Symbol>,
    offset: EmbeddedSpanOffset,
) -> Identifier {
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

    base.identifiers.push(identifier.clone());
    identifier
}

fn section_byte_offset(content: &str, start_line: usize) -> u32 {
    content
        .split_inclusive('\n')
        .take(start_line)
        .map(str::len)
        .sum::<usize>() as u32
}

pub(super) fn find_containing_symbol_id_for_span(
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
        if size_a != size_b {
            return size_a.cmp(&size_b);
        }

        // HashMap iteration order is non-deterministic. Without an id-level
        // tiebreaker, two symbols with identical priority and size would be
        // selected arbitrarily across runs and produce flaky
        // containing_symbol_id assignments.
        a.id.cmp(&b.id)
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

/// Check if a `type_identifier` node is a TypeScript declaration name (not a reference).
///
/// TypeScript `type_identifier` appears as the `name` field of:
/// - `interface_declaration` → `interface Foo {}`
/// - `type_alias_declaration` → `type Foo = ...`
/// - `class_declaration` / `abstract_class_declaration` → `class Foo {}`
/// - `type_parameter` → `<T extends Base>` (T is a declaration)
/// - `mapped_type_clause` → `[K in keyof T]` (K is a declaration)
fn is_ts_type_declaration_name(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        if let Some(name_node) = parent.child_by_field_name("name") {
            if name_node.id() == node.id() {
                return matches!(
                    parent.kind(),
                    "interface_declaration"
                        | "type_alias_declaration"
                        | "class_declaration"
                        | "abstract_class_declaration"
                        | "type_parameter"
                        | "mapped_type_clause"
                );
            }
        }
    }
    false
}

/// Returns true for TypeScript types too common to produce useful type-usage signals:
/// single-letter generic type parameters (T, K, V…) and TS compiler utility types.
fn is_ts_noise_type(name: &str) -> bool {
    if name.len() == 1
        && name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_uppercase())
    {
        return true;
    }
    matches!(
        name,
        "Readonly"
            | "Partial"
            | "Required"
            | "Pick"
            | "Omit"
            | "Exclude"
            | "Extract"
            | "NonNullable"
            | "ReturnType"
            | "InstanceType"
            | "Parameters"
            | "ConstructorParameters"
            | "Awaited"
            | "Record"
    )
}
