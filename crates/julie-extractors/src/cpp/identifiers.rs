//! C++ identifier extraction for LSP find_references functionality
//!
//! Extracts function calls, member access, and other identifier usages
//! from C++ source code for precise code navigation.

use crate::base::{IdentifierKind, Symbol};
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
            // Function calls: foo(), bar.baz()
            "call_expression" => {
                if let Some(func_node) = node.child_by_field_name("function") {
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
        self.base
            .find_containing_symbol_from_map(&node, symbol_map)
            .map(|s| s.id.clone())
    }
}
