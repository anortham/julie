/// LSP-quality identifier extraction for find_references support
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, extract_type_arguments};
use std::collections::HashMap;
use tree_sitter::Node;

impl super::RazorExtractor {
    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(
        &mut self,
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);

        // Return the collected identifiers
        self.base.identifiers.clone()
    }

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    /// Razor-specific: handles C# code within Razor directives and code blocks
    fn extract_identifier_from_node(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        match node.kind() {
            // Function/method calls: foo(), bar.Baz()
            // These appear in C# code blocks within Razor (@code {}, @{}, etc.)
            "invocation_expression" => {
                // The name is typically a child of the invocation_expression
                // Look for identifier or member_access_expression
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = self.base.get_node_text(&child);
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    } else if child.kind() == "member_access_expression" {
                        // For member access, extract the rightmost identifier (the method name)
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.base.get_node_text(&name_node);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &name_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                        break;
                    }
                }
                // Phase 3: capture string-literal call-arguments (config-free; the
                // carrier classification + gate happen in the src/ pipeline).
                self.record_razor_call_arg_literals(node, symbol_map);
            }

            // Member access: object.field
            // These appear in C# code blocks and Razor expressions
            "member_access_expression" => {
                // Only extract if it's NOT part of an invocation_expression
                // (we handle those in the invocation_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "invocation_expression" {
                        return; // Skip - handled by invocation_expression
                    }
                }

                // Extract the rightmost identifier (the member name)
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            // Type references in C# code blocks: `List<IBrowserFile>`, generics, etc.
            // Razor embeds C# with the same `generic_name` + `type_argument_list` grammar
            // as standalone C# — reuse the same outermost-check and decomposer logic.
            "identifier" => {
                let name = self.base.get_node_text(&node);
                if is_csharp_type_usage_identifier(node) && !is_csharp_builtin_type(&name) {
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);
                    let identifier = self.base.create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                    record_outermost_generic_type_arguments(&mut self.base, node, &identifier);
                }
            }

            _ => {
                // Skip other node types
            }
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
    fn find_containing_symbol_id(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        self.base
            .find_containing_symbol_from_map(&node, symbol_map)
            .map(|s| s.id.clone())
    }

    // ========================================================================
    // String-literal call-argument capture (Miller bridge Phase 3)
    // ========================================================================

    /// Capture string-literal arguments of a Razor/C# `invocation_expression`
    /// as `Literal` records. Config-free: `carrier` is the invoked method name
    /// (mirrors the C# leg); the URL/SQL classification and the carrier gate run
    /// later in the `src/` pipeline. Razor embeds C#, so each argument is wrapped
    /// in an `argument` node whose value is its last named child. `arg_position`
    /// is counted over the full argument list.
    fn record_razor_call_arg_literals(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let carrier = razor_carrier(&self.base, function);
        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

        let mut cursor = args.walk();
        for (pos, arg) in args.named_children(&mut cursor).enumerate() {
            let value = if arg.kind() == "argument" {
                let mut vc = arg.walk();
                arg.named_children(&mut vc).last()
            } else {
                Some(arg)
            };
            if let Some(value) = value {
                if let Some(text) = self.base.decode_string_literal(&value) {
                    self.base.record_literal(
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
}

/// Derive a Razor/C# call's carrier: the invoked method name with generic type
/// arguments stripped (`conn.Query<User>` -> `Query`, `Execute` -> `Execute`).
/// The receiver is dropped — Dapper/ADO/HttpClient carriers are matched by bare
/// method name via the gate's last-segment rule, and the receiver is usually a
/// local variable.
fn razor_carrier(base: &BaseExtractor, function: Node) -> Option<String> {
    let text = match function.kind() {
        "identifier" | "generic_name" => base.get_node_text(&function),
        "member_access_expression" => function
            .child_by_field_name("name")
            .map(|n| base.get_node_text(&n))?,
        _ => base.get_node_text(&function),
    };
    let stripped = match text.find('<') {
        Some(i) => text[..i].to_string(),
        None => text,
    };
    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}

// ── Free helpers (mirrors of csharp/identifiers.rs logic) ─────────────────────
// Razor embeds C# with the same generic_name + type_argument_list grammar.
// These functions operate on BaseExtractor / Node only — no C#-extractor coupling.

/// If `name_node` is the base identifier of an outermost `generic_name` use site
/// in Razor C# code (e.g. `List` in `List<IBrowserFile>`), record its ordered/
/// nested type arguments against `identifier`. Nested generics (whose `generic_name`
/// parent is a `type_argument_list`) are skipped — they ride along as `children`.
fn record_outermost_generic_type_arguments(
    base: &mut BaseExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(generic_name) = name_node.parent() else {
        return;
    };
    if generic_name.kind() != "generic_name" {
        return;
    }
    // A generic_name nested inside type_argument_list is itself a type argument
    // of an outer generic — skip here; it rides along as a child.
    if generic_name
        .parent()
        .map(|p| p.kind() == "type_argument_list")
        .unwrap_or(false)
    {
        return;
    }
    let Some(arg_list) = type_argument_list_child(generic_name) else {
        return;
    };
    let arguments = extract_type_arguments(base, arg_list, decompose_csharp_type_arg);
    base.record_type_arguments(identifier, arguments);
}

/// `TypeArgDecomposer` for Razor/C#: maps a child of `type_argument_list` to its
/// applied argument. Named `generic_name` children recurse (nested generics);
/// everything else (identifier, predefined_type, array_type, …) returns its
/// source text as a leaf. Unnamed punctuation is skipped.
fn decompose_csharp_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None;
    }
    match node.kind() {
        "generic_name" => {
            // Name is the first identifier child of generic_name.
            let name = direct_identifier(base, node)
                .map(|(_, n)| n)
                .unwrap_or_else(|| base.get_node_text(&node));
            Some((name, type_argument_list_child(node)))
        }
        _ => Some((base.get_node_text(&node), None)),
    }
}

/// First `type_argument_list` child of a `generic_name` node.
fn type_argument_list_child(generic_name: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = generic_name.walk();
    generic_name
        .children(&mut cursor)
        .find(|c| c.kind() == "type_argument_list")
}

/// First `identifier` child of `node`, returned with its source text.
fn direct_identifier<'a>(base: &BaseExtractor, node: Node<'a>) -> Option<(Node<'a>, String)> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some((child, base.get_node_text(&child)));
        }
    }
    None
}

/// Returns `true` when `node` is an `identifier` used in a type-annotation position
/// inside Razor/C# code (field type, parameter type, return type, generic arg, etc.).
fn is_csharp_type_usage_identifier(node: Node) -> bool {
    if is_csharp_declaration_name(node) {
        return false;
    }
    let mut current = node;
    while let Some(parent) = current.parent() {
        if let Some(type_node) = parent.child_by_field_name("type") {
            if contains_node(type_node, node) {
                return true;
            }
        }
        match parent.kind() {
            "generic_name" | "qualified_name" | "array_type" | "nullable_type" | "pointer_type"
            | "tuple_type" | "type_argument_list" => return true,
            "object_creation_expression" => {
                if let Some(type_node) = parent.child_by_field_name("type") {
                    if contains_node(type_node, node) {
                        return true;
                    }
                }
            }
            "invocation_expression"
            | "member_access_expression"
            | "argument_list"
            | "assignment_expression"
            | "return_statement"
            | "block"
            | "compilation_unit" => return false,
            _ => {}
        }
        current = parent;
    }
    false
}

/// Returns `true` when `node` is the declared name of a type, method, property,
/// namespace, or generic type parameter — not a reference position.
fn is_csharp_declaration_name(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if let Some(name_node) = parent.child_by_field_name("name") {
        if name_node.id() == node.id() {
            return matches!(
                parent.kind(),
                "class_declaration"
                    | "interface_declaration"
                    | "struct_declaration"
                    | "enum_declaration"
                    | "method_declaration"
                    | "property_declaration"
                    | "namespace_declaration"
                    | "type_parameter"
            );
        }
    }
    false
}

/// Returns `true` when `child` is byte-range-contained within `parent`.
fn contains_node(parent: Node, child: Node) -> bool {
    child.start_byte() >= parent.start_byte() && child.end_byte() <= parent.end_byte()
}

/// Returns `true` for C# builtin type keywords that are noise for centrality.
fn is_csharp_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "byte"
            | "char"
            | "decimal"
            | "double"
            | "float"
            | "int"
            | "long"
            | "object"
            | "sbyte"
            | "short"
            | "string"
            | "uint"
            | "ulong"
            | "ushort"
            | "var"
            | "void"
    )
}
