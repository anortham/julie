use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Identifier extraction for LSP-quality find_references
impl super::GoExtractor {
    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub(super) fn walk_tree_for_identifiers(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    pub(super) fn extract_identifier_from_node(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // Function/method calls: foo(), bar.Baz(), fn[T](args)
            "call_expression" => {
                // The function being called is typically the first child or in a selector
                let mut cursor = node.walk();
                let mut call_id: Option<Identifier> = None;
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" => {
                            // Simple function call: foo()
                            let name = self.base.get_node_text(&child);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);
                            let identifier = self.base.create_identifier(
                                &child,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                            call_id = Some(identifier);
                            break;
                        }
                        "selector_expression" => {
                            // Method call: obj.Method()
                            // Extract the rightmost identifier (the method name)
                            if let Some(field_node) = child.child_by_field_name("field") {
                                let name = self.base.get_node_text(&field_node);
                                let containing_symbol_id =
                                    self.find_containing_symbol_id(node, symbol_map);
                                let identifier = self.base.create_identifier(
                                    &field_node,
                                    name,
                                    IdentifierKind::Call,
                                    containing_symbol_id,
                                );
                                call_id = Some(identifier);
                            }
                            break;
                        }
                        _ => {}
                    }
                }
                // Record type arguments for generic function calls: fn[T](args)
                if let Some(ref identifier) = call_id {
                    if let Some(type_args_node) = node.child_by_field_name("type_arguments") {
                        let arguments = crate::base::extract_type_arguments(
                            &self.base,
                            type_args_node,
                            decompose_go_type_arg,
                        );
                        self.base.record_type_arguments(identifier, arguments);
                    }
                }
                // Phase 3b: capture string-literal call-arguments (config-free;
                // carrier classification + gate run later in the src/ pipeline).
                self.record_call_arg_literals(node, symbol_map);
            }

            // Member access: object.Field
            "selector_expression" => {
                // Only extract if it's NOT part of a call_expression
                // (we handle those in the call_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression" {
                        return; // Skip - handled by call_expression
                    }
                }

                // Extract the rightmost identifier (the field name)
                if let Some(field_node) = node.child_by_field_name("field") {
                    let name = self.base.get_node_text(&field_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &field_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            "type_identifier" => {
                let name = self.base.get_node_text(&node);
                if is_go_type_usage_identifier(&self.base, node) && !is_go_builtin_type(&name) {
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);
                    let identifier = self.base.create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                    record_outermost_go_type_arguments(&mut self.base, node, &identifier);
                }
            }

            _ => {}
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
    pub(super) fn find_containing_symbol_id(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        self.base
            .find_containing_symbol_from_map(&node, symbol_map)
            .map(|s| s.id.clone())
    }

    /// Capture string-literal arguments of a Go `call_expression` as `Literal`
    /// records (Miller bridge Phase 3b).
    ///
    /// Config-free: `carrier` is the verbatim callee text; the URL/SQL
    /// classification and the carrier gate run later in the `src/` pipeline.
    /// Records one literal per string-like argument, with `arg_position` counted
    /// over the full `argument_list`. Go has no string interpolation, so both
    /// `interpreted_string_literal` and `raw_string_literal` decode to their
    /// verbatim contents.
    pub(super) fn record_call_arg_literals(
        &mut self,
        call_node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        let Some(function_node) = call_node.child_by_field_name("function") else {
            return;
        };
        let Some(args_node) = call_node.child_by_field_name("arguments") else {
            return;
        };
        let carrier = go_carrier(&self.base, function_node);
        let containing_symbol_id = self.find_containing_symbol_id(call_node, symbol_map);

        let mut cursor = args_node.walk();
        for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
            if let Some(text) = self.base.decode_string_literal(&arg) {
                self.base.record_literal(
                    &arg,
                    text,
                    carrier.clone(),
                    pos as u32,
                    containing_symbol_id.clone(),
                );
            }
        }
    }
}

/// Derive a Go call's carrier from its callee.
///
/// Plain `identifier` → its text (`query`). `selector_expression`
/// (`http.Get`, `db.Query`) → the `operand.field` join so dotted client APIs
/// match config (`http.get`) exactly while bare DB verbs (`query`/`exec`) still
/// match any receiver via the gate's last-segment rule.
fn go_carrier(base: &BaseExtractor, function_node: Node) -> Option<String> {
    match function_node.kind() {
        "identifier" => Some(base.get_node_text(&function_node)),
        "selector_expression" => {
            let operand = function_node
                .child_by_field_name("operand")
                .map(|n| base.get_node_text(&n));
            let field = function_node
                .child_by_field_name("field")
                .map(|n| base.get_node_text(&n));
            match (operand, field) {
                (Some(o), Some(f)) => Some(format!("{o}.{f}")),
                (None, Some(f)) => Some(f),
                _ => None,
            }
        }
        _ => {
            let text = base.get_node_text(&function_node);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

/// If `name_node` is the base `type_identifier` of an *outermost* `generic_type`
/// use (e.g. the `Container` of `Container[int]`), records that generic's
/// ordered/nested applied type arguments against `identifier`.
///
/// Fires from the `type_identifier` arm so it uniformly covers field types,
/// variable declarations, and composite-literal types without an allowlist.
/// Nested generics are skipped: their args ride along as `children` of the
/// enclosing usage and are not double-counted as a separate row.
fn record_outermost_go_type_arguments(
    base: &mut BaseExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(generic_type) = name_node.parent() else {
        return;
    };
    if generic_type.kind() != "generic_type" {
        return;
    }
    // Confirm name_node is the `type` field (not a stray child).
    let is_type_field = generic_type
        .child_by_field_name("type")
        .map(|t| t.id() == name_node.id())
        .unwrap_or(false);
    if !is_type_field {
        return;
    }
    // A generic_type whose parent is type_elem is nested inside another
    // generic's type_arguments — its args ride along as children of the outer
    // usage rather than being recorded as a separate TypeArgumentUsage row.
    if generic_type
        .parent()
        .map(|p| p.kind() == "type_elem")
        .unwrap_or(false)
    {
        return;
    }
    let Some(arg_list) = generic_type.child_by_field_name("type_arguments") else {
        return;
    };
    let arguments = crate::base::extract_type_arguments(base, arg_list, decompose_go_type_arg);
    base.record_type_arguments(identifier, arguments);
}

/// `TypeArgDecomposer` for Go: maps a child of a `type_arguments` node to its
/// applied argument. Go wraps each argument in a `type_elem` node; skips
/// punctuation (`[`, `,`, `]`). For a nested `generic_type` inside `type_elem`
/// returns the base name plus its inner `type_arguments` to recurse into; for
/// every other type node returns its source text as a leaf.
fn decompose_go_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip punctuation: [, ], ,
    }
    if node.kind() != "type_elem" {
        return None; // unexpected — defensive skip
    }
    // type_elem wraps one type argument; get its single named inner node.
    let mut cursor = node.walk();
    let inner = node.named_children(&mut cursor).next()?;
    match inner.kind() {
        "generic_type" => {
            // Nested generic: extract name from `type` field, recurse into
            // its type_arguments for children.
            let name_node = inner.child_by_field_name("type")?;
            let name = base.get_node_text(&name_node);
            let nested = inner.child_by_field_name("type_arguments");
            Some((name, nested))
        }
        _ => {
            // Leaf type: type_identifier, qualified_type, pointer_type, etc.
            Some((base.get_node_text(&inner), None))
        }
    }
}

fn is_go_type_usage_identifier(base: &BaseExtractor, node: Node) -> bool {
    if is_go_declaration_type_name(node) || has_go_error_ancestor(node) {
        return false;
    }

    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "field_declaration" {
            if is_embedded_go_field_type(base, parent, node) {
                return true;
            }

            if let (Some(name_node), Some(type_node)) = (
                parent.child_by_field_name("name"),
                parent.child_by_field_name("type"),
            ) {
                return name_node.id() != node.id() && contains_node(type_node, node);
            }

            return false;
        }

        if let Some(type_node) = parent.child_by_field_name("type") {
            if contains_node(type_node, node) {
                return true;
            }
        }

        match parent.kind() {
            "qualified_type"
            | "pointer_type"
            | "slice_type"
            | "array_type"
            | "map_type"
            | "channel_type"
            | "generic_type"
            | "parameter_declaration"
            | "variadic_parameter_declaration" => return true,
            "selector_expression"
            | "call_expression"
            | "argument_list"
            | "statement_list"
            | "source_file" => return false,
            _ => {}
        }

        current = parent;
    }

    false
}

fn is_embedded_go_field_type(base: &BaseExtractor, parent: Node, node: Node) -> bool {
    let mut cursor = parent.walk();
    let named_children: Vec<_> = parent.named_children(&mut cursor).collect();
    if named_children.len() != 1 || named_children[0].id() != node.id() {
        return false;
    }

    if has_prior_recovery_error_sibling(parent) {
        return false;
    }

    let field_text = base.get_node_text(&parent);
    !field_text.contains('(') && !field_text.contains(',') && !field_text.contains("...")
}

fn has_prior_recovery_error_sibling(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    let mut cursor = parent.walk();
    for sibling in parent.children(&mut cursor) {
        if sibling.id() == node.id() {
            return false;
        }
        if sibling.kind() == "ERROR" || sibling.is_error() || sibling.is_missing() {
            return true;
        }
    }

    false
}

fn is_go_declaration_type_name(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if let Some(name_node) = parent.child_by_field_name("name") {
        if name_node.id() == node.id() {
            return matches!(
                parent.kind(),
                "type_spec"
                    | "type_parameter_declaration"
                    | "field_declaration"
                    | "function_declaration"
                    | "method_declaration"
                    | "parameter_declaration"
                    | "variadic_parameter_declaration"
            );
        }
    }

    matches!(parent.kind(), "type_parameter_list")
}

fn has_go_error_ancestor(node: Node) -> bool {
    let mut current = Some(node);
    while let Some(node) = current {
        if node.kind() == "ERROR" || node.is_error() || node.is_missing() {
            return true;
        }
        current = node.parent();
    }

    false
}

fn contains_node(parent: Node, child: Node) -> bool {
    child.start_byte() >= parent.start_byte() && child.end_byte() <= parent.end_byte()
}

fn is_go_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "bool"
            | "byte"
            | "comparable"
            | "complex64"
            | "complex128"
            | "error"
            | "float32"
            | "float64"
            | "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "rune"
            | "string"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "uintptr"
    )
}
