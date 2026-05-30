// R Identifier Extraction
// Extracts identifier usages: function calls, variable references, member access

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use crate::r::RExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages from R code
pub(super) fn extract_identifiers(
    extractor: &mut RExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base.identifiers.clone()
}

/// Recursively walk the tree and extract identifiers
fn walk_tree_for_identifiers(
    extractor: &mut RExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from current node
    extract_identifier_from_node(extractor, node, symbol_map);

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(extractor, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    extractor: &mut RExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function calls: foo(), library(dplyr), lapply(x, f)
        "call" => {
            if let Some(function_node) = node.child(0) {
                let name = match function_node.kind() {
                    "identifier" => extractor.base.get_node_text(&function_node),
                    "namespace_operator" => {
                        // Handle package::function syntax
                        if let Some(function_child) = function_node.child(2) {
                            extractor.base.get_node_text(&function_child)
                        } else {
                            extractor.base.get_node_text(&function_node)
                        }
                    }
                    "extract_operator" => {
                        // Handle object$method() syntax
                        if let Some(member) = function_node.child(2) {
                            extractor.base.get_node_text(&member)
                        } else {
                            extractor.base.get_node_text(&function_node)
                        }
                    }
                    _ => extractor.base.get_node_text(&function_node),
                };

                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base.create_identifier(
                    &function_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // Phase 3b: capture string-literal call-arguments config-free; the
            // carrier classification + bloat gate run later in the src/ pipeline.
            record_r_call_arg_literals(extractor, node, symbol_map);
        }

        // Member access: object$property, object@slot
        "extract_operator" => {
            // Skip if this is part of a call expression (handled above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" {
                    return;
                }
            }

            // Extract the member being accessed
            if let Some(member_node) = node.child(2) {
                let name = extractor.base.get_node_text(&member_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base.create_identifier(
                    &member_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // Variable references
        "identifier" => {
            // Only create variable reference if not already handled
            if let Some(parent) = node.parent() {
                match parent.kind() {
                    // Skip if this is the function being called
                    "call" if parent.child(0).map(|c| c.id()) == Some(node.id()) => {
                        return;
                    }
                    // Skip if this is in an extract operator (handled separately)
                    "extract_operator" => {
                        return;
                    }
                    // Skip if this is in a namespace operator
                    "namespace_operator" => {
                        return;
                    }
                    // Skip if this is a parameter name
                    "parameter" => {
                        return;
                    }
                    // Check if this is the left side of an assignment
                    "binary_operator" => {
                        if let Some(operator) = parent.child(1) {
                            let op_text = extractor.base.get_node_text(&operator);
                            // Skip if this is the target of an assignment
                            if (op_text == "<-" || op_text == "=" || op_text == "<<-")
                                && parent.child(0).map(|c| c.id()) == Some(node.id())
                            {
                                return;
                            }
                            if (op_text == "->" || op_text == "->>")
                                && parent.child(2).map(|c| c.id()) == Some(node.id())
                            {
                                return;
                            }
                        }
                        // This is a variable being used in a binary expression
                        let name = extractor.base.get_node_text(&node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base.create_identifier(
                            &node,
                            name,
                            IdentifierKind::VariableRef,
                            containing_symbol_id,
                        );
                    }
                    _ => {
                        // This is likely a variable reference
                        let name = extractor.base.get_node_text(&node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base.create_identifier(
                            &node,
                            name,
                            IdentifierKind::VariableRef,
                            containing_symbol_id,
                        );
                    }
                }
            }
        }

        _ => {
            // Skip other node types
        }
    }
}

/// Find the containing symbol ID for a node using byte-range containment
/// Uses BaseExtractor::find_containing_symbol for accurate position-based matching
fn find_containing_symbol_id(
    extractor: &RExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    extractor
        .base
        .find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3b)
// ============================================================================

/// Capture string-literal arguments of an R `call` as `Literal` records.
///
/// Config-free: `carrier` is the function name — a plain `identifier`
/// (`dbGetQuery`, imported `POST`), or the `package.function` join for a
/// `namespace_operator` (`httr::GET` → `httr.GET`) / `extract_operator`
/// (`con$query` → `con.query`). Qualifying the namespace form lets the gate
/// match `httr.GET`/`httr.HEAD` exactly without a bare `get`/`head` config
/// (which would flood base R's `get()`/`head()`). `kind` stays `Other`; the
/// `src/` carrier gate sets the authoritative kind and drops non-carrier
/// literals. `arg_position` counts over the full argument list; every R arg is
/// an `argument` node whose literal lives in its `value` field.
fn record_r_call_arg_literals(
    extractor: &mut RExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(function_node) = call_node.child_by_field_name("function") else {
        return;
    };
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = r_carrier(&extractor.base, function_node);
    let containing_symbol_id = find_containing_symbol_id(extractor, call_node, symbol_map);

    let mut cursor = args_node.walk();
    for (pos, arg) in args_node
        .children_by_field_name("argument", &mut cursor)
        .enumerate()
    {
        // Every R argument wraps its expression in a `value` field.
        let value = arg.child_by_field_name("value").unwrap_or(arg);
        if let Some(text) = extractor.base.decode_string_literal(&value) {
            extractor.base.record_literal(
                &value,
                text,
                carrier.clone(),
                pos as u32,
                containing_symbol_id.clone(),
            );
        }
    }
}

/// Derive an R call's carrier from its `function` node.
///
/// Plain `identifier` → its text (`dbGetQuery`, imported `POST`).
/// `namespace_operator` (`httr::GET`) and `extract_operator` (`con$query`) → the
/// `lhs.rhs` join (`httr.GET`, `con.query`). The qualified namespace form lets
/// the gate match `httr.GET`/`httr.HEAD` exactly without a bare `get`/`head`
/// config that would flood base R's `get()`/`head()`; verbs that are not base R
/// names (`POST`, `PUT`, …) stay bare in config and match both the imported and
/// `httr::POST` forms via the gate's last-segment rule.
fn r_carrier(base: &BaseExtractor, function_node: Node) -> Option<String> {
    match function_node.kind() {
        "identifier" => Some(base.get_node_text(&function_node)),
        "namespace_operator" | "extract_operator" => {
            let lhs = function_node
                .child_by_field_name("lhs")
                .map(|n| base.get_node_text(&n));
            let rhs = function_node
                .child_by_field_name("rhs")
                .map(|n| base.get_node_text(&n));
            match (lhs, rhs) {
                (Some(l), Some(r)) => Some(format!("{l}.{r}")),
                (None, Some(r)) => Some(r),
                _ => None,
            }
        }
        _ => {
            let text = base.get_node_text(&function_node);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}
