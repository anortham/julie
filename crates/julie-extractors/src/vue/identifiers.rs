// Vue identifier extraction for LSP-quality find_references
//
// Parses the <script> section with JavaScript tree-sitter and extracts identifier usages
// Handles function calls, method calls, and member access patterns

use super::parsing::{VueSection, parse_vue_sfc};
use crate::base::{
    BaseExtractor, EmbeddedSpanOffset, Identifier, IdentifierKind, Literal, LiteralKind,
    NormalizedSpan, Symbol, SymbolKind, TypeArgument,
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
fn get_node_text_from_content(node: &Node, content: &str) -> String {
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

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3)
// ============================================================================
//
// Vue parses the <script> section with its own tree-sitter pass, so the call
// nodes carry byte offsets into `script_content` (the section text), NOT the
// host SFC in `base.content`. The base helpers `decode_string_literal` /
// `record_literal` read from `base.content` and build script-relative spans, so
// they would read the wrong bytes and mislocate the literal. This leg therefore
// decodes from `script_content` and remaps the span to the host file via
// `offset` — exactly like `extract_vue_type_arguments` does for type args. The
// decode is a faithful port of `BaseExtractor::decode_string_literal`, differing
// only in reading text from `script_content`.

/// Capture string-literal arguments of a Vue `<script>` `call_expression` as
/// `Literal` records. Config-free: `carrier` is the verbatim callee text; the
/// URL/SQL classification and the carrier gate run later in the `src/` pipeline.
/// `arg_position` is counted over the full (named) argument list.
fn record_vue_call_arg_literals(
    base: &mut BaseExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    script_content: &str,
    offset: EmbeddedSpanOffset,
) {
    let Some(function_node) = call_node.child_by_field_name("function") else {
        return;
    };
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = vue_callee_text(function_node, script_content);
    let containing_span = offset.apply(NormalizedSpan::from_node(&call_node));
    let containing_symbol_id = find_containing_symbol_id_for_span(base, containing_span, symbol_map);

    let mut cursor = args_node.walk();
    for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
        let Some(text) = decode_vue_string_literal(&arg, script_content) else {
            continue;
        };
        let span = offset.apply(NormalizedSpan::from_node(&arg));
        let literal = Literal {
            id: base.generate_id_for_span(&text, &span),
            literal_text: text,
            kind: LiteralKind::Other,
            carrier: carrier.clone(),
            arg_position: pos as u32,
            language: base.language.clone(),
            file_path: base.file_path.clone(),
            start_line: span.start_line,
            start_column: span.start_column,
            end_line: span.end_line,
            end_column: span.end_column,
            start_byte: span.start_byte,
            end_byte: span.end_byte,
            containing_symbol_id: containing_symbol_id.clone(),
            confidence: 1.0,
        };
        base.literals.push(literal);
    }
}

/// Derive the verbatim callee text used as a literal's `carrier`, reading from
/// `script_content`. Plain `identifier` → its text (`fetch`); `member_expression`
/// → the `object.property` join (`axios.get`) so dotted client APIs match config.
fn vue_callee_text(function_node: Node, script_content: &str) -> Option<String> {
    match function_node.kind() {
        "identifier" => Some(get_node_text_from_content(&function_node, script_content)),
        "member_expression" => {
            let object = function_node
                .child_by_field_name("object")
                .map(|n| get_node_text_from_content(&n, script_content));
            let property = function_node
                .child_by_field_name("property")
                .map(|n| get_node_text_from_content(&n, script_content));
            match (object, property) {
                (Some(o), Some(p)) => Some(format!("{o}.{p}")),
                (None, Some(p)) => Some(p),
                _ => None,
            }
        }
        _ => {
            let text = get_node_text_from_content(&function_node, script_content);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

/// Port of [`BaseExtractor::decode_string_literal`] that reads node text from
/// `script_content` (the Vue script-section bytes the parse nodes index into).
/// Strips delimiters and replaces interpolation/substitution holes with `{}`.
/// Returns `None` for non-string nodes.
fn decode_vue_string_literal(node: &Node, content: &str) -> Option<String> {
    let kind = node.kind();
    if !(kind.contains("string") || kind.contains("char")) {
        return None;
    }
    let mut out = String::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let ck = child.kind();
        if ck == "interpolation" || ck == "template_substitution" || ck.ends_with("_substitution") {
            out.push_str("{}");
        } else if ck.contains("content")
            || ck.contains("fragment")
            || ck.contains("text")
            || ck == "escape_sequence"
        {
            out.push_str(&get_node_text_from_content(&child, content));
        }
        // else: delimiter marker (start/end/quote/brace/encoding) — skip.
    }
    if out.is_empty() {
        out = strip_vue_string_delimiters(&get_node_text_from_content(node, content));
    }
    Some(out)
}

/// Port of `base::extractor::strip_string_delimiters` (private there). Strips one
/// matching outer delimiter pair, skipping any leading string prefix.
fn strip_vue_string_delimiters(raw: &str) -> String {
    let s = raw.trim();
    let Some(qpos) = s.find(['"', '\'', '`']) else {
        return s.to_string();
    };
    let s = &s[qpos..];
    for q in ["\"\"\"", "'''"] {
        if s.len() >= 2 * q.len() && s.starts_with(q) && s.ends_with(q) {
            return s[q.len()..s.len() - q.len()].to_string();
        }
    }
    let count = s.chars().count();
    if count >= 2 {
        let first = s.chars().next();
        let last = s.chars().last();
        if let (Some(f), Some(l)) = (first, last) {
            if f == l && matches!(f, '"' | '\'' | '`') {
                return s.chars().skip(1).take(count - 2).collect();
            }
        }
    }
    s.to_string()
}

// ============================================================================
// Type-argument capture helpers (Miller bridge Phase 2)
// ============================================================================

/// Recursively extract ordered, nested type arguments from a TypeScript `type_arguments`
/// node, reading type names from `script_content` (the script section text, whose byte
/// offsets the parse nodes use) rather than from the full Vue SFC content in `base`.
fn extract_vue_type_arguments<'a>(
    arg_list_node: Node<'a>,
    script_content: &str,
) -> Vec<TypeArgument> {
    let mut arguments = Vec::new();
    let mut ordinal: u32 = 0;
    let children: Vec<Node<'a>> = arg_list_node.children(&mut arg_list_node.walk()).collect();
    for child in children {
        if !child.is_named() {
            continue; // skip < , >
        }
        match child.kind() {
            "generic_type" => {
                // Nested generic: e.g. `Array<User>` inside outer type_arguments.
                let name = child
                    .child_by_field_name("name")
                    .map(|n| get_node_text_from_content(&n, script_content))
                    .unwrap_or_else(|| get_node_text_from_content(&child, script_content));
                // Find the nested type_arguments child of this generic_type
                let nested_children: Vec<Node<'a>> = child.children(&mut child.walk()).collect();
                let nested_arg_list = nested_children
                    .iter()
                    .find(|c| c.kind() == "type_arguments")
                    .copied();
                let sub_args = match nested_arg_list {
                    Some(nested) => extract_vue_type_arguments(nested, script_content),
                    None => Vec::new(),
                };
                arguments.push(TypeArgument {
                    ordinal,
                    type_name: name,
                    children: sub_args,
                });
                ordinal += 1;
            }
            _ => {
                // Leaf type: predefined_type ("string"), type_identifier ("User"), etc.
                let name = get_node_text_from_content(&child, script_content);
                arguments.push(TypeArgument {
                    ordinal,
                    type_name: name,
                    children: Vec::new(),
                });
                ordinal += 1;
            }
        }
    }
    arguments
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
