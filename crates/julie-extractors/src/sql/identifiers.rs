//! Identifier extraction for SQL files.
//!
//! Handles walking the AST to extract identifier usages including:
//! - Function/procedure invocations
//! - Column references
//! - Qualified names (schema.table.column)

use crate::base::{IdentifierKind, Symbol};
use std::collections::HashMap;

use super::SqlExtractor;

impl SqlExtractor {
    /// Recursively walk tree extracting identifiers from each node
    pub(super) fn walk_tree_for_identifiers(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        self.extract_identifier_from_node(node, symbol_map);

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            "invocation" => {
                let name_node = if let Some(obj_ref) =
                    self.base.find_child_by_type(&node, "object_reference")
                {
                    self.base.find_child_by_type(&obj_ref, "identifier")
                } else {
                    self.base.find_child_by_type(&node, "identifier")
                };

                if let Some(name_node) = name_node {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }

            "identifier" => {
                if let Some(next_sibling) = node.next_sibling() {
                    if next_sibling.kind() == "function_arguments" {
                        let name = self.base.get_node_text(&node);
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        return;
                    }
                }

                if let Some(parent) = node.parent() {
                    match parent.kind() {
                        "select_expression" | "where_clause" | "having_clause" => {
                            let name = self.base.get_node_text(&node);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &node,
                                name,
                                IdentifierKind::MemberAccess,
                                containing_symbol_id,
                            );
                        }
                        _ => {}
                    }
                }
            }

            "field" => {
                if let Some(parent) = node.parent() {
                    if parent.kind() == "table_reference" || parent.kind() == "qualified_name" {
                        return;
                    }
                }

                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                } else {
                    let name = self.base.get_node_text(&node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            "qualified_name" => {
                let mut rightmost_identifier = None;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        rightmost_identifier = Some(child);
                    }
                }

                if let Some(name_node) = rightmost_identifier {
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

            _ => {}
        }
    }

    /// Find the ID of the symbol that contains this node
    fn find_containing_symbol_id(
        &self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
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
