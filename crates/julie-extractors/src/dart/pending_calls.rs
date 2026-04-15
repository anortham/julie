// Dart Extractor - Pending Cross-File Call Detection
//
// Walks the syntax tree to find function calls that reference symbols
// not defined in the current file, creating PendingRelationship entries
// for cross-file resolution during workspace indexing.

use super::helpers::find_child_by_type;
use crate::base::{Symbol, SymbolKind, UnresolvedTarget};
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

    fn check_identifier_call(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        let next = match node.next_sibling() {
            Some(s) => s,
            None => return,
        };
        if next.kind() != "selector" && next.kind() != "argument_part" && next.kind() != "arguments"
        {
            return;
        }
        let function_name = self.base.get_node_text(&node);
        let target = if next.kind() == "selector" {
            if let Some(method_node) = find_child_by_type(&next, "identifier") {
                let terminal_name = self.base.get_node_text(&method_node);
                UnresolvedTarget {
                    display_name: format!("{function_name}.{terminal_name}"),
                    terminal_name,
                    receiver: Some(function_name.clone()),
                    namespace_path: Vec::new(),
                    import_context: None,
                }
            } else {
                UnresolvedTarget::simple(function_name.clone())
            }
        } else {
            UnresolvedTarget::simple(function_name.clone())
        };

        if let Some(called_symbol) = symbol_map.get(target.terminal_name.as_str()) {
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
            let pending = self.base.create_pending_relationship(
                caller.id.clone(),
                target,
                crate::base::RelationshipKind::Calls,
                &node,
                Some(caller.id.clone()),
                Some(0.7),
            );
            self.add_structured_pending_relationship(pending);
        }
    }

    fn check_member_access_call(&mut self, node: &Node, symbol_map: &HashMap<String, &Symbol>) {
        let target = match self.extract_call_target(node) {
            Some(target) => target,
            None => return,
        };
        let function_name = target.terminal_name.clone();
        if symbol_map.contains_key(function_name.as_str()) {
            return;
        }
        if let Some(caller) = self.find_containing_function(*node, symbol_map) {
            let pending = self.base.create_pending_relationship(
                caller.id.clone(),
                target,
                crate::base::RelationshipKind::Calls,
                node,
                Some(caller.id.clone()),
                Some(0.7),
            );
            self.add_structured_pending_relationship(pending);
        }
    }

    /// Extract the unresolved target from a member_access node, if it's a call.
    fn extract_call_target(&self, node: &Node) -> Option<UnresolvedTarget> {
        let has_call = find_child_by_type(node, "selector")
            .and_then(|s| find_child_by_type(&s, "argument_part"))
            .is_some();
        if !has_call {
            return None;
        }
        if let Some(obj) = node.child_by_field_name("object") {
            if let Some(sel) = node.child_by_field_name("selector") {
                if let Some(id) = find_child_by_type(&sel, "identifier") {
                    let terminal_name = self.base.get_node_text(&id);
                    let receiver = self.base.get_node_text(&obj);
                    return Some(UnresolvedTarget {
                        display_name: format!("{receiver}.{terminal_name}"),
                        terminal_name,
                        receiver: Some(receiver),
                        namespace_path: Vec::new(),
                        import_context: None,
                    });
                }
            }
            let node_text = self.base.get_node_text(node);
            let call_head = node_text.split('(').next().unwrap_or(node_text.as_str());
            if let Some((receiver, terminal_name)) = call_head.rsplit_once('.') {
                return Some(UnresolvedTarget {
                    display_name: call_head.to_string(),
                    terminal_name: terminal_name.to_string(),
                    receiver: Some(receiver.to_string()),
                    namespace_path: Vec::new(),
                    import_context: None,
                });
            }
            if obj.kind() == "identifier" {
                return Some(UnresolvedTarget::simple(self.base.get_node_text(&obj)));
            }
        }
        if let Some(sel) = node.child_by_field_name("selector") {
            if let Some(id) = find_child_by_type(&sel, "identifier") {
                return Some(UnresolvedTarget::simple(self.base.get_node_text(&id)));
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
