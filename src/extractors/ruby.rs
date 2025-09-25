use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, SymbolOptions, Visibility};
use tree_sitter::{Tree, Node};

/// Ruby extractor for extracting symbols and relationships from Ruby source code
/// Port of Miller's comprehensive Ruby extractor with metaprogramming support
pub struct RubyExtractor {
    base: BaseExtractor,
    current_visibility: Visibility,
}

impl RubyExtractor {
    pub fn new(file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new("ruby".to_string(), file_path, content),
            current_visibility: Visibility::Public,
        }
    }

    /// Extract all symbols from Ruby source code
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.current_visibility = Visibility::Public; // Reset for each file

        // Clear any previous symbols from symbol_map
        self.base.symbol_map.clear();

        self.traverse_tree(tree.root_node(), &mut symbols);

        // Include additional symbols from symbol_map (parallel assignments, etc.)
        for (_, symbol) in self.base.symbol_map.iter() {
            symbols.push(symbol.clone());
        }

        symbols
    }

    /// Extract relationships between symbols (inheritance, module inclusion, etc.)
    pub fn extract_relationships(&self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.extract_relationships_from_node(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn traverse_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>) {
        self.traverse_tree_with_parent(node, symbols, None);
    }

    fn traverse_tree_with_parent(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        let mut symbol_opt: Option<Symbol> = None;

        match node.kind() {
            "module" => {
                symbol_opt = Some(self.extract_module(node, parent_id.clone()));
            }
            "class" => {
                symbol_opt = Some(self.extract_class(node, parent_id.clone()));
            }
            "singleton_class" => {
                symbol_opt = Some(self.extract_singleton_class(node, parent_id.clone()));
            }
            "method" => {
                symbol_opt = Some(self.extract_method(node, parent_id.clone()));
            }
            "singleton_method" => {
                symbol_opt = Some(self.extract_singleton_method(node, parent_id.clone()));
            }
            "call" => {
                if let Some(symbol) = self.extract_call(node) {
                    symbol_opt = Some(symbol);
                }
            }
            "assignment" | "operator_assignment" => {
                // Handle assignments by extracting symbols
                if let Some(symbol) = self.extract_assignment(node, parent_id.clone()) {
                    symbols.push(symbol);
                }
            }
            "class_variable" | "instance_variable" | "global_variable" => {
                // Only create symbol if not part of an assignment (which handles it)
                // TODO: Implement is_part_of_assignment method
                // if !self.is_part_of_assignment(&node) {
                    symbol_opt = Some(self.extract_variable(node));
                // }
            }
            "constant" => {
                // Extract all constants for now to debug the parent_id issue
                symbol_opt = Some(self.extract_constant(node, parent_id.clone()));
            }
            "alias" => {
                symbol_opt = Some(self.extract_alias(node));
            }
            "identifier" => {
                // Handle visibility modifiers
                let text = self.base.get_node_text(&node);
                let new_visibility = match text.as_str() {
                    "private" => Visibility::Private,
                    "protected" => Visibility::Protected,
                    "public" => Visibility::Public,
                    _ => self.current_visibility.clone(),
                };
                if new_visibility != self.current_visibility {
                    self.current_visibility = new_visibility.clone();
                }
            }
            _ => {}
        }

        // Add symbol to collection and update parent_id for children
        let current_parent_id = if let Some(symbol) = symbol_opt {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);
            Some(symbol_id)
        } else {
            parent_id
        };

        // Recursively traverse children with updated parent context
        let old_visibility = self.current_visibility.clone();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // Check if child is a visibility modifier that affects subsequent siblings
            if child.kind() == "identifier" {
                let text = self.base.get_node_text(&child);
                if matches!(text.as_str(), "private" | "protected" | "public") {
                    self.current_visibility = match text.as_str() {
                        "private" => Visibility::Private,
                        "protected" => Visibility::Protected,
                        "public" => Visibility::Public,
                        _ => self.current_visibility.clone(),
                    };
                }
            }
            self.traverse_tree_with_parent(child, symbols, current_parent_id.clone());
        }
        self.current_visibility = old_visibility; // Restore previous visibility
    }

    fn extract_module(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        // Try different field names that Ruby tree-sitter uses
        let name = self.extract_name_from_node(node, "name")
            .or_else(|| self.extract_name_from_node(node, "constant"))
            .or_else(|| {
                // Fallback: find first constant child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "constant" {
                        return Some(self.base.get_node_text(&child));
                    }
                }
                None
            })
            .unwrap_or_else(|| "UnknownModule".to_string());

        let signature = self.build_module_signature(&node, &name);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Module,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_class(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        // Try different field names that Ruby tree-sitter uses
        let name = self.extract_name_from_node(node, "name")
            .or_else(|| self.extract_name_from_node(node, "constant"))
            .or_else(|| {
                // Fallback: find first constant child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "constant" {
                        return Some(self.base.get_node_text(&child));
                    }
                }
                None
            })
            .unwrap_or_else(|| "UnknownClass".to_string());

        let signature = self.build_class_signature(&node, &name);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_singleton_class(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        // Find the target of the singleton class (self, identifier, etc.)
        let target_node = node.children(&mut node.walk()).find(|c| matches!(c.kind(), "self" | "identifier"));
        let target = target_node.map(|n| self.base.get_node_text(&n)).unwrap_or_else(|| "self".to_string());
        let signature = format!("class << {}", target);

        self.base.create_symbol(
            &node,
            format!("<<{}", target),
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_method(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name = self.extract_name_from_node(node, "name")
            .or_else(|| self.extract_name_from_node(node, "identifier"))
            .or_else(|| self.extract_name_from_node(node, "operator"))
            .or_else(|| {
                // Fallback: find method name by traversing children
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" | "operator" => {
                            return Some(self.base.get_node_text(&child));
                        }
                        _ => continue,
                    }
                }
                None
            })
            .unwrap_or_else(|| "unknownMethod".to_string());

        let signature = self.build_method_signature(&node, &name);
        let kind = if name == "initialize" { SymbolKind::Constructor } else { SymbolKind::Method };

        let visibility = self.current_visibility.clone();

        self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_singleton_method(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name = self.extract_singleton_method_name(node);
        let signature = self.build_singleton_method_signature(&node, &name);

        let visibility = self.current_visibility.clone();

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_call(&mut self, node: Node) -> Option<Symbol> {
        let method_name = self.extract_method_name_from_call(node)?;

        match method_name.as_str() {
            "require" | "require_relative" => self.extract_require(node),
            "attr_reader" | "attr_writer" | "attr_accessor" => self.extract_attr_accessor(node, &method_name),
            "define_method" | "define_singleton_method" => self.extract_define_method(node, &method_name),
            "def_delegator" => self.extract_def_delegator(node),
            _ => None,
        }
    }

    fn extract_assignment(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        // Handle various assignment patterns including parallel assignment
        let left_side = node.child_by_field_name("left").or_else(|| node.children(&mut node.walk()).next())?;

        // Handle parallel assignments (a, b, c = 1, 2, 3)
        if left_side.kind() == "left_assignment_list" {
            // For parallel assignments, we need to extract each variable separately
            // Return the first variable as the primary symbol, and store others in additionalSymbols
            let right_side = node.child_by_field_name("right").or_else(|| node.children(&mut node.walk()).last());
            let _right_value = right_side.map(|n| self.base.get_node_text(&n)).unwrap_or_default();
            let full_assignment = self.base.get_node_text(&node);

            // Extract identifiers from left_assignment_list
            let mut cursor = left_side.walk();
            let identifiers: Vec<_> = left_side.children(&mut cursor)
                .filter(|child| child.kind() == "identifier")
                .collect();

            // Extract rest assignments (splat expressions like *rest)
            let mut cursor = left_side.walk();
            let rest_assignments: Vec<_> = left_side.children(&mut cursor)
                .filter(|child| child.kind() == "rest_assignment")
                .collect();

            // Create symbols for identifiers
            let mut created_symbols = Vec::new();

            for identifier in &identifiers {
                let name = self.base.get_node_text(identifier);
                let symbol = self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Variable,
                    SymbolOptions {
                        signature: Some(full_assignment.clone()),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.clone(),
                        metadata: None,
                        doc_comment: None,
                    },
                );
                created_symbols.push(symbol);
            }

            // Handle rest assignments
            for rest_node in &rest_assignments {
                if let Some(rest_identifier) = rest_node.children(&mut rest_node.walk()).find(|c| c.kind() == "identifier") {
                    let rest_name = self.base.get_node_text(&rest_identifier);
                    let rest_symbol = self.base.create_symbol(
                        &node,
                        rest_name,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(full_assignment.clone()),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.clone(),
                            metadata: None,
                            doc_comment: None,
                        },
                    );
                    created_symbols.push(rest_symbol);
                }
            }

            // Store additional symbols in the base extractor's symbol_map
            // Since this method only returns one symbol, we add the rest to the symbol_map
            for symbol in created_symbols.iter().skip(1) {
                self.base.symbol_map.insert(symbol.id.clone(), symbol.clone());
            }

            // Return the first symbol (if any were created)
            return created_symbols.into_iter().next();
        }

        // Handle regular assignments
        let right_side = node.child_by_field_name("right").or_else(|| node.children(&mut node.walk()).last());
        let name = self.base.get_node_text(&left_side);
        let signature = if let Some(right) = right_side {
            format!("{} = {}", name, self.base.get_node_text(&right))
        } else {
            name.clone()
        };

        let kind = self.infer_symbol_kind_from_assignment(&left_side);

        Some(self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    #[allow(dead_code)]
    fn extract_assignment_symbols(&mut self, node: Node, parent_id: Option<String>, symbols: &mut Vec<Symbol>) {
        let left_side = node.child_by_field_name("left").or_else(|| node.children(&mut node.walk()).next());

        if let Some(left) = left_side {
            // Handle parallel assignments (a, b, c = 1, 2, 3)
            if left.kind() == "left_assignment_list" {
                let full_assignment = self.base.get_node_text(&node);

                // Extract all identifiers from left_assignment_list
                let mut cursor = left.walk();
                for child in left.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = self.base.get_node_text(&child);
                        let symbol = self.base.create_symbol(
                            &node,
                            name,
                            SymbolKind::Variable,
                            SymbolOptions {
                                signature: Some(full_assignment.clone()),
                                visibility: Some(Visibility::Public),
                                parent_id: parent_id.clone(),
                                metadata: None,
                                doc_comment: None,
                            },
                        );
                        symbols.push(symbol);
                    } else if child.kind() == "rest_assignment" {
                        // Handle rest assignments (splat expressions like *rest)
                        if let Some(rest_identifier) = child.children(&mut child.walk()).find(|c| c.kind() == "identifier") {
                            let rest_name = self.base.get_node_text(&rest_identifier);
                            let symbol = self.base.create_symbol(
                                &node,
                                rest_name,
                                SymbolKind::Variable,
                                SymbolOptions {
                                    signature: Some(full_assignment.clone()),
                                    visibility: Some(Visibility::Public),
                                    parent_id: parent_id.clone(),
                                    metadata: None,
                                    doc_comment: None,
                                },
                            );
                            symbols.push(symbol);
                        }
                    } else if child.kind() == "splat_argument" || child.kind().contains("splat") {
                        // Try alternative splat node names
                        if let Some(rest_identifier) = child.children(&mut child.walk()).find(|c| c.kind() == "identifier") {
                            let rest_name = self.base.get_node_text(&rest_identifier);
                            let symbol = self.base.create_symbol(
                                &node,
                                rest_name,
                                SymbolKind::Variable,
                                SymbolOptions {
                                    signature: Some(full_assignment.clone()),
                                    visibility: Some(Visibility::Public),
                                    parent_id: parent_id.clone(),
                                    metadata: None,
                                    doc_comment: None,
                                },
                            );
                            symbols.push(symbol);
                        }
                    }
                }
                return;
            } else {
                // Check if this might be a parallel assignment with different structure
                let assignment_text = self.base.get_node_text(&node);
                if assignment_text.contains(",") && (assignment_text.contains("*") || assignment_text.contains("=")) {
                    // Try to extract variables from a manual parse of the assignment
                    self.extract_parallel_assignment_fallback(&node, &assignment_text, parent_id.clone(), symbols);
                    return;
                }
            }

            // Handle regular assignments
            let right_side = node.child_by_field_name("right").or_else(|| node.children(&mut node.walk()).last());
            let name = self.base.get_node_text(&left);
            let signature = if let Some(right) = right_side {
                format!("{} = {}", name, self.base.get_node_text(&right))
            } else {
                name.clone()
            };

            let kind = self.infer_symbol_kind_from_assignment(&left);

            let symbol = self.base.create_symbol(
                &node,
                name,
                kind,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id,
                    metadata: None,
                    doc_comment: None,
                },
            );
            symbols.push(symbol);
        }
    }

    #[allow(dead_code)]
    fn extract_parallel_assignment_fallback(&mut self, node: &Node, assignment_text: &str, parent_id: Option<String>, symbols: &mut Vec<Symbol>) {
        // Fallback method to extract variables from parallel assignments when tree structure is unexpected
        // Split by '=' to get left and right sides
        if let Some(eq_pos) = assignment_text.find('=') {
            let left_side = assignment_text[..eq_pos].trim();

            // Extract variable names from the left side
            let variables: Vec<&str> = left_side.split(',').map(|s| s.trim()).collect();

            for var in variables {
                let clean_var = var.trim_start_matches('*'); // Remove splat operator
                if !clean_var.is_empty() && clean_var.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    let symbol = self.base.create_symbol(
                        node,
                        clean_var.to_string(),
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(assignment_text.to_string()),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.clone(),
                            metadata: None,
                            doc_comment: None,
                        },
                    );
                    symbols.push(symbol);
                }
            }
        }
    }

    fn extract_variable(&mut self, node: Node) -> Symbol {
        let name = self.base.get_node_text(&node);
        let signature = name.clone();

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_constant(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name = self.base.get_node_text(&node);
        let signature = name.clone();

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_alias(&mut self, node: Node) -> Symbol {
        let signature = self.base.get_node_text(&node);
        let alias_name = self.extract_alias_name(node);

        self.base.create_symbol(
            &node,
            alias_name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    // Helper methods for building signatures and extracting names

    fn build_qualified_name(&self, node: Node, name: &str) -> String {
        let mut namespace_parts = Vec::new();
        let mut current = node;

        // Walk up the tree to find parent modules/classes
        while let Some(parent) = current.parent() {
            if matches!(parent.kind(), "module" | "class") {
                // Extract the name of the parent module/class
                if let Some(parent_name) = self.extract_name_from_node(parent, "name")
                    .or_else(|| self.extract_name_from_node(parent, "constant"))
                    .or_else(|| {
                        // Fallback: find first constant child
                        let mut cursor = parent.walk();
                        for child in parent.children(&mut cursor) {
                            if child.kind() == "constant" {
                                return Some(self.base.get_node_text(&child));
                            }
                        }
                        None
                    }) {
                    namespace_parts.push(parent_name);
                }
            }
            current = parent;
        }

        // Reverse to get the correct order (outermost first)
        namespace_parts.reverse();

        // If we have namespace parts, join them with ::
        if namespace_parts.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", namespace_parts.join("::"), name)
        }
    }

    fn extract_name_from_node(&self, node: Node, field_name: &str) -> Option<String> {
        node.child_by_field_name(field_name)
            .map(|name_node| self.base.get_node_text(&name_node))
    }

    fn build_module_signature(&self, node: &Node, name: &str) -> String {
        // Build namespace-aware module name
        let qualified_name = self.build_qualified_name(*node, name);
        let mut signature = format!("module {}", qualified_name);

        // Look for include/extend statements
        let includes = self.find_includes_and_extends(node);
        if !includes.is_empty() {
            signature.push_str(&format!("\n  {}", includes.join("\n  ")));
        }

        signature
    }

    fn build_class_signature(&self, node: &Node, name: &str) -> String {
        // Build namespace-aware class name
        let qualified_name = self.build_qualified_name(*node, name);
        let mut signature = format!("class {}", qualified_name);

        // Check for inheritance
        if let Some(superclass) = node.child_by_field_name("superclass") {
            let superclass_name = self.base.get_node_text(&superclass).replace('<', "").trim().to_string();
            signature.push_str(&format!(" < {}", superclass_name));
        }

        // Look for include/extend statements
        let includes = self.find_includes_and_extends(node);
        if !includes.is_empty() {
            signature.push_str(&format!("\n  {}", includes.join("\n  ")));
        }

        signature
    }

    fn build_method_signature(&self, node: &Node, name: &str) -> String {
        let mut signature = format!("def {}", name);

        // Try different field names for parameters
        if let Some(params) = node.child_by_field_name("parameters") {
            signature.push_str(&self.base.get_node_text(&params));
        } else if let Some(params) = node.child_by_field_name("method_parameters") {
            signature.push_str(&self.base.get_node_text(&params));
        } else {
            // Fallback: look for parameter list node
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "parameters" | "method_parameters" | "parameter_list") {
                    signature.push_str(&self.base.get_node_text(&child));
                    return signature;
                }
            }
            signature.push_str("()");
        }

        // Extract return statements from method body to include in signature
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_return_statements_from_body(&body, &mut signature);
        } else {
            // Fallback: look for method body in children
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "body_statement" || child.kind() == "block" {
                    self.extract_return_statements_from_body(&child, &mut signature);
                    break;
                }
            }
        }

        signature
    }

    fn extract_return_statements_from_body(&self, body_node: &Node, signature: &mut String) {
        let mut cursor = body_node.walk();
        for child in body_node.children(&mut cursor) {
            if child.kind() == "return" {
                let return_text = self.base.get_node_text(&child);
                if !signature.contains(&return_text) {
                    signature.push_str(&format!("\n  {}", return_text));
                }
            } else {
                // Recursively search for return statements in nested blocks
                self.extract_return_statements_from_body(&child, signature);
            }
        }
    }

    fn build_singleton_method_signature(&self, node: &Node, name: &str) -> String {
        let target = self.extract_singleton_method_target(*node);
        let mut signature = format!("def {}.{}", target, name);

        if let Some(params) = node.child_by_field_name("parameters") {
            signature.push_str(&self.base.get_node_text(&params));
        } else {
            signature.push_str("()");
        }

        signature
    }

    fn extract_singleton_method_name(&self, node: Node) -> String {
        // Ruby singleton method structure: def target.method_name
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" && child.prev_sibling().map_or(false, |s| s.kind() == ".") {
                return self.base.get_node_text(&child);
            }
        }
        "unknownMethod".to_string()
    }

    fn extract_singleton_method_target(&self, node: Node) -> String {
        // Find the target before the dot
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "self" {
                if child.next_sibling().map_or(false, |s| s.kind() == ".") {
                    return self.base.get_node_text(&child);
                }
            }
        }
        "self".to_string()
    }

    fn extract_method_name_from_call(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                return Some(self.base.get_node_text(&child));
            }
        }
        None
    }

    fn extract_require(&mut self, node: Node) -> Option<Symbol> {
        let arg_node = node.child_by_field_name("arguments")?;
        let string_node = arg_node.children(&mut arg_node.walk()).find(|c| c.kind() == "string")?;

        let require_path = self.base.get_node_text(&string_node).replace(['\'', '"'], "");
        let module_name = require_path.split('/').last().unwrap_or(&require_path).to_string();
        let method_name = self.extract_method_name_from_call(node)?;

        Some(self.base.create_symbol(
            &node,
            module_name,
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(format!("{} {}", method_name, self.base.get_node_text(&string_node))),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_attr_accessor(&mut self, node: Node, method_name: &str) -> Option<Symbol> {
        let arg_node = node.child_by_field_name("arguments")?;
        let symbol_nodes: Vec<_> = arg_node.children(&mut arg_node.walk())
            .filter(|c| matches!(c.kind(), "simple_symbol" | "symbol"))
            .collect();

        if let Some(first_symbol) = symbol_nodes.first() {
            let attr_name = self.base.get_node_text(first_symbol).replace(':', "");
            let signature = format!("{} :{}", method_name, attr_name);
            Some(self.base.create_symbol(
                &node,
                attr_name,
                SymbolKind::Property,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: None,
                    metadata: None,
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    fn extract_define_method(&mut self, node: Node, method_name: &str) -> Option<Symbol> {
        let arg_node = node.child_by_field_name("arguments")?;
        let name_node = arg_node.children(&mut arg_node.walk())
            .find(|c| matches!(c.kind(), "simple_symbol" | "symbol" | "string"))?;

        let dynamic_method_name = self.base.get_node_text(&name_node)
            .trim_start_matches(':')
            .trim_matches('"')
            .to_string();

        Some(self.base.create_symbol(
            &node,
            dynamic_method_name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(format!("{} {}", method_name, self.base.get_node_text(&name_node))),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_def_delegator(&mut self, node: Node) -> Option<Symbol> {
        let arg_node = node.child_by_field_name("arguments")?;
        let args: Vec<_> = arg_node.children(&mut arg_node.walk()).collect();

        if args.len() >= 2 {
            let method_arg = &args[1];
            let delegated_method_name = if matches!(method_arg.kind(), "simple_symbol" | "symbol") {
                self.base.get_node_text(method_arg).replace(':', "")
            } else {
                "delegated_method".to_string()
            };

            Some(self.base.create_symbol(
                &node,
                delegated_method_name,
                SymbolKind::Method,
                SymbolOptions {
                    signature: Some(format!("def_delegator {}", self.base.get_node_text(&arg_node))),
                    visibility: Some(Visibility::Public),
                    parent_id: None,
                    metadata: None,
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    fn extract_alias_name(&self, node: Node) -> String {
        // alias new_name old_name - extract the new_name
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        if children.len() >= 2 {
            self.base.get_node_text(&children[1])
        } else {
            "alias_method".to_string()
        }
    }

    fn find_includes_and_extends(&self, node: &Node) -> Vec<String> {
        let mut includes = Vec::new();

        self.find_includes_and_extends_recursive(*node, &mut includes);

        includes
    }

    fn find_includes_and_extends_recursive(&self, node: Node, includes: &mut Vec<String>) {
        // Check if this node itself is a call node for include/extend/prepend
        if node.kind() == "call" {
            if let Some(method_name) = self.extract_method_name_from_call(node) {
                if matches!(method_name.as_str(), "include" | "extend" | "prepend" | "using") {
                    includes.push(self.base.get_node_text(&node));
                }
            }
        }

        // Recursively search children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.find_includes_and_extends_recursive(child, includes);
        }
    }

    fn infer_symbol_kind_from_assignment(&self, left_node: &Node) -> SymbolKind {
        match left_node.kind() {
            "constant" => SymbolKind::Constant,
            "class_variable" | "instance_variable" | "global_variable" => SymbolKind::Variable,
            _ => {
                let text = self.base.get_node_text(left_node);
                if text.chars().all(|c| c.is_uppercase() || c == '_') {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Variable
                }
            }
        }
    }

    fn extract_relationships_from_node(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        match node.kind() {
            "class" => {
                self.extract_inheritance_relationship(node, symbols, relationships);
                self.extract_module_inclusion_relationships(node, symbols, relationships);
            }
            "module" => {
                self.extract_module_inclusion_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_relationships_from_node(child, symbols, relationships);
        }
    }

    fn extract_inheritance_relationship(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        if let Some(superclass_node) = node.child_by_field_name("superclass") {
            let class_name = self.extract_name_from_node(node, "name")
                .or_else(|| self.extract_name_from_node(node, "constant"))
                .or_else(|| {
                    // Fallback: find first constant child
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "constant" {
                            return Some(self.base.get_node_text(&child));
                        }
                    }
                    None
                })
                .unwrap_or_else(|| "UnknownClass".to_string());

            let superclass_name = self.base.get_node_text(&superclass_node).replace('<', "").trim().to_string();

            if let (Some(from_symbol), Some(to_symbol)) = (
                symbols.iter().find(|s| s.name == class_name),
                symbols.iter().find(|s| s.name == superclass_name),
            ) {
                relationships.push(Relationship {
                    id: format!("{}_{}_{:?}_{}", from_symbol.id, to_symbol.id, RelationshipKind::Extends, node.start_position().row),
                    from_symbol_id: from_symbol.id.clone(),
                    to_symbol_id: to_symbol.id.clone(),
                    kind: RelationshipKind::Extends,
                    file_path: self.base.file_path.clone(),
                    line_number: node.start_position().row as u32 + 1,
                    confidence: 1.0,
                    metadata: None,
                });
            }
        }
    }

    fn extract_module_inclusion_relationships(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        let class_or_module_name = self.extract_name_from_node(node, "name")
            .or_else(|| self.extract_name_from_node(node, "constant"))
            .or_else(|| {
                // Fallback: find first constant child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "constant" {
                        return Some(self.base.get_node_text(&child));
                    }
                }
                None
            })
            .unwrap_or_else(|| "Unknown".to_string());


        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "call" {
                // Direct call node
                self.process_include_extend_call(child, &class_or_module_name, symbols, relationships);
            } else if child.kind() == "body_statement" {
                // Call might be inside a body_statement
                let mut body_cursor = child.walk();
                for body_child in child.children(&mut body_cursor) {
                    if body_child.kind() == "call" {
                        self.process_include_extend_call(body_child, &class_or_module_name, symbols, relationships);
                    }
                }
            }
        }
    }

    fn process_include_extend_call(&self, child: tree_sitter::Node, class_or_module_name: &str, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        if let Some(method_name) = self.extract_method_name_from_call(child) {
            if matches!(method_name.as_str(), "include" | "extend" | "prepend" | "using") {
                if let Some(arg_node) = child.child_by_field_name("arguments") {
                    if let Some(module_node) = arg_node.children(&mut arg_node.walk()).next() {
                        let module_name = self.base.get_node_text(&module_node);

                        let from_symbol = symbols.iter().find(|s| s.name == class_or_module_name);
                        let to_symbol = symbols.iter().find(|s| s.name == module_name);

                        if let (Some(from_symbol), Some(to_symbol)) = (from_symbol, to_symbol) {
                            relationships.push(Relationship {
                                id: format!("{}_{}_{:?}_{}", from_symbol.id, to_symbol.id, RelationshipKind::Implements, child.start_position().row),
                                from_symbol_id: from_symbol.id.clone(),
                                to_symbol_id: to_symbol.id.clone(),
                                kind: RelationshipKind::Implements,
                                file_path: self.base.file_path.clone(),
                                line_number: child.start_position().row as u32 + 1,
                                confidence: 1.0,
                                metadata: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // Helper methods for checking node context
    #[allow(dead_code)]
    fn is_part_of_assignment(&self, node: &Node) -> bool {
        let mut current = *node;
        while let Some(parent) = current.parent() {
            if matches!(parent.kind(), "assignment" | "operator_assignment") {
                return true;
            }
            current = parent;
        }
        false
    }

    #[allow(dead_code)]
    fn is_part_of_class_module_declaration(&self, node: &Node) -> bool {
        let mut current = *node;
        while let Some(parent) = current.parent() {
            if matches!(parent.kind(), "class" | "module") {
                // Check if this constant is the name of the class/module
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if name_node.id() == current.id() {
                        return true;
                    }
                } else if let Some(name_node) = parent.child_by_field_name("constant") {
                    if name_node.id() == current.id() {
                        return true;
                    }
                } else {
                    // Fallback: check if this is the first constant child
                    let mut cursor = parent.walk();
                    for child in parent.children(&mut cursor) {
                        if child.kind() == "constant" && child.id() == current.id() {
                            return true;
                        }
                    }
                }
            }
            current = parent;
        }
        false
    }
}