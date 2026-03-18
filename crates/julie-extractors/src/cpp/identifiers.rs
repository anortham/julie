//! C++ identifier extraction for LSP find_references functionality
//!
//! Extracts function calls, member access, and other identifier usages
//! from C++ source code for precise code navigation.

use crate::base::{IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

use super::CppExtractor;

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
            // Function calls: foo(), bar.baz()
            "call_expression" => {
                if let Some(func_node) = node.child_by_field_name("function") {
                    let name = self.base.get_node_text(&func_node);

                    // Find containing symbol (which function/method contains this call)
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    // Create identifier for this function call
                    self.base.create_identifier(
                        &func_node,
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
                if is_cpp_type_declaration_name(&node) {
                    return;
                }

                let name = self.base.get_node_text(&node);

                if is_cpp_noise_type(&name) {
                    return;
                }

                let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                self.base.create_identifier(
                    &node,
                    name,
                    IdentifierKind::TypeUsage,
                    containing_symbol_id,
                );
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
        // CRITICAL FIX: Only search symbols from THIS FILE, not all files
        // Bug was: searching all symbols in DB caused wrong file symbols to match
        let file_symbols: Vec<Symbol> = symbol_map
            .values()
            .filter(|s| s.file_path == self.base.file_path)
            .map(|&s| s.clone())
            .collect();

        self.base
            .find_containing_symbol(&node, &file_symbols)
            .map(|s| s.id.clone())
    }
}

/// Check if a `type_identifier` node is a declaration name rather than a type reference.
///
/// In C++ tree-sitter, `type_identifier` appears as the `name` field of:
/// - `class_specifier` -> `class MyClass {}`
/// - `struct_specifier` -> `struct MyStruct {}`
/// - `enum_specifier` -> `enum Color { ... }`
/// - `type_definition` -> `typedef int MyInt;` (the alias name)
/// - `template_type_parameter` -> `template<typename T>` (T is a declaration)
///
/// It also appears in reference positions like parameter types, variable types,
/// template arguments, etc. — those are NOT declarations and should produce TypeUsage.
fn is_cpp_type_declaration_name(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        if let Some(name_node) = parent.child_by_field_name("name") {
            if name_node.id() == node.id() {
                return matches!(
                    parent.kind(),
                    "class_specifier"
                        | "struct_specifier"
                        | "enum_specifier"
                        | "type_definition"
                        | "template_type_parameter"
                );
            }
        }
        // For type_definition, the alias name is in the `declarator` field, not `name`
        if parent.kind() == "type_definition" {
            if let Some(declarator) = parent.child_by_field_name("declarator") {
                if declarator.id() == node.id() {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns true for C++ types that are too common to be meaningful
/// type references for centrality scoring.
///
/// Filters:
/// - Single-letter uppercase names (T, U, V, etc.) — generic template parameters
fn is_cpp_noise_type(name: &str) -> bool {
    // Single-letter uppercase names are almost always template type parameters.
    // Even when they appear as references (e.g. `T value`), they carry no cross-file signal.
    if name.len() == 1
        && name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_uppercase())
    {
        return true;
    }

    false
}
