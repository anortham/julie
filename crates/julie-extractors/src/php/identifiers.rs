// PHP Extractor - Identifier extraction (function calls, member access, type usage)

use super::PhpExtractor;
use crate::base::{BaseExtractor, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract identifier from a single node based on its kind
pub(super) fn extract_identifier_from_node(
    extractor: &mut PhpExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Direct function calls: print_r(), array_map()
        "function_call_expression" => {
            // The function field contains the function being called
            if let Some(function_node) = node.child_by_field_name("function") {
                let name = extractor.get_base().get_node_text(&function_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.get_base_mut().create_identifier(
                    &function_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // Phase 3b: capture string-literal call-arguments config-free.
            record_php_call_arg_literals(extractor, node, symbol_map);
        }

        // Method calls: $this->add(), $obj->method()
        "member_call_expression" => {
            // Extract the method name from the name field
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = extractor.get_base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.get_base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // Phase 3b: capture string-literal call-arguments config-free.
            record_php_call_arg_literals(extractor, node, symbol_map);
        }

        // Static method calls: Http::get(), DB::select(), Model::where()
        "scoped_call_expression" => {
            // Extract the method name from the name field
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = extractor.get_base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.get_base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // Phase 3b: capture string-literal call-arguments config-free.
            record_php_call_arg_literals(extractor, node, symbol_map);
        }

        // Member access: $obj->property
        "member_access_expression" => {
            // Skip if parent is a call expression (handled above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "function_call_expression"
                    || parent.kind() == "member_call_expression"
                {
                    return; // Skip - handled by call expressions
                }
            }

            // Extract the member name (rightmost identifier)
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = extractor.get_base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.get_base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // Type annotations: parameter types, return types, property types.
        // PHP tree-sitter uses `named_type` for class/interface type references
        // (e.g., Request, Response, App) and `primitive_type` for builtins
        // (e.g., int, string, void). We only create type_usage for named_type.
        //
        // named_type appears in:
        //   - Parameter types:  function handle(Request $req)
        //   - Return types:     function handle(): Response
        //   - Property types:   public Request $request
        //   - Union types:      string|Request  (named_type inside union_type)
        //   - Optional types:   ?Request        (named_type inside optional_type)
        "named_type" => {
            let name = extractor.get_base().get_node_text(&node);

            // Skip single-letter type params (rare in PHP, but possible)
            if name.len() <= 1 {
                return;
            }

            let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

            extractor.get_base_mut().create_identifier(
                &node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
        }

        // instanceof expressions: $obj instanceof Router
        // PHP tree-sitter represents this as binary_expression with an
        // "instanceof" anonymous child. The type name after instanceof is
        // a `name` node.
        "binary_expression" => {
            let mut cursor = node.walk();
            let mut found_instanceof = false;
            for child in node.children(&mut cursor) {
                if found_instanceof && child.is_named() {
                    let name = extractor.get_base().get_node_text(&child);

                    // Skip single-letter names
                    if name.len() <= 1 {
                        return;
                    }

                    let containing_symbol_id =
                        find_containing_symbol_id(extractor, node, symbol_map);

                    extractor.get_base_mut().create_identifier(
                        &child,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                    return;
                }
                if child.kind() == "instanceof" {
                    found_instanceof = true;
                }
            }
        }

        _ => {
            // Skip other node types for now
        }
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &PhpExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    extractor
        .get_base()
        .find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3b)
// ============================================================================

/// Capture string-literal arguments of a PHP call (`function_call_expression`,
/// `member_call_expression`, `scoped_call_expression`) as `Literal` records.
///
/// Config-free: `carrier` is the verbatim callee — the bare function `name` for a
/// function call, the `object.name` join for a method call (`$client.get`), or the
/// `scope.name` join for a static call (`Http.get`). `kind` stays `Other`; the
/// `src/` carrier gate sets the authoritative kind and drops non-carrier literals.
/// `arg_position` counts over the full argument list. Named-argument labels are
/// skipped via `php_argument_value` (the value is the argument's last named child).
fn record_php_call_arg_literals(
    extractor: &mut PhpExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = php_carrier(extractor.get_base(), call_node);
    let containing_symbol_id = find_containing_symbol_id(extractor, call_node, symbol_map);

    let mut cursor = args_node.walk();
    for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
        let Some(value) = php_argument_value(arg) else {
            continue;
        };
        if let Some(text) = extractor.get_base().decode_string_literal(&value) {
            extractor.get_base_mut().record_literal(
                &value,
                text,
                carrier.clone(),
                pos as u32,
                containing_symbol_id.clone(),
            );
        }
    }
}

/// Resolve a PHP `argument` node to its value node.
///
/// A positional argument wraps its value directly (`(argument (encapsed_string …))`);
/// a named argument carries a `name:` label first (`foo(label: "v")`), so the value
/// is the *last* named child. Reference (`&$x`) and spread (`...$a`) modifiers ride
/// as anonymous tokens, so the last named child is still the value expression.
fn php_argument_value(arg: Node) -> Option<Node> {
    if arg.kind() != "argument" {
        return None;
    }
    let mut cursor = arg.walk();
    arg.named_children(&mut cursor).last()
}

/// Derive a PHP call's carrier from its callee shape.
///
/// `function_call_expression` → the bare `function` text (`mysqli_query`).
/// `member_call_expression` → `object.name` (`$pdo.query`) so a local-variable
/// receiver still matches a bare method config (`query`, `prepare`) via the gate's
/// last-segment rule. `scoped_call_expression` → `scope.name` (`Http.get`) so a
/// dotted facade config matches exactly.
fn php_carrier(base: &BaseExtractor, call_node: Node) -> Option<String> {
    match call_node.kind() {
        "function_call_expression" => call_node
            .child_by_field_name("function")
            .map(|n| base.get_node_text(&n)),
        "member_call_expression" => {
            let object = call_node
                .child_by_field_name("object")
                .map(|n| base.get_node_text(&n));
            let name = call_node
                .child_by_field_name("name")
                .map(|n| base.get_node_text(&n));
            match (object, name) {
                (Some(o), Some(n)) => Some(format!("{o}.{n}")),
                (None, Some(n)) => Some(n),
                _ => None,
            }
        }
        "scoped_call_expression" => {
            let scope = call_node
                .child_by_field_name("scope")
                .map(|n| base.get_node_text(&n));
            let name = call_node
                .child_by_field_name("name")
                .map(|n| base.get_node_text(&n));
            match (scope, name) {
                (Some(s), Some(n)) => Some(format!("{s}.{n}")),
                (None, Some(n)) => Some(n),
                _ => None,
            }
        }
        _ => None,
    }
}
