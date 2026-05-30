//! PowerShell identifier extraction for LSP-quality find_references
//! Extracts identifier usages (function calls, member access, etc.)

use crate::base::{
    BaseExtractor, Identifier, IdentifierKind, Symbol, SymbolKind, extract_type_arguments,
};
use std::collections::HashMap;
use tree_sitter::Node;

use super::helpers::find_command_name_node;

/// Extract all identifier usages (function calls, member access, etc.)
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    base.identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node(base, node, symbol_map);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(base, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // PowerShell commands and cmdlet calls: Get-Process, Write-Host, etc.
        "command" | "command_expression" => {
            // Extract command name
            if let Some(name_node) = find_command_name_node(node) {
                let name = base.get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &name_node,
                    name.clone(),
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
                // Miller bridge Phase 3b: capture string-literal command args.
                record_command_arg_literals(base, node, &name, symbol_map);
            }
        }

        // PowerShell invocation expressions: function calls
        "invocation_expression" => {
            // Extract function name from invocation
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "command_name" || child.kind() == "identifier" {
                    let name = base.get_node_text(&child);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                    base.create_identifier(
                        &child,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                    break;
                } else if child.kind() == "member_access_expression" {
                    // For member access in invocation (e.g., $obj.Method())
                    // Extract the rightmost identifier (the method name)
                    let text = base.get_node_text(&child);
                    if let Some(last_dot_pos) = text.rfind('.') {
                        if last_dot_pos + 1 < text.len() {
                            let method_name = &text[last_dot_pos + 1..];
                            let containing_symbol_id =
                                find_containing_symbol_id(base, node, symbol_map);

                            base.create_identifier(
                                &child,
                                method_name.to_string(),
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                    break;
                }
            }
        }

        // PowerShell member access: $object.Property, $this.Name
        // PowerShell tree-sitter uses "member_access" (not "member_access_expression")
        "member_access" => {
            // Only extract if it's NOT part of an invocation_expression or command
            // (we handle method calls separately)
            if let Some(parent) = node.parent() {
                if parent.kind() == "invocation_expression" || parent.kind() == "command" {
                    return; // Skip - handled by invocation/command
                }
            }

            // Extract member name from member_access node
            // Structure: member_access -> member_name -> simple_name
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "member_name" {
                    // Get the simple_name child
                    let mut name_cursor = child.walk();
                    for name_child in child.children(&mut name_cursor) {
                        if name_child.kind() == "simple_name" {
                            let member_name = base.get_node_text(&name_child);
                            let containing_symbol_id =
                                find_containing_symbol_id(base, node, symbol_map);

                            base.create_identifier(
                                &name_child,
                                member_name,
                                IdentifierKind::MemberAccess,
                                containing_symbol_id,
                            );
                            return;
                        }
                    }
                }
            }
        }

        // PowerShell .NET generic type: [List[User]], [Dictionary[string, int]]
        // The grammar uses `generic_type_name` for the base name and
        // `generic_type_arguments` (sibling in the parent `type_spec`) for the args.
        "generic_type_name" => {
            // Skip nested generics: a generic_type_name whose parent type_spec lives
            // inside generic_type_arguments is itself a nested argument — its args
            // ride along as children of the enclosing usage, not as a separate row.
            let is_nested = node
                .parent() // type_spec
                .and_then(|p| p.parent()) // generic_type_arguments
                .map(|gp| gp.kind() == "generic_type_arguments")
                .unwrap_or(false);
            if is_nested {
                return;
            }

            // Extract the base type name from the `type_name` child.
            let mut cursor = node.walk();
            let Some(type_name_node) = node.named_children(&mut cursor).next() else {
                return;
            };
            let name = base.get_node_text(&type_name_node);
            let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
            let identifier = base.create_identifier(
                &type_name_node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );

            // Find the `generic_type_arguments` sibling in the parent `type_spec`.
            let Some(type_spec) = node.parent() else {
                return;
            };
            let mut spec_cursor = type_spec.walk();
            let Some(arg_list) = type_spec
                .named_children(&mut spec_cursor)
                .find(|c| c.kind() == "generic_type_arguments")
            else {
                return;
            };

            let arguments = extract_type_arguments(base, arg_list, decompose_powershell_type_arg);
            base.record_type_arguments(&identifier, arguments);
        }

        _ => {
            // Skip other node types for now
        }
    }
}

// ============================================================================
// Type-argument capture helpers (Miller bridge Phase 2)
// ============================================================================

/// `TypeArgDecomposer` for PowerShell: maps a child of a `generic_type_arguments`
/// node to its applied argument.
///
/// Each child of `generic_type_arguments` is a `type_spec`. A `type_spec` for a
/// leaf argument contains a `type_name` child; one for a nested generic contains
/// a `generic_type_name` + `generic_type_arguments`. Unnamed nodes (commas,
/// brackets `[`, `]`) return `None` and are skipped.
fn decompose_powershell_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip commas, brackets
    }
    if node.kind() != "type_spec" {
        return None; // only type_spec children are arguments
    }
    let mut cursor1 = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor1).collect();

    // Check for nested generic: generic_type_name + generic_type_arguments
    if let Some(&gtn) = children.iter().find(|c| c.kind() == "generic_type_name") {
        // Extract the type_name text from the generic_type_name child.
        let mut gtn_cursor = gtn.walk();
        let name = gtn
            .named_children(&mut gtn_cursor)
            .next()
            .map(|n| base.get_node_text(&n))
            .unwrap_or_else(|| base.get_node_text(&gtn));
        // The nested arg list is the generic_type_arguments sibling.
        let nested = children
            .iter()
            .find(|c| c.kind() == "generic_type_arguments")
            .copied();
        Some((name, nested))
    } else {
        // Leaf argument: extract name from type_name child.
        let type_name = children.iter().find(|c| c.kind() == "type_name")?;
        Some((base.get_node_text(type_name), None))
    }
}

// ============================================================================
// String-literal command-argument capture (Miller bridge Phase 3b)
// ============================================================================

/// Capture string-literal arguments of a PowerShell `command` node.
///
/// PowerShell is a COMMAND grammar, not `call_expression`: a `command` has a
/// `command_name` field and a `command_elements` field holding the argument list
/// (whitespace separators, `command_parameter` flags like `-Uri`/`-Query`, and
/// value expressions). The carrier is the cmdlet name itself
/// (`Invoke-RestMethod`, `Invoke-WebRequest`, `Invoke-Sqlcmd`, …); this is
/// config-free — `kind` is `Other` and the `src/` carrier gate reclassifies and
/// drops, with `languages/powershell.toml` deciding which cmdlet names survive.
///
/// The string value is nested
/// (`array_literal_expression > unary_expression > string_literal`), so each
/// argument subtree is walked and the outermost string-bearing node is decoded.
/// `arg_position` counts over the non-separator `command_elements` children, so a
/// `-Uri`/`-Query` flag occupies a position and the quoted value that follows
/// reports the next index (a positional `Invoke-WebRequest "url"` reports 0).
fn record_command_arg_literals(
    base: &mut BaseExtractor,
    command_node: Node,
    carrier: &str,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(elements) = command_node.child_by_field_name("command_elements") else {
        // command_expression / parameter-less forms have no element list.
        return;
    };
    let containing_symbol_id = find_containing_symbol_id(base, command_node, symbol_map);
    let mut position = 0u32;
    let mut cursor = elements.walk();
    for child in elements.children(&mut cursor) {
        // Whitespace separators are not arguments; skip without advancing.
        if child.kind() == "command_argument_sep" {
            continue;
        }
        let mut strings = Vec::new();
        collect_string_literals(child, &mut strings);
        for string_node in strings {
            if let Some(text) = decode_ps_string_literal(base, &string_node) {
                base.record_literal(
                    &string_node,
                    text,
                    Some(carrier.to_string()),
                    position,
                    containing_symbol_id.clone(),
                );
            }
        }
        position += 1;
    }
}

/// Collect the outermost string-bearing nodes within an argument subtree.
///
/// PowerShell wraps a string value as
/// `string_literal > {expandable,verbatim}_string_literal`; recursion stops at
/// the first string-bearing node (kind contains `string`) so the wrapper and its
/// inner variant are never double-counted.
fn collect_string_literals<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    if node.kind().contains("string") {
        out.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_string_literals(child, out);
    }
}

/// Decode a PowerShell string-literal node to its static shape, normalizing
/// `$var` / `$(...)` interpolation inside expandable strings to `{}`.
///
/// Unlike most grammars, PowerShell tokenizes an expandable string's static text
/// as ANONYMOUS bytes (only the `variable` / `sub_expression` holes are child
/// nodes), so the shared `decode_string_literal` named-child walk cannot rebuild
/// the surrounding text — it would collapse to bare `{}` and lose the URL. Here we
/// reconstruct from the raw bytes, blanking each outermost interpolation hole, so
/// `"https://api/users/$id"` → `https://api/users/{}` consistent with bash/Swift/
/// Dart. Verbatim (single-quoted) and plain strings have no holes and fall back to
/// the shared delimiter-stripping decoder.
fn decode_ps_string_literal(base: &BaseExtractor, node: &Node) -> Option<String> {
    let mut holes: Vec<(usize, usize)> = Vec::new();
    collect_ps_interpolation_holes(*node, &mut holes);
    if holes.is_empty() {
        return base.decode_string_literal(node);
    }
    holes.sort_by_key(|&(start, _)| start);
    let raw = base.get_node_text(node);
    let base_off = node.start_byte();
    let mut out = String::new();
    let mut cursor = 0usize;
    for (hole_start, hole_end) in holes {
        let rel_start = hole_start.saturating_sub(base_off);
        let rel_end = hole_end.saturating_sub(base_off);
        // Guard against any overlap (nested holes already excluded) / bad ranges.
        if rel_start >= cursor && rel_end <= raw.len() && rel_start <= rel_end {
            out.push_str(&raw[cursor..rel_start]);
            out.push_str("{}");
            cursor = rel_end;
        }
    }
    out.push_str(&raw[cursor..]);
    Some(strip_ps_string_delimiters(&out))
}

/// Collect the OUTERMOST interpolation holes (`variable`, `sub_expression`) within
/// a PowerShell string subtree, as absolute `(start_byte, end_byte)` ranges. A
/// `$(...)` sub-expression is recorded whole (its nested `$vars` are not descended
/// into) so the entire expression collapses to a single `{}`.
fn collect_ps_interpolation_holes(node: Node, out: &mut Vec<(usize, usize)>) {
    match node.kind() {
        "variable" | "sub_expression" => {
            out.push((node.start_byte(), node.end_byte()));
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_ps_interpolation_holes(child, out);
            }
        }
    }
}

/// Strip the outer quote pair from a reconstructed PowerShell string, including
/// here-string (`@"…"@` / `@'…'@`) and the common `"…"` / `'…'` forms.
fn strip_ps_string_delimiters(s: &str) -> String {
    if s.len() >= 4
        && ((s.starts_with("@\"") && s.ends_with("\"@"))
            || (s.starts_with("@'") && s.ends_with("'@")))
    {
        return s[2..s.len() - 2].to_string();
    }
    let bytes = s.as_bytes();
    if s.len() >= 2 {
        let first = bytes[0];
        if (first == b'"' || first == b'\'') && bytes[bytes.len() - 1] == first {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
/// POWERSHELL-SPECIFIC: Skip command symbols to avoid matching command calls with themselves
fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    base.find_containing_symbol_from_map_filtered(&node, symbol_map, |symbol| {
        matches!(
            symbol.kind,
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Class
        ) && symbol.start_line < symbol.end_line
    })
    .map(|s| s.id.clone())
}
