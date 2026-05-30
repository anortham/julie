//! C++ identifier extraction for LSP find_references functionality
//!
//! Extracts function calls, member access, and other identifier usages
//! from C++ source code for precise code navigation.

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

use super::CppExtractor;
use super::helpers;

impl CppExtractor {
    /// Walk the tree and extract identifiers
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
    fn extract_identifier_from_node(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        match node.kind() {
            // Function calls: foo(), bar.baz(), make_shared<Foo>()
            "call_expression" => {
                // Phase 3: capture string-literal call-arguments (config-free; the
                // carrier classification + gate happen in the src/ pipeline). Done
                // first so it also covers template calls (`query<T>("SELECT ...")`),
                // which the identifier logic below returns early for.
                self.record_call_arg_literals(node, symbol_map);
                if let Some(func_node) = node.child_by_field_name("function") {
                    // Template function call: make_shared<Foo>(), invoke<T>(), etc.
                    if func_node.kind() == "template_function" {
                        if let Some(name_node) = func_node.child_by_field_name("name") {
                            let name = self.base.get_node_text(&name_node);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);
                            let identifier = self.base.create_identifier(
                                &name_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                            if let Some(arg_list) = func_node.child_by_field_name("arguments") {
                                let arguments = crate::base::extract_type_arguments(
                                    &self.base,
                                    arg_list,
                                    decompose_cpp_type_arg,
                                );
                                self.base.record_type_arguments(&identifier, arguments);
                            }
                        }
                        return;
                    }

                    let (identifier_node, name) = if func_node.kind() == "field_expression" {
                        if let Some(field_node) = func_node.child_by_field_name("field") {
                            (field_node, self.base.get_node_text(&field_node))
                        } else {
                            (func_node, self.base.get_node_text(&func_node))
                        }
                    } else {
                        (func_node, self.base.get_node_text(&func_node))
                    };

                    // Find containing symbol (which function/method contains this call)
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    // Create identifier for this function call
                    self.base.create_identifier(
                        &identifier_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }

            // Member access: object.field, object->field
            "field_expression" => {
                // Extract the field name
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

            // Type references: MyClass x, void f(MyStruct param), Container<MyClass>
            // C++ tree-sitter uses `type_identifier` for BOTH declaration names
            // (class MyClass, struct Foo, enum Bar) AND reference positions.
            // We only want references — declarations are filtered by parent context.
            "type_identifier" => {
                if helpers::is_type_declaration_name(&node) {
                    return;
                }

                let name = self.base.get_node_text(&node);

                if helpers::is_noise_type(&name) {
                    return;
                }

                let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                let identifier = self.base.create_identifier(
                    &node,
                    name,
                    IdentifierKind::TypeUsage,
                    containing_symbol_id,
                );
                record_outermost_cpp_type_arguments(&mut self.base, node, &identifier);
            }

            _ => {}
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL FIX: Only search symbols from THIS FILE, not all files
    fn find_containing_symbol_id(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        self.base
            .find_containing_symbol_from_map(&node, symbol_map)
            .map(|s| s.id.clone())
    }

    /// Capture string-literal arguments of a C++ `call_expression` as `Literal`
    /// records. Config-free: `carrier` is the called function name (or
    /// `recv.method` for a member/qualified call); the URL/SQL classification and
    /// the carrier gate run later in the `src/` pipeline. C++ wraps arguments in
    /// an `argument_list` with no per-argument name wrapper, so each named child
    /// is decoded directly. `arg_position` is counted over the full argument list,
    /// so e.g. the URL in `curl_easy_setopt(h, CURLOPT_URL, "https://...")`
    /// reports position 2.
    fn record_call_arg_literals(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        let Some(func_node) = node.child_by_field_name("function") else {
            return;
        };
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let carrier = cpp_carrier(&self.base, func_node);
        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

        let mut cursor = args.walk();
        for (pos, arg) in args.named_children(&mut cursor).enumerate() {
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

/// Truncate a callee segment at its first `<` so generic arguments don't leak
/// into the carrier (`query<User>` -> `query`). Mirrors the C# leg's generic
/// strip and keeps the gate's last-segment match working for template methods.
fn strip_cpp_generics(text: &str) -> String {
    match text.find('<') {
        Some(i) => text[..i].to_string(),
        None => text.to_string(),
    }
}

/// Derive a C++ call's carrier. Plain `identifier` → its text (`PQexec`);
/// `field_expression` (`db.exec(...)`, `repo.query<User>(...)`) → the
/// `object.field` join (generics stripped from the field) so the gate's
/// last-segment rule can match a bare config; `template_function`
/// (`query<T>(...)`) → the `name` field; `qualified_identifier` (`ns::fn(...)`)
/// → the trailing `name` segment.
fn cpp_carrier(base: &BaseExtractor, func_node: Node) -> Option<String> {
    match func_node.kind() {
        "identifier" => Some(base.get_node_text(&func_node)),
        "field_expression" => {
            let object = func_node
                .child_by_field_name("argument")
                .map(|n| base.get_node_text(&n));
            let field = func_node
                .child_by_field_name("field")
                .map(|n| strip_cpp_generics(&base.get_node_text(&n)));
            match (object, field) {
                (Some(o), Some(f)) => Some(format!("{o}.{f}")),
                (None, Some(f)) => Some(f),
                _ => None,
            }
        }
        "template_function" => func_node
            .child_by_field_name("name")
            .map(|n| strip_cpp_generics(&base.get_node_text(&n))),
        "qualified_identifier" => func_node
            .child_by_field_name("name")
            .map(|n| base.get_node_text(&n))
            .or_else(|| Some(base.get_node_text(&func_node))),
        _ => {
            let text = base.get_node_text(&func_node);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

// ============================================================================
// Type-argument capture helpers (Miller bridge Phase 2)
// ============================================================================

/// Record type arguments for the outermost `template_type` generic use site.
///
/// Called from the `type_identifier` arm after creating the identifier.  Records
/// only when:
/// - the `type_identifier`'s parent is a `template_type` (e.g. `Box` in `Box<Item>`)
/// - AND that `template_type` is not itself nested inside a `type_descriptor` (which
///   places it inside another template's `template_argument_list`)
///
/// The qualified-identifier case (`std::vector<T>`) is handled by also checking
/// one level further: if the parent of `template_type` is a `qualified_identifier`
/// which is itself inside a `type_descriptor`, it's still nested.
fn record_outermost_cpp_type_arguments(
    base: &mut BaseExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(parent) = name_node.parent() else {
        return;
    };
    if parent.kind() != "template_type" {
        return;
    }
    // "Outermost" means the template_type is not nested inside another
    // template's type_descriptor argument wrapper.
    let template_parent = parent.parent();
    let is_nested = template_parent
        .map(|tp| {
            tp.kind() == "type_descriptor"
                || (tp.kind() == "qualified_identifier"
                    && tp
                        .parent()
                        .map(|gp| gp.kind() == "type_descriptor")
                        .unwrap_or(false))
        })
        .unwrap_or(false);
    if is_nested {
        return;
    }
    let Some(arg_list) = parent.child_by_field_name("arguments") else {
        return;
    };
    let arguments = crate::base::extract_type_arguments(base, arg_list, decompose_cpp_type_arg);
    base.record_type_arguments(identifier, arguments);
}

/// Decompose a child of `template_argument_list` into `(type_name, nested_arg_list)`.
///
/// C++ template arguments are wrapped in `type_descriptor` nodes. We unwrap the
/// `type` field of the descriptor:
/// - `template_type` → nested generic: name from `name` field, recurse into `arguments`
/// - Anything else (`primitive_type`, `type_identifier`, `qualified_identifier`, …) → leaf
///
/// Non-type template arguments (`expression` children) are skipped.
fn decompose_cpp_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip < , >
    }
    match node.kind() {
        "type_descriptor" => {
            let type_node = node.child_by_field_name("type")?;
            match type_node.kind() {
                "template_type" => {
                    // Nested generic
                    let name = type_node
                        .child_by_field_name("name")
                        .map(|n| base.get_node_text(&n))
                        .unwrap_or_else(|| base.get_node_text(&type_node));
                    let nested = type_node.child_by_field_name("arguments");
                    Some((name, nested))
                }
                _ => {
                    // Leaf: primitive_type, type_identifier, qualified_identifier, etc.
                    Some((base.get_node_text(&type_node), None))
                }
            }
        }
        _ => {
            // Non-type template argument (e.g. `5` in `array<int, 5>`).
            // Capture the raw source text as a leaf; dropping these shifts ordinals.
            Some((base.get_node_text(&node), None))
        }
    }
}
