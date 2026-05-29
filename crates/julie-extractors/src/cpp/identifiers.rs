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
    let arguments =
        crate::base::extract_type_arguments(base, arg_list, decompose_cpp_type_arg);
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
