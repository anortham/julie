/// Identifier extraction for LSP-quality find_references
/// Tracks function calls, member access, and other identifier usages
use super::PythonExtractor;
use super::type_arguments::record_outermost_python_type_arguments;
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub fn extract_identifiers(
    extractor: &mut PythonExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base_mut().identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    extractor: &mut PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node(extractor, node, symbol_map);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(extractor, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    extractor: &mut PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function/method calls: foo(), bar.baz()
        // Python uses "call" node type
        "call" => {
            // The function being called is in the "function" field
            if let Some(function_node) = node.child_by_field_name("function") {
                match function_node.kind() {
                    "identifier" => {
                        // Simple function call: foo()
                        let name = extractor.base_mut().get_node_text(&function_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            &function_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    "attribute" => {
                        // Member call: object.method()
                        // Extract the rightmost identifier (the method name)
                        if let Some(attr_node) = function_node.child_by_field_name("attribute") {
                            let name = extractor.base_mut().get_node_text(&attr_node);
                            let containing_symbol_id =
                                find_containing_symbol_id(extractor, node, symbol_map);

                            extractor.base_mut().create_identifier(
                                &attr_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                    _ => {
                        // Other cases like subscript expressions
                        // Skip for now
                    }
                }
            }
            // Phase 3: capture string-literal call-arguments config-free; the
            // carrier classification + bloat gate run later in the src/ pipeline.
            record_python_call_arg_literals(extractor, node, symbol_map);
        }

        // Member access: object.property
        // Python uses "attribute" node type
        "attribute" => {
            if is_python_type_usage_node(node) {
                if let Some(attr_node) = node.child_by_field_name("attribute") {
                    let name = extractor.base_mut().get_node_text(&attr_node);
                    if !is_python_builtin_type(&name) {
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        let identifier = extractor.base_mut().create_identifier(
                            &attr_node,
                            name,
                            IdentifierKind::TypeUsage,
                            containing_symbol_id,
                        );
                        // `node` is the whole `a.B` attribute expression; if it is
                        // the `value` field of a subscript (e.g. `typing.Optional[X]`)
                        // we record the ordered type arguments against the identifier.
                        record_outermost_python_type_arguments(extractor, node, &identifier);
                    }
                }
                return;
            }

            // Only extract if it's NOT part of a call
            // (we handle those in the call case above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" {
                    // Check if this attribute is the function being called
                    if let Some(function_node) = parent.child_by_field_name("function") {
                        if function_node.id() == node.id() {
                            return; // Skip - handled by call
                        }
                    }
                }
            }

            // Extract the attribute name
            if let Some(attr_node) = node.child_by_field_name("attribute") {
                let name = extractor.base_mut().get_node_text(&attr_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &attr_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        "identifier" => {
            if is_python_type_usage_identifier(node) {
                let name = extractor.base_mut().get_node_text(&node);
                if !is_python_builtin_type(&name) {
                    let containing_symbol_id =
                        find_containing_symbol_id(extractor, node, symbol_map);

                    let identifier = extractor.base_mut().create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                    // If this identifier is the `value` of an outermost subscript
                    // (e.g. `Optional` in `Optional[User]`), record the ordered
                    // type arguments.  Nested generics are skipped here — their
                    // args ride along as `children` of the enclosing usage.
                    record_outermost_python_type_arguments(extractor, node, &identifier);
                }
            }
        }

        _ => {}
    }
}

fn is_python_type_usage_identifier(node: Node) -> bool {
    if let Some(parent) = node.parent() {
        if parent.kind() == "attribute" {
            return false;
        }
    }

    is_python_type_usage_node(node)
}

fn is_python_type_usage_node(node: Node) -> bool {
    if is_python_declaration_name(node) {
        return false;
    }

    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "type" | "generic_type" | "union_type" => return true,
            "call" | "return_statement" | "block" | "module" => return false,
            // `argument_list` is a stopping node for regular call arguments.
            // Exception: class superclasses sit in an `argument_list` whose parent
            // is `class_definition`. Allow that context so heritage subscripts like
            // `class Repo(Mapping[str, int])` are captured as type-usage positions.
            "argument_list" => {
                let parent_is_class = parent
                    .parent()
                    .map(|gp| gp.kind() == "class_definition")
                    .unwrap_or(false);
                return parent_is_class;
            }
            _ => {}
        }

        current = parent;
    }

    false
}

fn is_python_declaration_name(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if let Some(name_node) = parent.child_by_field_name("name") {
        return name_node.id() == node.id()
            && matches!(
                parent.kind(),
                "class_definition" | "function_definition" | "type_alias_statement"
            );
    }

    false
}

fn is_python_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "bytes"
            | "complex"
            | "dict"
            | "float"
            | "frozenset"
            | "int"
            | "list"
            | "None"
            | "object"
            | "set"
            | "str"
            | "tuple"
            | "type"
    )
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let base = extractor.base();
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3)
// ============================================================================

/// Capture string-literal arguments of a Python `call` as `Literal` records.
///
/// Config-free: `carrier` is the verbatim callee text; the URL/SQL
/// classification and the carrier gate run later in the `src/` pipeline.
/// Records one literal per string-like argument, with `arg_position` counted
/// over the full (named) argument list. Keyword arguments (`url="..."`) descend
/// to their `value` so `requests.get(url="/api")` is captured too.
fn record_python_call_arg_literals(
    extractor: &mut PythonExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(function_node) = call_node.child_by_field_name("function") else {
        return;
    };
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = python_carrier(extractor.base(), function_node);
    let containing_symbol_id = find_containing_symbol_id(extractor, call_node, symbol_map);

    let mut cursor = args_node.walk();
    for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
        // Keyword args (`name=value`) hold the literal in their `value` field.
        let value = if arg.kind() == "keyword_argument" {
            arg.child_by_field_name("value")
        } else {
            Some(arg)
        };
        if let Some(value) = value {
            if let Some(text) = extractor.base().decode_string_literal(&value) {
                extractor.base_mut().record_literal(
                    &value,
                    text,
                    carrier.clone(),
                    pos as u32,
                    containing_symbol_id.clone(),
                );
            }
        }
    }
}

/// Derive a Python call's carrier from its callee.
///
/// Plain `identifier` → its text (`open`). `attribute` (`requests.get`,
/// `cursor.execute`) → the `object.attribute` join so dotted client APIs match
/// config (`requests.get`) and local-variable receivers still match a bare
/// method config (`execute`) via the gate's last-segment rule.
fn python_carrier(base: &BaseExtractor, function_node: Node) -> Option<String> {
    match function_node.kind() {
        "identifier" => Some(base.get_node_text(&function_node)),
        "attribute" => {
            let object = function_node
                .child_by_field_name("object")
                .map(|n| base.get_node_text(&n));
            let attribute = function_node
                .child_by_field_name("attribute")
                .map(|n| base.get_node_text(&n));
            match (object, attribute) {
                (Some(o), Some(a)) => Some(format!("{o}.{a}")),
                (None, Some(a)) => Some(a),
                _ => None,
            }
        }
        _ => {
            let text = base.get_node_text(&function_node);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}
