use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind, UnresolvedTarget};
use std::collections::HashMap;
use tree_sitter::Node;

fn import_path_from_signature(signature: &str) -> Option<&str> {
    signature
        .strip_prefix("import ")?
        .split_whitespace()
        .next_back()
        .map(|path| path.trim_matches('"'))
}

fn is_stdlib_import_path(import_path: &str) -> bool {
    matches!(import_path, "fmt")
}

/// Relationship extraction for Go (method receivers, interface implementations, embedding, function calls)
impl super::GoExtractor {
    pub(super) fn walk_tree_for_relationships(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // Handle interface implementations (implicit in Go)
        if node.kind() == "method_declaration" {
            self.extract_method_relationships_from_node(node, symbol_map, relationships);
        }

        // Handle struct embedding
        if node.kind() == "struct_type" {
            self.extract_embedding_relationships(node, symbol_map, relationships);
        }

        // Handle function calls (direct and cross-package)
        if node.kind() == "call_expression" {
            self.extract_call_relationships(node, symbol_map, relationships);
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_relationships(child, symbol_map, relationships);
        }
    }

    pub(super) fn extract_method_relationships_from_node(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // Extract method to receiver type relationship
        let receiver_list = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "parameter_list");
        if let Some(receiver_list) = receiver_list {
            let param_decl = receiver_list
                .children(&mut receiver_list.walk())
                .find(|c| c.kind() == "parameter_declaration");
            if let Some(param_decl) = param_decl {
                // Extract receiver type
                let receiver_type = self.extract_receiver_type_from_param(param_decl);
                let receiver_symbol = symbol_map.get(&receiver_type);

                let name_node = node
                    .children(&mut node.walk())
                    .find(|c| c.kind() == "field_identifier");
                if let Some(name_node) = name_node {
                    let method_name = self.get_node_text(name_node);
                    let method_symbol = symbol_map.get(&method_name);

                    if let (Some(receiver_sym), Some(method_sym)) = (receiver_symbol, method_symbol)
                    {
                        // Create Uses relationship from method to receiver type
                        relationships.push(self.base.create_relationship(
                            method_sym.id.clone(),
                            receiver_sym.id.clone(),
                            RelationshipKind::Uses,
                            &node,
                            Some(0.9),
                            None,
                        ));
                    }
                }
            }
        }
    }

    pub(super) fn extract_embedding_relationships(
        &self,
        _node: Node,
        _symbol_map: &HashMap<String, &Symbol>,
        _relationships: &mut Vec<Relationship>,
    ) {
        // Go struct embedding creates implicit relationships
        // This would need more complex parsing to detect embedded types
        // For now, we'll skip this advanced feature
    }

    /// Extract function call relationships
    ///
    /// Creates resolved Relationship when target is a local function/method.
    /// Creates PendingRelationship when target is:
    /// - An Import symbol (needs cross-file resolution)
    /// - Not found in local symbol_map (e.g., method on imported package)
    fn extract_call_relationships(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // In Go, call_expression has the function being called as the first child
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Find the function name - it's usually the first significant child
        // For package calls like fmt.Println, we need the Println part
        // For direct calls like helper, we need helper
        if let Some(func_node) = children.first() {
            let target = match func_node.kind() {
                // Direct call: helper()
                "identifier" => UnresolvedTarget::simple(self.base.get_node_text(func_node)),
                // Package call: fmt.Println() or package method calls
                "selector_expression" => {
                    let selector_children: Vec<_> = func_node
                        .children(&mut func_node.walk())
                        .filter(|c| c.kind() == "field_identifier" || c.kind() == "identifier")
                        .collect();
                    let Some(last) = selector_children.last() else {
                        return;
                    };
                    let terminal_name = self.base.get_node_text(last);
                    let receiver = selector_children.first().and_then(|first| {
                        if first.id() == last.id() {
                            None
                        } else {
                            Some(self.base.get_node_text(first))
                        }
                    });

                    if let Some(receiver) = receiver {
                        UnresolvedTarget {
                            display_name: format!("{receiver}.{terminal_name}"),
                            terminal_name,
                            receiver: Some(receiver),
                            namespace_path: Vec::new(),
                            import_context: None,
                        }
                    } else {
                        UnresolvedTarget::simple(terminal_name)
                    }
                }
                _ => return,
            };
            let callee_name = target.terminal_name.clone();

            // Find the containing function to know who is calling
            let caller_symbol = self.find_containing_function(symbol_map, node);
            if caller_symbol.is_none() {
                return; // Not inside a function, can't create relationship
            }
            let caller = caller_symbol.unwrap();

            // Check if we can resolve the callee locally
            match symbol_map.get(&callee_name) {
                Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
                    // Target is an Import symbol - need cross-file resolution
                    let pending = self.base.create_pending_relationship(
                        caller.id.clone(),
                        target.clone(),
                        RelationshipKind::Calls,
                        &node,
                        Some(caller.id.clone()),
                        Some(0.8),
                    );
                    self.add_structured_pending_relationship(pending);
                }
                Some(called_symbol) => {
                    // Target is a local function/method - create resolved Relationship
                    relationships.push(self.base.create_relationship(
                        caller.id.clone(),
                        called_symbol.id.clone(),
                        RelationshipKind::Calls,
                        &node,
                        Some(0.9),
                        None,
                    ));
                }
                None => {
                    // Target not found in local symbols. Package-qualified calls may still be
                    // methods on imported packages, so keep those pending unless the import is
                    // an obvious stdlib package.
                    if let Some(receiver) = target.receiver.as_deref() {
                        let is_stdlib_package = symbol_map
                            .get(receiver)
                            .filter(|symbol| symbol.kind == SymbolKind::Import)
                            .and_then(|symbol| symbol.signature.as_deref())
                            .and_then(import_path_from_signature)
                            .is_some_and(is_stdlib_import_path);

                        if is_stdlib_package {
                            return;
                        }
                    }

                    let pending = self.base.create_pending_relationship(
                        caller.id.clone(),
                        target,
                        RelationshipKind::Calls,
                        &node,
                        Some(caller.id.clone()),
                        Some(0.7),
                    );
                    self.add_structured_pending_relationship(pending);
                }
            }
        }
    }

    /// Find the containing function for a call node
    fn find_containing_function<'a>(
        &self,
        symbol_map: &HashMap<String, &'a Symbol>,
        node: Node,
    ) -> Option<&'a Symbol> {
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.kind() == "function_declaration" || parent.kind() == "method_declaration" {
                // Extract the function name
                let name = parent
                    .children(&mut parent.walk())
                    .find(|c| c.kind() == "identifier")
                    .map(|n| self.base.get_node_text(&n))
                    .unwrap_or_default();

                if !name.is_empty() {
                    return symbol_map.get(&name).copied();
                }
            }
            current = parent.parent();
        }
        None
    }
}
