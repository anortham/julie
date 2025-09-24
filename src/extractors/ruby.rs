use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, SymbolOptions, Visibility};
use tree_sitter::{Tree, Node};
use std::collections::HashMap;

/// Ruby extractor for extracting symbols and relationships from Ruby source code
/// Port of Miller's comprehensive Ruby extractor with metaprogramming support
pub struct RubyExtractor {
    base: BaseExtractor,
    current_visibility: String,
}

impl RubyExtractor {
    pub fn new(file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new("ruby".to_string(), file_path, content),
            current_visibility: "public".to_string(),
        }
    }

    /// Extract all symbols from Ruby source code
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.current_visibility = "public".to_string(); // Reset for each file
        self.traverse_tree(tree.root_node(), &mut symbols);
        symbols
    }

    /// Extract relationships between symbols (inheritance, module inclusion, etc.)
    pub fn extract_relationships(&self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.extract_relationships_from_node(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn traverse_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>) {
        match node.kind() {
            "module" => {
                let symbol = self.extract_module(node);
                symbols.push(symbol);
            }
            "class" => {
                let symbol = self.extract_class(node);
                symbols.push(symbol);
            }
            "singleton_class" => {
                let symbol = self.extract_singleton_class(node);
                symbols.push(symbol);
            }
            "method" => {
                let symbol = self.extract_method(node);
                symbols.push(symbol);
            }
            "singleton_method" => {
                let symbol = self.extract_singleton_method(node);
                symbols.push(symbol);
            }
            "call" => {
                if let Some(symbol) = self.extract_call(node) {
                    symbols.push(symbol);
                }
            }
            "assignment" | "operator_assignment" => {
                if let Some(symbol) = self.extract_assignment(node) {
                    symbols.push(symbol);
                }
            }
            "class_variable" | "instance_variable" | "global_variable" => {
                let symbol = self.extract_variable(node);
                symbols.push(symbol);
            }
            "constant" => {
                let symbol = self.extract_constant(node);
                symbols.push(symbol);
            }
            "alias" => {
                let symbol = self.extract_alias(node);
                symbols.push(symbol);
            }
            "identifier" => {
                // Handle visibility modifiers
                let text = self.base.get_node_text(&node);
                if matches!(text.as_str(), "private" | "protected" | "public") {
                    self.current_visibility = text;
                }
            }
            _ => {}
        }

        // Recursively traverse children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_tree(child, symbols);
        }
    }

    fn extract_module(&mut self, node: Node) -> Symbol {
        let name = self.extract_name_from_node(node, "constant").unwrap_or_else(|| "UnknownModule".to_string());
        let signature = self.build_module_signature(&node, &name);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Module,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_class(&mut self, node: Node) -> Symbol {
        let name = self.extract_name_from_node(node, "constant").unwrap_or_else(|| "UnknownClass".to_string());
        let signature = self.build_class_signature(&node, &name);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_singleton_class(&mut self, node: Node) -> Symbol {
        let signature = format!("class << self");

        self.base.create_symbol(
            &node,
            "SingletonClass".to_string(),
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_method(&mut self, node: Node) -> Symbol {
        let name = self.extract_name_from_node(node, "identifier").unwrap_or_else(|| "unknownMethod".to_string());
        let signature = self.build_method_signature(&node, &name);
        let kind = if name == "initialize" { SymbolKind::Constructor } else { SymbolKind::Method };

        let visibility = match self.current_visibility.as_str() {
            "private" => Visibility::Private,
            "protected" => Visibility::Protected,
            _ => Visibility::Public,
        };

        self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn extract_singleton_method(&mut self, node: Node) -> Symbol {
        let name = self.extract_singleton_method_name(node);
        let signature = self.build_singleton_method_signature(&node, &name);

        let visibility = match self.current_visibility.as_str() {
            "private" => Visibility::Private,
            "protected" => Visibility::Protected,
            _ => Visibility::Public,
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: None,
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

    fn extract_assignment(&mut self, node: Node) -> Option<Symbol> {
        // Handle various assignment patterns including parallel assignment
        let left_side = node.child_by_field_name("left")?;
        let right_side = node.child_by_field_name("right")?;

        let name = self.base.get_node_text(&left_side);
        let signature = format!("{} = {}", name, self.base.get_node_text(&right_side));

        let kind = self.infer_symbol_kind_from_assignment(&left_side);

        Some(self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        ))
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

    fn extract_constant(&mut self, node: Node) -> Symbol {
        let name = self.base.get_node_text(&node);
        let signature = name.clone();

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: None,
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

    fn extract_name_from_node(&self, node: Node, field_name: &str) -> Option<String> {
        node.child_by_field_name(field_name)
            .map(|name_node| self.base.get_node_text(&name_node))
    }

    fn build_module_signature(&self, node: &Node, name: &str) -> String {
        let mut signature = format!("module {}", name);

        // Look for include/extend statements
        let includes = self.find_includes_and_extends(node);
        if !includes.is_empty() {
            signature.push_str(&format!("\n  {}", includes.join("\n  ")));
        }

        signature
    }

    fn build_class_signature(&self, node: &Node, name: &str) -> String {
        let mut signature = format!("class {}", name);

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

        if let Some(params) = node.child_by_field_name("parameters") {
            signature.push_str(&self.base.get_node_text(&params));
        } else {
            signature.push_str("()");
        }

        signature
    }

    fn build_singleton_method_signature(&self, node: &Node, name: &str) -> String {
        let target = self.extract_singleton_method_target(node);
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
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "call" {
                if let Some(method_name) = self.extract_method_name_from_call(child) {
                    if matches!(method_name.as_str(), "include" | "extend" | "prepend") {
                        includes.push(self.base.get_node_text(&child));
                    }
                }
            }
        }

        includes
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
                .unwrap_or_else(|| "UnknownClass".to_string());
            let superclass_name = self.base.get_node_text(&superclass_node).replace('<', "").trim().to_string();

            if let (Some(from_symbol), Some(to_symbol)) = (
                symbols.iter().find(|s| s.name == class_name),
                symbols.iter().find(|s| s.name == superclass_name),
            ) {
                relationships.push(Relationship {
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
            .unwrap_or_else(|| "Unknown".to_string());

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "call" {
                if let Some(method_name) = self.extract_method_name_from_call(child) {
                    if matches!(method_name.as_str(), "include" | "extend" | "prepend") {
                        if let Some(arg_node) = child.child_by_field_name("arguments") {
                            if let Some(module_node) = arg_node.children(&mut arg_node.walk()).next() {
                                let module_name = self.base.get_node_text(&module_node);

                                if let (Some(from_symbol), Some(to_symbol)) = (
                                    symbols.iter().find(|s| s.name == class_or_module_name),
                                    symbols.iter().find(|s| s.name == module_name),
                                ) {
                                    relationships.push(Relationship {
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
        }
    }
}