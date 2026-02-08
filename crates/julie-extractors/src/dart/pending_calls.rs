// Dart Extractor - Pending Cross-File Call Detection
//
// Walks the syntax tree to find function calls that reference symbols
// not defined in the current file, creating PendingRelationship entries
// for cross-file resolution during workspace indexing.

use super::helpers::find_child_by_type;
use crate::base::{PendingRelationship, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::Node;

impl super::DartExtractor {
    /// Walk the tree looking for function calls that reference unknown symbols
    pub(super) fn walk_for_pending_calls(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        if node.kind() == "identifier" {
            self.check_identifier_call(node, symbol_map);
        }

        if node.kind() == "member_access" {
            self.check_member_access_call(&node, symbol_map);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_for_pending_calls(child, symbol_map);
        }
    }

    fn check_identifier_call(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        let next = match node.next_sibling() {
            Some(s) => s,
            None => return,
        };
        if next.kind() != "selector" && next.kind() != "argument_part" && next.kind() != "arguments"
        {
            return;
        }
        let function_name = self.base.get_node_text(&node);
        if let Some(called_symbol) = symbol_map.get(function_name.as_str()) {
            // Same-file call
            if let Some(caller) = self.find_containing_function(node, symbol_map) {
                if caller.id != called_symbol.id {
                    let line = node.start_position().row as u32 + 1;
                    self.same_file_calls
                        .push((caller.id.clone(), called_symbol.id.clone(), line));
                }
            }
        } else if let Some(caller) = self.find_containing_function(node, symbol_map) {
            // Cross-file call
            let line = node.start_position().row as u32 + 1;
            self.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: function_name.clone(),
                kind: crate::base::RelationshipKind::Calls,
                file_path: self.base.file_path.clone(),
                line_number: line,
                confidence: 0.7,
            });
        }
    }

    fn check_member_access_call(
        &mut self,
        node: &Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        let function_name = match self.extract_call_name(node) {
            Some(n) => n,
            None => return,
        };
        if symbol_map.contains_key(function_name.as_str()) {
            return;
        }
        if let Some(caller) = self.find_containing_function(*node, symbol_map) {
            let line = node.start_position().row as u32 + 1;
            self.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: function_name.clone(),
                kind: crate::base::RelationshipKind::Calls,
                file_path: self.base.file_path.clone(),
                line_number: line,
                confidence: 0.7,
            });
        }
    }

    /// Extract the called function name from a member_access node, if it's a call.
    fn extract_call_name(&self, node: &Node) -> Option<String> {
        let has_call = find_child_by_type(node, "selector")
            .and_then(|s| find_child_by_type(&s, "argument_part"))
            .is_some();
        if !has_call {
            return None;
        }
        if let Some(obj) = node.child_by_field_name("object") {
            if let Some(sel) = node.child_by_field_name("selector") {
                if let Some(id) = find_child_by_type(&sel, "identifier") {
                    return Some(self.base.get_node_text(&id));
                }
            }
            if obj.kind() == "identifier" {
                return Some(self.base.get_node_text(&obj));
            }
        }
        if let Some(sel) = node.child_by_field_name("selector") {
            if let Some(id) = find_child_by_type(&sel, "identifier") {
                return Some(self.base.get_node_text(&id));
            }
        }
        None
    }

    /// Find the containing function for a node by walking up the tree
    fn find_containing_function<'a>(
        &self,
        node: Node,
        symbol_map: &'a HashMap<String, &'a Symbol>,
    ) -> Option<&'a Symbol> {
        let mut current = node.parent();
        while let Some(cur) = current {
            if cur.kind() == "function_body" || cur.kind() == "lambda_expression" {
                if let Some(parent) = cur.parent() {
                    let mut cursor = parent.walk();
                    for sibling in parent.children(&mut cursor) {
                        if sibling.kind() == "function_signature" {
                            if let Some(sym) = self.lookup_func_symbol(&sibling, symbol_map) {
                                return Some(sym);
                            }
                        }
                    }
                }
            }
            if matches!(
                cur.kind(),
                "function_declaration" | "method_signature" | "function_signature"
            ) {
                if let Some(sym) = self.lookup_func_symbol(&cur, symbol_map) {
                    return Some(sym);
                }
            }
            current = cur.parent();
        }
        None
    }

    fn lookup_func_symbol<'a>(
        &self,
        node: &Node,
        symbol_map: &'a HashMap<String, &'a Symbol>,
    ) -> Option<&'a Symbol> {
        let name_node = find_child_by_type(node, "identifier")?;
        let name = self.base.get_node_text(&name_node);
        let sym = symbol_map.get(&name)?;
        if matches!(sym.kind, SymbolKind::Function | SymbolKind::Method) {
            Some(sym)
        } else {
            None
        }
    }
}
