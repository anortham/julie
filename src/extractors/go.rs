use crate::extractors::base::{BaseExtractor, Symbol, Relationship, SymbolKind, RelationshipKind, Visibility, SymbolOptions};
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

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        // Basic implementation for method-receiver relationships
        self.extract_method_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            if let Some(signature) = &symbol.signature {
                // Extract type information from signatures
                match symbol.kind {
                    SymbolKind::Function | SymbolKind::Method => {
                        if let Some(return_type) = self.extract_return_type_from_signature(signature) {
                            types.insert(symbol.id.clone(), return_type);
                        }
                    }
                    SymbolKind::Variable | SymbolKind::Constant => {
                        if let Some(var_type) = self.extract_variable_type_from_signature(signature) {
                            types.insert(symbol.id.clone(), var_type);
                        }
                    }
                    _ => {}
                }
            }
        }

        types
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

    fn extract_import_symbols(&mut self, node: Node, parent_id: Option<&str>) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_spec" => {
                    if let Some(symbol) = self.extract_import_spec(child, parent_id) {
                        symbols.push(symbol);
                    }
                }
                "import_spec_list" => {
                    let mut nested_cursor = child.walk();
                    for nested_child in child.children(&mut nested_cursor) {
                        if nested_child.kind() == "import_spec" {
                            if let Some(symbol) = self.extract_import_spec(nested_child, parent_id) {
                                symbols.push(symbol);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
    }

    fn extract_var_symbols(&mut self, node: Node, parent_id: Option<&str>) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "var_spec" => {
                    if let Some(symbol) = self.extract_var_spec(child, parent_id) {
                        symbols.push(symbol);
                    }
                }
                "var_spec_list" => {
                    let mut nested_cursor = child.walk();
                    for nested_child in child.children(&mut nested_cursor) {
                        if nested_child.kind() == "var_spec" {
                            if let Some(symbol) = self.extract_var_spec(nested_child, parent_id) {
                                symbols.push(symbol);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
    }

    fn extract_const_symbols(&mut self, node: Node, parent_id: Option<&str>) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "const_spec" => {
                    if let Some(symbol) = self.extract_const_spec(child, parent_id) {
                        symbols.push(symbol);
                    }
                }
                "const_spec_list" => {
                    let mut nested_cursor = child.walk();
                    for nested_child in child.children(&mut nested_cursor) {
                        if nested_child.kind() == "const_spec" {
                            if let Some(symbol) = self.extract_const_spec(nested_child, parent_id) {
                                symbols.push(symbol);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
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
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: None,
                    },
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
                // Handle basic type definitions (type UserID int64)
                "primitive_type" | "type_identifier" if type_identifier.is_some() && type_def.is_none() => {
                    type_def = Some(("alias", child));
                },
                "pointer_type" | "slice_type" | "map_type" | "array_type" | "channel_type" if type_identifier.is_some() && type_def.is_none() => {
                    type_def = Some(("alias", child));
                },
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
                        SymbolOptions {
                            signature: Some(signature),
                            visibility,
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: None,
                        },
                    ))
                },
                "interface" => {
                    let signature = format!("type {} interface", name);
                    Some(self.base.create_symbol(
                        &type_id,
                        name,
                        SymbolKind::Interface,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility,
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: None,
                        },
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
                        SymbolOptions {
                            signature: Some(signature),
                            visibility,
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: None,
                        },
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
            "type_identifier" | "primitive_type" => self.get_node_text(node),
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
            "array_type" => self.get_node_text(node),
            "pointer_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "*" {
                        return format!("*{}", self.extract_type_from_node(child));
                    }
                }
                self.get_node_text(node)
            },
            "channel_type" => {
                // Handle channel types like <-chan, chan<-, chan
                self.get_node_text(node)
            },
            "interface_type" => {
                // Handle interface{} and other interface types
                self.get_node_text(node)
            },
            "function_type" => {
                // Handle function types like func(int) string
                self.get_node_text(node)
            },
            "qualified_type" => {
                // Handle types like package.TypeName
                self.get_node_text(node)
            },
            "generic_type" => {
                // Handle generic types like Stack[T]
                self.get_node_text(node)
            },
            "type_arguments" => {
                // Handle type arguments like [T, U]
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
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type" | "channel_type" | "interface_type" | "function_type" | "map_type" | "array_type" | "qualified_type" | "generic_type" => {
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
            SymbolOptions {
                signature: Some(signature),
                visibility,
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: None,
            },
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
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type" | "channel_type" | "interface_type" | "function_type" | "map_type" | "array_type" | "qualified_type" | "generic_type" => {
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
            SymbolOptions {
                signature: Some(signature),
                visibility,
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: None,
            },
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
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type" | "map_type" | "channel_type" | "interface_type" | "function_type" | "array_type" | "qualified_type" | "generic_type" => {
                    param_type = Some(self.extract_type_from_node(child));
                },
                "variadic_parameter" => {
                    // Handle variadic parameters like ...interface{}
                    let variadic_text = self.get_node_text(child);
                    param_type = Some(variadic_text);
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

    // Helper methods for specific Go constructs
    fn extract_import_spec(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut alias = None;
        let mut path = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => alias = Some(self.get_node_text(child)),
                "interpreted_string_literal" => path = Some(self.get_node_text(child)),
                _ => {}
            }
        }

        if let Some(import_path) = path {
            // Extract package name from path
            let package_name = if let Some(ref a) = alias {
                a.clone()
            } else {
                // Extract package name from import path
                import_path.trim_matches('"')
                    .split('/')
                    .last()
                    .unwrap_or("unknown")
                    .to_string()
            };

            let signature = if let Some(ref a) = alias {
                format!("import {} {}", a, import_path)
            } else {
                format!("import {}", import_path)
            };

            Some(self.base.create_symbol(
                &node,
                package_name,
                SymbolKind::Import,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    fn extract_var_spec(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut identifier = None;
        let mut var_type = None;
        let mut value = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => identifier = Some(self.get_node_text(child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type" | "map_type" => {
                    var_type = Some(self.extract_type_from_node(child));
                },
                "expression_list" => {
                    // Extract the first expression as the value
                    let mut expr_cursor = child.walk();
                    for expr_child in child.children(&mut expr_cursor) {
                        if !matches!(expr_child.kind(), "," | " ") {
                            value = Some(self.get_node_text(expr_child));
                            break;
                        }
                    }
                },
                _ => {}
            }
        }

        if let Some(name) = identifier {
            let visibility = if self.is_public(&name) {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            };

            let signature = if let Some(typ) = var_type {
                if let Some(val) = value {
                    format!("var {} {} = {}", name, typ, val)
                } else {
                    format!("var {} {}", name, typ)
                }
            } else if let Some(val) = value {
                format!("var {} = {}", name, val)
            } else {
                format!("var {}", name)
            };

            Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(signature),
                    visibility,
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    fn extract_const_spec(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut identifier = None;
        let mut const_type = None;
        let mut value = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => identifier = Some(self.get_node_text(child)),
                "type_identifier" | "primitive_type" => {
                    const_type = Some(self.extract_type_from_node(child));
                },
                "expression_list" => {
                    // Extract the first expression as the value
                    let mut expr_cursor = child.walk();
                    for expr_child in child.children(&mut expr_cursor) {
                        if !matches!(expr_child.kind(), "," | " ") {
                            value = Some(self.get_node_text(expr_child));
                            break;
                        }
                    }
                },
                _ if child.kind().starts_with("literal") || matches!(child.kind(), "true" | "false" | "nil") => {
                    value = Some(self.get_node_text(child));
                },
                _ => {}
            }
        }

        if let Some(name) = identifier {
            let visibility = if self.is_public(&name) {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            };

            let signature = if let Some(val) = value {
                if let Some(typ) = const_type {
                    format!("const {} {} = {}", name, typ, val)
                } else {
                    format!("const {} = {}", name, val)
                }
            } else {
                format!("const {}", name)
            };

            Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Constant,
                SymbolOptions {
                    signature: Some(signature),
                    visibility,
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    // Additional helper methods for relationships and type inference
    fn extract_method_relationships(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "method_declaration" => {
                    // Find receiver and method relationships
                    if let Some(receiver_type) = self.extract_receiver_type(child) {
                        // Find corresponding struct/interface
                        if let Some(struct_symbol) = symbols.iter().find(|s| s.name == receiver_type && s.kind == SymbolKind::Class) {
                            if let Some(method_symbol) = symbols.iter().find(|s| self.is_method_of_node(s, child)) {
                                // Create method-receiver relationship (placeholder)
                                // This would require proper relationship creation with RelationshipKind
                            }
                        }
                    }
                }
                _ => {
                    // Recursively process children
                    self.extract_method_relationships(child, symbols, relationships);
                }
            }
        }
    }

    fn extract_receiver_type(&self, method_node: Node) -> Option<String> {
        let mut cursor = method_node.walk();
        for child in method_node.children(&mut cursor) {
            if child.kind() == "parameter_list" {
                // First parameter list is the receiver
                let params = self.extract_parameter_list(child);
                if !params.is_empty() {
                    // Extract type from receiver parameter
                    let receiver = &params[0];
                    if let Some(type_start) = receiver.rfind(' ') {
                        let receiver_type = &receiver[type_start + 1..];
                        return Some(receiver_type.trim_start_matches('*').to_string());
                    }
                }
                break;
            }
        }
        None
    }

    fn is_method_of_node(&self, symbol: &Symbol, node: Node) -> bool {
        // Simple check - in a real implementation, we'd compare node positions
        symbol.kind == SymbolKind::Method
    }

    fn extract_return_type_from_signature(&self, signature: &str) -> Option<String> {
        // Extract return type from function signatures like "func getName() string"
        if let Some(paren_end) = signature.rfind(')') {
            let after_paren = signature[paren_end + 1..].trim();
            if !after_paren.is_empty() && after_paren != "{" {
                return Some(after_paren.split_whitespace().next().unwrap_or("").to_string());
            }
        }
        None
    }

    fn extract_variable_type_from_signature(&self, signature: &str) -> Option<String> {
        // Extract type from variable signatures like "var name string = value"
        if signature.starts_with("var ") {
            let parts: Vec<&str> = signature.split_whitespace().collect();
            if parts.len() >= 3 {
                let potential_type = parts[2];
                if potential_type != "=" {
                    return Some(potential_type.to_string());
                }
            }
        } else if signature.starts_with("const ") {
            let parts: Vec<&str> = signature.split_whitespace().collect();
            if parts.len() >= 3 {
                let potential_type = parts[2];
                if potential_type != "=" {
                    return Some(potential_type.to_string());
                }
            }
        }
        None
    }
}