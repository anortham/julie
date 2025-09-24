use crate::extractors::base::{BaseExtractor, Symbol, Relationship, SymbolKind, RelationshipKind, Visibility};
use tree_sitter::{Tree, Node};
use std::collections::HashMap;

/// Go language extractor that handles Go-specific constructs including:
/// - Structs, interfaces, and type aliases
/// - Functions and methods with receivers
/// - Packages and imports
/// - Constants and variables
/// - Goroutines and channels
/// - Interface implementations and embedding
pub struct GoExtractor {
    base: BaseExtractor,
}

impl GoExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    /// Extract symbols from Go source code - direct port from Miller's logic
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree(tree.root_node(), &mut symbols, None);

        // Prioritize functions over fields with the same name (Miller's logic)
        self.prioritize_functions_over_fields(symbols)
    }

    pub fn extract_relationships(&mut self, _tree: &Tree, _symbols: &[Symbol]) -> Vec<Relationship> {
        // Stub implementation - will be implemented after basic symbol extraction
        Vec::new()
    }

    pub fn infer_types(&self, _symbols: &[Symbol]) -> HashMap<String, String> {
        // Stub implementation - will be implemented after basic symbol extraction
        HashMap::new()
    }

    /// Prioritize functions over fields with the same name (direct port from Miller)
    fn prioritize_functions_over_fields(&self, symbols: Vec<Symbol>) -> Vec<Symbol> {
        let mut symbol_map: HashMap<String, Vec<Symbol>> = HashMap::new();

        // Group symbols by name
        for symbol in symbols {
            symbol_map.entry(symbol.name.clone()).or_default().push(symbol);
        }

        let mut result = Vec::new();

        // For each name group, add functions first, then other types
        for (_name, symbol_group) in symbol_map {
            let functions: Vec<Symbol> = symbol_group.iter()
                .filter(|s| s.kind == SymbolKind::Function || s.kind == SymbolKind::Method)
                .cloned()
                .collect();
            let others: Vec<Symbol> = symbol_group.iter()
                .filter(|s| s.kind != SymbolKind::Function && s.kind != SymbolKind::Method)
                .cloned()
                .collect();

            result.extend(functions);
            result.extend(others);
        }

        result
    }

    /// Walk the tree and extract symbols (port from Miller's walkTree method)
    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        // Handle declarations that can produce multiple symbols
        match node.kind() {
            "import_declaration" => {
                let import_symbols = self.extract_import_symbols(node, parent_id.as_deref());
                symbols.extend(import_symbols);
            }
            "var_declaration" => {
                let var_symbols = self.extract_var_symbols(node, parent_id.as_deref());
                symbols.extend(var_symbols);
            }
            "const_declaration" => {
                let const_symbols = self.extract_const_symbols(node, parent_id.as_deref());
                symbols.extend(const_symbols);
            }
            _ => {
                if let Some(symbol) = self.extract_symbol(node, parent_id.as_deref()) {
                    let symbol_id = symbol.id.clone();
                    symbols.push(symbol);

                    // Recursively walk children with the new parent_id
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        self.walk_tree(child, symbols, Some(symbol_id.clone()));
                    }
                    return;
                }
            }
        }

        // If no symbol was created, continue walking children with same parent_id
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, symbols, parent_id.clone());
        }
    }

    /// Extract symbol from node (port from Miller's extractSymbol method)
    fn extract_symbol(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        match node.kind() {
            "package_clause" => self.extract_package(node, parent_id),
            "type_declaration" => self.extract_type_declaration(node, parent_id),
            "function_declaration" => Some(self.extract_function(node, parent_id)),
            "method_declaration" => Some(self.extract_method(node, parent_id)),
            "field_declaration" => self.extract_field(node, parent_id),
            "ERROR" => self.extract_from_error_node(node, parent_id),
            _ => None,
        }
    }

    /// Check if identifier is public (Go visibility rules)
    fn is_public(&self, name: &str) -> bool {
        // In Go, identifiers starting with uppercase are public
        name.chars().next().map_or(false, |c| c.is_uppercase())
    }

    /// Get node text (helper method)
    fn get_node_text(&self, node: Node) -> String {
        self.base.get_node_text(&node)
    }

    // Stub implementations for specific extractors - will be implemented step by step
    fn extract_import_symbols(&mut self, _node: Node, _parent_id: Option<&str>) -> Vec<Symbol> {
        Vec::new()
    }

    fn extract_var_symbols(&mut self, _node: Node, _parent_id: Option<&str>) -> Vec<Symbol> {
        Vec::new()
    }

    fn extract_const_symbols(&mut self, _node: Node, _parent_id: Option<&str>) -> Vec<Symbol> {
        Vec::new()
    }

    fn extract_package(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Find package identifier node
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "package_identifier" {
                let name = self.get_node_text(child);
                let signature = format!("package {}", name);

                return Some(self.base.create_symbol(
                    &child,
                    name,
                    SymbolKind::Namespace,
                    Some(&signature),
                    Some(Visibility::Public),
                    parent_id.map(|s| s.to_string()),
                ));
            }
        }
        None
    }

    fn extract_type_declaration(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Find type_spec node which contains the actual type definition
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_spec" {
                return self.extract_type_spec(child, parent_id);
            }
        }
        None
    }

    fn extract_type_spec(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut type_identifier = None;
        let mut type_def = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => type_identifier = Some(child),
                "struct_type" => type_def = Some(("struct", child)),
                "interface_type" => type_def = Some(("interface", child)),
                "type_alias" => type_def = Some(("alias", child)),
                _ => {}
            }
        }

        if let (Some(type_id), Some((type_kind, type_node))) = (type_identifier, type_def) {
            let name = self.get_node_text(type_id);
            let visibility = if self.is_public(&name) {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            };

            match type_kind {
                "struct" => {
                    let signature = format!("type {} struct", name);
                    Some(self.base.create_symbol(
                        &type_id,
                        name,
                        SymbolKind::Class,
                        Some(&signature),
                        visibility,
                        parent_id.map(|s| s.to_string()),
                    ))
                },
                "interface" => {
                    let signature = format!("type {} interface", name);
                    Some(self.base.create_symbol(
                        &type_id,
                        name,
                        SymbolKind::Interface,
                        Some(&signature),
                        visibility,
                        parent_id.map(|s| s.to_string()),
                    ))
                },
                "alias" => {
                    // For type alias, extract the aliased type
                    let aliased_type = self.extract_type_from_node(type_node);
                    let signature = format!("type {} = {}", name, aliased_type);
                    Some(self.base.create_symbol(
                        &type_id,
                        name,
                        SymbolKind::Type,
                        Some(&signature),
                        visibility,
                        parent_id.map(|s| s.to_string()),
                    ))
                },
                _ => None,
            }
        } else {
            None
        }
    }

    fn extract_type_from_node(&self, node: Node) -> String {
        // Extract the type string from a type node
        match node.kind() {
            "primitive_type" => self.get_node_text(node),
            "map_type" => {
                let mut parts = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    parts.push(self.get_node_text(child));
                }
                parts.join("")
            },
            "slice_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "[" && child.kind() != "]" {
                        return format!("[]{}", self.extract_type_from_node(child));
                    }
                }
                self.get_node_text(node)
            },
            "pointer_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "*" {
                        return format!("*{}", self.extract_type_from_node(child));
                    }
                }
                self.get_node_text(node)
            },
            _ => self.get_node_text(node),
        }
    }

    fn extract_function(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let mut cursor = node.walk();
        let mut func_name = None;
        let mut parameters = Vec::new();
        let mut return_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => func_name = Some(self.get_node_text(child)),
                "parameter_list" => {
                    parameters = self.extract_parameter_list(child);
                },
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type" => {
                    return_type = Some(self.extract_type_from_node(child));
                },
                _ => {}
            }
        }

        let name = func_name.unwrap_or_else(|| "anonymous".to_string());
        let visibility = if name == "main" || name == "init" {
            Some(Visibility::Private) // Special Go functions
        } else if self.is_public(&name) {
            Some(Visibility::Public)
        } else {
            Some(Visibility::Private)
        };

        let signature = self.build_function_signature("func", &name, &parameters, return_type.as_deref());

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            Some(&signature),
            visibility,
            parent_id.map(|s| s.to_string()),
        )
    }

    fn extract_method(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let mut cursor = node.walk();
        let mut receiver = None;
        let mut func_name = None;
        let mut parameters = Vec::new();
        let mut return_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "parameter_list" => {
                    if receiver.is_none() {
                        // First parameter list is the receiver
                        let receiver_params = self.extract_parameter_list(child);
                        if !receiver_params.is_empty() {
                            receiver = Some(receiver_params[0].clone());
                        }
                    } else {
                        // Second parameter list is the actual parameters
                        parameters = self.extract_parameter_list(child);
                    }
                },
                "identifier" => func_name = Some(self.get_node_text(child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type" => {
                    return_type = Some(self.extract_type_from_node(child));
                },
                _ => {}
            }
        }

        let name = func_name.unwrap_or_else(|| "anonymous".to_string());
        let visibility = if self.is_public(&name) {
            Some(Visibility::Public)
        } else {
            Some(Visibility::Private)
        };

        let signature = if let Some(recv) = receiver {
            format!("func ({}) {}", recv, self.build_function_signature("", &name, &parameters, return_type.as_deref()).trim_start_matches("func "))
        } else {
            self.build_function_signature("func", &name, &parameters, return_type.as_deref())
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            Some(&signature),
            visibility,
            parent_id.map(|s| s.to_string()),
        )
    }

    fn extract_parameter_list(&self, node: Node) -> Vec<String> {
        let mut parameters = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                let param = self.extract_parameter_declaration(child);
                if !param.is_empty() {
                    parameters.push(param);
                }
            }
        }

        parameters
    }

    fn extract_parameter_declaration(&self, node: Node) -> String {
        let mut names = Vec::new();
        let mut param_type = None;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => names.push(self.get_node_text(child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type" | "map_type" => {
                    param_type = Some(self.extract_type_from_node(child));
                },
                _ => {}
            }
        }

        if let Some(typ) = param_type {
            if names.is_empty() {
                typ // Anonymous parameter
            } else {
                format!("{} {}", names.join(", "), typ)
            }
        } else if !names.is_empty() {
            names[0].clone() // Just the name if no type found
        } else {
            String::new()
        }
    }

    fn build_function_signature(&self, func_keyword: &str, name: &str, parameters: &[String], return_type: Option<&str>) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = return_type.map_or(String::new(), |t| format!(" {}", t));

        if func_keyword.is_empty() {
            format!("{}{}{}", name, params, return_part)
        } else {
            format!("{} {}{}{}", func_keyword, name, params, return_part)
        }
    }

    fn extract_field(&mut self, _node: Node, _parent_id: Option<&str>) -> Option<Symbol> {
        // Stub - will implement
        None
    }

    fn extract_from_error_node(&mut self, _node: Node, _parent_id: Option<&str>) -> Option<Symbol> {
        // Stub - will implement
        None
    }
}