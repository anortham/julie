/// Rust identifier extraction for LSP-quality reference tracking
/// - Function calls
/// - Variable references
/// - Member access expressions
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, SymbolKind};
use crate::rust::RustExtractor;
use tree_sitter::Tree;

/// Extract all identifiers (references/usages) for LSP-quality reference tracking
///
/// Phase 1 - basic extraction. We extract:
/// - Function calls (call_expression)
/// - Variable references (identifier nodes in certain contexts)
///
/// Identifiers are stored unresolved (target_symbol_id = None) and resolved
/// on-demand during queries for optimal incremental update performance.
pub(super) fn extract_identifiers(
    extractor: &mut RustExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    let file_path = extractor.get_base_mut().file_path.clone();
    let containing_symbols = ContainingSymbolIndex::new(symbols, &file_path);

    walk_tree_for_identifiers(extractor, tree.root_node(), &containing_symbols);

    // Return extracted identifiers from base extractor
    extractor.get_base_mut().identifiers.clone()
}

/// Walk the tree extracting identifiers
fn walk_tree_for_identifiers(
    extractor: &mut RustExtractor,
    node: tree_sitter::Node,
    containing_symbols: &ContainingSymbolIndex<'_>,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node(extractor, node, containing_symbols);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(extractor, child, containing_symbols);
    }
}

/// Extract identifier from a single node
fn extract_identifier_from_node(
    extractor: &mut RustExtractor,
    node: tree_sitter::Node,
    containing_symbols: &ContainingSymbolIndex<'_>,
) {
    match node.kind() {
        // Function calls: foo(), bar.baz(), foo::<T>() (turbofish)
        "call_expression" => {
            if let Some(func_node) = node.child_by_field_name("function") {
                // Unwrap turbofish: generic_function { function: ..., type_arguments: ... }
                // e.g. `foo::<String>()` or `self.collect::<Vec<u8>>()`
                let (inner_func, turbofish_arg_list) = if func_node.kind() == "generic_function" {
                    let inner = func_node
                        .child_by_field_name("function")
                        .unwrap_or(func_node);
                    let args = func_node.child_by_field_name("type_arguments");
                    (inner, args)
                } else {
                    (func_node, None)
                };

                let name = {
                    let base = extractor.get_base_mut();
                    if inner_func.kind() == "field_expression" {
                        // Method call: extract just the field name
                        if let Some(field_node) = inner_func.child_by_field_name("field") {
                            base.get_node_text(&field_node)
                        } else {
                            base.get_node_text(&inner_func)
                        }
                    } else if inner_func.kind() == "scoped_identifier" {
                        // Qualified call: crate::module::function() → extract "function"
                        if let Some(name_node) = inner_func.child_by_field_name("name") {
                            base.get_node_text(&name_node)
                        } else {
                            base.get_node_text(&inner_func)
                        }
                    } else {
                        // Regular function call (bare identifier)
                        base.get_node_text(&inner_func)
                    }
                };

                let identifier_node = if inner_func.kind() == "field_expression" {
                    if let Some(field_node) = inner_func.child_by_field_name("field") {
                        field_node
                    } else {
                        inner_func
                    }
                } else if inner_func.kind() == "scoped_identifier" {
                    if let Some(name_node) = inner_func.child_by_field_name("name") {
                        name_node
                    } else {
                        inner_func
                    }
                } else {
                    inner_func
                };

                // Find containing symbol (which function/method contains this call)
                let containing_symbol_id = find_containing_symbol_id(node, containing_symbols);

                // Create identifier for this function call
                let identifier = {
                    let base = extractor.get_base_mut();
                    base.create_identifier(
                        &identifier_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    )
                };

                // Record turbofish type arguments (e.g. `foo::<String>()` → (0, "String"))
                if let Some(arg_list) = turbofish_arg_list {
                    let base = extractor.get_base_mut();
                    let arguments = crate::base::extract_type_arguments(
                        base,
                        arg_list,
                        decompose_rust_type_arg,
                    );
                    base.record_type_arguments(&identifier, arguments);
                }
            }
        }

        // Variable/field references in specific contexts
        // We're conservative - only extract clear variable usages, not all identifiers
        "field_expression" => {
            // Skip if this field_expression is the function of a call_expression
            // (e.g., self.method() - we want "method" as Call, not MemberAccess)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    if let Some(func_child) = parent.child_by_field_name("function") {
                        if func_child.id() == node.id() {
                            // This field_expression IS the function being called, skip it
                            return;
                        }
                    }
                }
            }

            // object.field - extract the field name (not part of a call)
            if let Some(field_node) = node.child_by_field_name("field") {
                let name = {
                    let base = extractor.get_base_mut();
                    base.get_node_text(&field_node)
                };
                let containing_symbol_id = find_containing_symbol_id(node, containing_symbols);

                {
                    let base = extractor.get_base_mut();
                    base.create_identifier(
                        &field_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }
        }

        "scoped_identifier" | "scoped_type_identifier" => {
            if is_inside_call_function(node) {
                return;
            }

            if let Some(name_node) = node.child_by_field_name("name") {
                let name = {
                    let base = extractor.get_base_mut();
                    base.get_node_text(&name_node)
                };
                let containing_symbol_id = find_containing_symbol_id(node, containing_symbols);

                let identifier = {
                    let base = extractor.get_base_mut();
                    base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    )
                };
                // Record type args when the scoped type is the `type` field of a
                // generic_type: e.g. `std::io::Error<T>` → node.parent() == generic_type
                record_outermost_rust_type_arguments_for_scoped(extractor, node, &identifier);
            }
        }

        "type_identifier" => {
            if !is_rust_declaration_type_name(node) {
                let name = {
                    let base = extractor.get_base_mut();
                    base.get_node_text(&node)
                };
                let containing_symbol_id = find_containing_symbol_id(node, containing_symbols);

                let identifier = {
                    let base = extractor.get_base_mut();
                    base.create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    )
                };
                // Record type args when this identifier is the base of an outermost
                // generic: e.g. `Vec` in `Vec<String>` → parent is generic_type
                record_outermost_rust_type_arguments(extractor, node, &identifier);
            }
        }

        _ => {}
    }
}

fn is_rust_declaration_type_name(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if let Some(name_node) = parent.child_by_field_name("name") {
        if name_node.id() == node.id() {
            return matches!(
                parent.kind(),
                "struct_item"
                    | "enum_item"
                    | "union_item"
                    | "trait_item"
                    | "type_item"
                    | "impl_item"
                    | "type_parameter"
            );
        }
    }

    matches!(parent.kind(), "type_parameters")
}

fn is_inside_call_function(node: tree_sitter::Node) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "call_expression" {
            if let Some(function_node) = parent.child_by_field_name("function") {
                return node.start_byte() >= function_node.start_byte()
                    && node.end_byte() <= function_node.end_byte();
            }
        }
        current = parent;
    }
    false
}

/// If `name_node` (a `type_identifier`) is the direct `type` child of an
/// *outermost* `generic_type` use site (e.g. the `Vec` of `Vec<String>`),
/// record that generic's ordered/nested applied type arguments against
/// `identifier`.
///
/// "Outermost" means the `generic_type` is NOT itself inside another
/// `type_arguments` list — nested generics ride along as `children` of the
/// enclosing usage and are never double-counted as separate rows.
fn record_outermost_rust_type_arguments(
    extractor: &mut RustExtractor,
    name_node: tree_sitter::Node,
    identifier: &Identifier,
) {
    let Some(parent) = name_node.parent() else {
        return;
    };
    // Both `generic_type` (type-position) and `generic_type_with_turbofish` (struct-literal
    // construction, e.g. `Repo::<String> { .. }`) carry a `type_arguments` field with the
    // same shape — handle both uniformly.
    if parent.kind() != "generic_type" && parent.kind() != "generic_type_with_turbofish" {
        return;
    }
    // Skip if this generic is itself nested inside another type_arguments.
    if parent
        .parent()
        .map(|p| p.kind() == "type_arguments")
        .unwrap_or(false)
    {
        return;
    }
    let Some(arg_list) = parent.child_by_field_name("type_arguments") else {
        return;
    };
    let base = extractor.get_base_mut();
    let arguments = crate::base::extract_type_arguments(base, arg_list, decompose_rust_type_arg);
    base.record_type_arguments(identifier, arguments);
}

/// Like `record_outermost_rust_type_arguments` but the anchor is a
/// `scoped_identifier` or `scoped_type_identifier` node — the node itself
/// (not a name child inside it) is the `type` field of the parent
/// `generic_type`.
fn record_outermost_rust_type_arguments_for_scoped(
    extractor: &mut RustExtractor,
    scoped_node: tree_sitter::Node,
    identifier: &Identifier,
) {
    let Some(parent) = scoped_node.parent() else {
        return;
    };
    // Mirror the type_identifier variant: accept both generic_type and
    // generic_type_with_turbofish for the same reasons.
    if parent.kind() != "generic_type" && parent.kind() != "generic_type_with_turbofish" {
        return;
    }
    if parent
        .parent()
        .map(|p| p.kind() == "type_arguments")
        .unwrap_or(false)
    {
        return;
    }
    let Some(arg_list) = parent.child_by_field_name("type_arguments") else {
        return;
    };
    let base = extractor.get_base_mut();
    let arguments = crate::base::extract_type_arguments(base, arg_list, decompose_rust_type_arg);
    base.record_type_arguments(identifier, arguments);
}

/// `TypeArgDecomposer` for Rust: maps a named child of a `type_arguments`
/// list to its applied argument.  Returns `None` to skip punctuation (`<`,
/// `,`, `>`) and lifetime parameters (`'a`, `'static`).  For a nested
/// `generic_type` returns the type name plus its inner `type_arguments` to
/// recurse into; for every other named type node (primitive types, references,
/// arrays, etc.) returns its source text as a leaf.
fn decompose_rust_type_arg<'a>(
    base: &BaseExtractor,
    node: tree_sitter::Node<'a>,
) -> Option<(String, Option<tree_sitter::Node<'a>>)> {
    if !node.is_named() {
        return None; // skip punctuation: < , >
    }
    match node.kind() {
        "lifetime" => None, // skip 'a, 'static, etc.
        "generic_type" => {
            // Nested generic such as `Vec<u8>` inside `HashMap<String, Vec<u8>>`
            let name = node
                .child_by_field_name("type")
                .map(|t| base.get_node_text(&t))
                .unwrap_or_else(|| base.get_node_text(&node));
            let nested = node.child_by_field_name("type_arguments");
            Some((name, nested))
        }
        _ => Some((base.get_node_text(&node), None)),
    }
}

/// Find the ID of the symbol that contains this node
fn find_containing_symbol_id(
    node: tree_sitter::Node,
    containing_symbols: &ContainingSymbolIndex<'_>,
) -> Option<String> {
    containing_symbols
        .find(node)
        .map(|symbol| symbol.id.clone())
}

struct ContainingSymbolIndex<'a> {
    symbols: Vec<IndexedSymbol<'a>>,
}

struct IndexedSymbol<'a> {
    symbol: &'a Symbol,
    priority: u32,
    size: u32,
}

impl<'a> ContainingSymbolIndex<'a> {
    fn new(symbols: &'a [Symbol], file_path: &str) -> Self {
        let mut symbols: Vec<IndexedSymbol<'a>> = symbols
            .iter()
            .filter(|symbol| symbol.file_path == file_path)
            .map(|symbol| IndexedSymbol {
                symbol,
                priority: symbol_priority(&symbol.kind),
                size: symbol.end_byte.saturating_sub(symbol.start_byte),
            })
            .collect();
        symbols.sort_by(|left, right| {
            left.symbol
                .start_line
                .cmp(&right.symbol.start_line)
                .then_with(|| left.symbol.start_column.cmp(&right.symbol.start_column))
        });
        Self { symbols }
    }

    fn find(&self, node: tree_sitter::Node) -> Option<&'a Symbol> {
        let position = node.start_position();
        let pos_line = (position.row + 1) as u32;
        let pos_column = position.column as u32;
        let mut best: Option<&IndexedSymbol<'a>> = None;

        for candidate in &self.symbols {
            if candidate.symbol.start_line > pos_line {
                break;
            }

            if !symbol_contains_position(candidate.symbol, pos_line, pos_column) {
                continue;
            }

            if best.is_none_or(|current| is_better_containing_symbol(candidate, current)) {
                best = Some(candidate);
            }
        }

        best.map(|candidate| candidate.symbol)
    }
}

fn symbol_contains_position(symbol: &Symbol, pos_line: u32, pos_column: u32) -> bool {
    let line_contains = symbol.start_line <= pos_line && symbol.end_line >= pos_line;
    if !line_contains {
        return false;
    }

    if pos_line == symbol.start_line && pos_line == symbol.end_line {
        symbol.start_column <= pos_column && symbol.end_column >= pos_column
    } else if pos_line == symbol.start_line {
        symbol.start_column <= pos_column
    } else if pos_line == symbol.end_line {
        symbol.end_column >= pos_column
    } else {
        true
    }
}

fn is_better_containing_symbol(candidate: &IndexedSymbol<'_>, current: &IndexedSymbol<'_>) -> bool {
    candidate.priority < current.priority
        || (candidate.priority == current.priority && candidate.size < current.size)
}

fn symbol_priority(kind: &SymbolKind) -> u32 {
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => 1,
        SymbolKind::Class | SymbolKind::Interface => 2,
        SymbolKind::Namespace => 3,
        SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Property => 10,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::SymbolKind;
    use tree_sitter::Parser;

    #[test]
    fn containing_symbol_index_keeps_existing_priority_and_smallest_span_rules() {
        let source = "fn caller() {\n    helper();\n}\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("failed to set Rust language");
        let tree = parser.parse(source, None).expect("failed to parse Rust");
        let call = find_first_node_kind(tree.root_node(), "call_expression")
            .expect("call expression should parse");

        let symbols = vec![
            test_symbol(
                "module",
                SymbolKind::Namespace,
                "test.rs",
                1,
                0,
                3,
                1,
                0,
                28,
            ),
            test_symbol(
                "wide_fn",
                SymbolKind::Function,
                "test.rs",
                1,
                0,
                3,
                1,
                0,
                28,
            ),
            test_symbol(
                "narrow_fn",
                SymbolKind::Function,
                "test.rs",
                2,
                4,
                2,
                13,
                call.start_byte() as u32,
                call.end_byte() as u32,
            ),
            test_symbol(
                "other_file",
                SymbolKind::Function,
                "other.rs",
                2,
                4,
                2,
                13,
                call.start_byte() as u32,
                call.end_byte() as u32,
            ),
        ];

        let index = ContainingSymbolIndex::new(&symbols, "test.rs");

        assert_eq!(
            index.find(call).map(|symbol| symbol.id.as_str()),
            Some("narrow_fn")
        );
    }

    fn find_first_node_kind<'a>(
        node: tree_sitter::Node<'a>,
        kind: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_first_node_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }

    fn test_symbol(
        id: &str,
        kind: SymbolKind,
        file_path: &str,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        start_byte: u32,
        end_byte: u32,
    ) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line,
            start_column,
            end_line,
            end_column,
            start_byte,
            end_byte,
            body_span: None,
            body_hash: None,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            annotations: Vec::new(),
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }
}
