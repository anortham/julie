use crate::extractors::base::{
    BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

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
        let symbol_map = self.build_symbol_map(symbols);

        // Extract relationships from the AST
        self.walk_tree_for_relationships(tree.root_node(), &symbol_map, &mut relationships);

        relationships
    }

    fn build_symbol_map<'a>(&self, symbols: &'a [Symbol]) -> HashMap<String, &'a Symbol> {
        let mut symbol_map = HashMap::new();
        for symbol in symbols {
            symbol_map.insert(symbol.name.clone(), symbol);
        }
        symbol_map
    }

    fn walk_tree_for_relationships(
        &self,
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

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_relationships(child, symbol_map, relationships);
        }
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            if let Some(signature) = &symbol.signature {
                // Extract type information from signatures
                match symbol.kind {
                    SymbolKind::Function | SymbolKind::Method => {
                        if let Some(return_type) =
                            self.extract_return_type_from_signature(signature)
                        {
                            types.insert(symbol.id.clone(), return_type);
                        }
                    }
                    SymbolKind::Variable | SymbolKind::Constant => {
                        if let Some(var_type) = self.extract_variable_type_from_signature(signature)
                        {
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
            symbol_map
                .entry(symbol.name.clone())
                .or_default()
                .push(symbol);
        }

        let mut result = Vec::new();

        // For each name group, add functions first, then other types
        for (_name, symbol_group) in symbol_map {
            let functions: Vec<Symbol> = symbol_group
                .iter()
                .filter(|s| s.kind == SymbolKind::Function || s.kind == SymbolKind::Method)
                .cloned()
                .collect();
            let others: Vec<Symbol> = symbol_group
                .iter()
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
                            if let Some(symbol) = self.extract_import_spec(nested_child, parent_id)
                            {
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
        // Find type_spec or type_alias node which contains the actual type definition
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_spec" {
                return self.extract_type_spec(child, parent_id);
            } else if child.kind() == "type_alias" {
                return self.extract_type_alias(child, parent_id);
            }
        }
        None
    }

    fn extract_type_spec(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut type_identifier = None;
        let mut type_parameters = None;
        let mut type_def = None;
        let mut second_type_identifier = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" if type_identifier.is_none() => type_identifier = Some(child),
                "type_identifier"
                    if type_identifier.is_some() && second_type_identifier.is_none() =>
                {
                    // Second type_identifier indicates type alias (type Alias = Target)
                    second_type_identifier = Some(child);
                    type_def = Some(("alias", child));
                }
                "type_parameter_list" => type_parameters = Some(child),
                "struct_type" => type_def = Some(("struct", child)),
                "interface_type" => type_def = Some(("interface", child)),
                "=" => {} // Type alias syntax detected (handled by second_type_identifier)
                // Handle basic type definitions (type UserID int64) and aliases (type UserID = int64)
                "primitive_type" if type_identifier.is_some() && type_def.is_none() => {
                    type_def = Some(("definition", child));
                }
                "pointer_type" | "slice_type" | "map_type" | "array_type" | "channel_type"
                | "function_type" | "qualified_type" | "generic_type"
                    if type_identifier.is_some() && type_def.is_none() =>
                {
                    type_def = Some(("definition", child));
                }
                _ => {}
            }
        }

        if let (Some(type_id), Some((type_kind, type_node))) = (type_identifier, type_def) {
            let name = self.get_node_text(type_id);
            let type_params = type_parameters
                .map(|tp| self.get_node_text(tp))
                .unwrap_or_default();

            let visibility = if self.is_public(&name) {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            };

            match type_kind {
                "struct" => {
                    let signature = format!("type {}{} struct", name, type_params);
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
                }
                "interface" => {
                    let mut signature = format!("type {}{} interface", name, type_params);

                    // Extract interface body content for union types and methods
                    let interface_body = (&*self).extract_interface_body(type_node);
                    if !interface_body.is_empty() {
                        signature += &format!(" {{ {} }}", interface_body);
                    }

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
                }
                "alias" => {
                    // For type alias, extract the aliased type
                    let aliased_type = self.extract_type_from_node(type_node);
                    let signature = format!("type {}{} = {}", name, type_params, aliased_type);
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
                }
                "definition" => {
                    // For type definition (no equals sign) - Miller formats these like aliases
                    let aliased_type = self.extract_type_from_node(type_node);
                    let signature = format!("type {}{} = {}", name, type_params, aliased_type);
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
                }
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
            }
            "slice_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "[" && child.kind() != "]" {
                        return format!("[]{}", self.extract_type_from_node(child));
                    }
                }
                self.get_node_text(node)
            }
            "array_type" => self.get_node_text(node),
            "pointer_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "*" {
                        return format!("*{}", self.extract_type_from_node(child));
                    }
                }
                self.get_node_text(node)
            }
            "channel_type" => {
                // Handle channel types like <-chan, chan<-, chan
                self.get_node_text(node)
            }
            "interface_type" => {
                // Handle interface{} and other interface types
                self.get_node_text(node)
            }
            "function_type" => {
                // Handle function types like func(int) string
                self.get_node_text(node)
            }
            "qualified_type" => {
                // Handle types like package.TypeName
                self.get_node_text(node)
            }
            "generic_type" => {
                // Handle generic types like Stack[T]
                self.get_node_text(node)
            }
            "type_arguments" => {
                // Handle type arguments like [T, U]
                self.get_node_text(node)
            }
            _ => self.get_node_text(node),
        }
    }

    fn extract_function(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let mut cursor = node.walk();
        let mut func_name = None;
        let mut type_parameters = None;
        let mut parameters = Vec::new();
        let mut return_type = None;
        let mut param_list_found = false;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => func_name = Some(self.get_node_text(child)),
                "type_parameter_list" => type_parameters = Some(self.get_node_text(child)),
                "parameter_list" => {
                    parameters = self.extract_parameter_list(child);
                    param_list_found = true;
                }
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "channel_type" | "interface_type" | "function_type" | "map_type"
                | "array_type" | "qualified_type" | "generic_type" => {
                    // Only treat as return type if we've seen parameters already
                    if param_list_found {
                        return_type = Some(self.extract_type_from_node(child));
                    }
                }
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

        let type_params = type_parameters.unwrap_or_default();
        let signature = self.build_function_signature_with_generics(
            "func",
            &name,
            &type_params,
            &parameters,
            return_type.as_deref(),
        );

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
        let mut type_parameters = None;
        let mut parameters = Vec::new();
        let mut return_types = Vec::new();
        let mut param_lists_found = 0;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "parameter_list" => {
                    param_lists_found += 1;
                    if param_lists_found == 1 {
                        // First parameter list is the receiver
                        let receiver_params = self.extract_parameter_list(child);
                        if !receiver_params.is_empty() {
                            receiver = Some(receiver_params[0].clone());
                        }
                    } else if param_lists_found == 2 {
                        // Second parameter list is the actual parameters
                        parameters = self.extract_parameter_list(child);
                    } else if param_lists_found == 3 {
                        // Third parameter list is the return types (Go methods can have 3 parameter lists)
                        return_types = self.extract_parameter_list(child);
                    }
                }
                "field_identifier" => func_name = Some(self.get_node_text(child)), // Miller uses field_identifier for method names
                "type_parameter_list" => type_parameters = Some(self.get_node_text(child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "channel_type" | "interface_type" | "function_type" | "map_type"
                | "array_type" | "qualified_type" | "generic_type" => {
                    // Only treat as return type if we've seen parameters already
                    if param_lists_found >= 2 {
                        return_types.push(self.extract_type_from_node(child));
                    }
                }
                _ => {}
            }
        }

        let name = func_name.unwrap_or_else(|| "anonymous".to_string());
        let visibility = if self.is_public(&name) {
            Some(Visibility::Public)
        } else {
            Some(Visibility::Private)
        };

        let type_params = type_parameters.unwrap_or_default();

        let signature = if let Some(recv) = receiver {
            format!(
                "func ({}) {}{}",
                recv,
                name,
                self.build_method_signature_with_return_types(
                    &type_params,
                    &parameters,
                    &return_types
                )
            )
        } else {
            self.build_function_signature_with_return_types(
                "func",
                &name,
                &parameters,
                &return_types,
            )
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
            match child.kind() {
                "parameter_declaration" | "variadic_parameter_declaration" => {
                    let param = self.extract_parameter_declaration(child);
                    if !param.is_empty() {
                        parameters.push(param);
                    }
                }
                _ => {}
            }
        }

        parameters
    }

    fn extract_parameter_declaration(&self, node: Node) -> String {
        // Handle variadic parameter declarations
        if node.kind() == "variadic_parameter_declaration" {
            return self.get_node_text(node);
        }

        let mut names = Vec::new();
        let mut param_type = None;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => names.push(self.get_node_text(child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "map_type" | "channel_type" | "interface_type" | "function_type"
                | "array_type" | "qualified_type" | "generic_type" => {
                    param_type = Some(self.extract_type_from_node(child));
                }
                "variadic_parameter" => {
                    // Handle variadic parameters like ...interface{}
                    let variadic_text = self.get_node_text(child);
                    param_type = Some(variadic_text);
                }
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

    #[allow(dead_code)]
    fn build_function_signature(
        &self,
        func_keyword: &str,
        name: &str,
        parameters: &[String],
        return_type: Option<&str>,
    ) -> String {
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

    fn build_function_signature_with_generics(
        &self,
        func_keyword: &str,
        name: &str,
        type_params: &str,
        parameters: &[String],
        return_type: Option<&str>,
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = return_type.map_or(String::new(), |t| format!(" {}", t));

        if func_keyword.is_empty() {
            format!("{}{}{}{}", name, type_params, params, return_part)
        } else {
            format!(
                "{} {}{}{}{}",
                func_keyword, name, type_params, params, return_part
            )
        }
    }

    #[allow(dead_code)]
    fn build_method_signature(
        &self,
        type_params: &str,
        parameters: &[String],
        return_type: Option<&str>,
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = return_type.map_or(String::new(), |t| format!(" {}", t));
        format!("{}{}{}", type_params, params, return_part)
    }

    fn build_method_signature_with_return_types(
        &self,
        type_params: &str,
        parameters: &[String],
        return_types: &[String],
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = match return_types.len() {
            0 => String::new(),
            1 => format!(" {}", return_types[0]),
            _ => format!(" ({})", return_types.join(", ")),
        };

        format!("{}{}{}", type_params, params, return_part)
    }

    fn build_function_signature_with_return_types(
        &self,
        func_keyword: &str,
        name: &str,
        parameters: &[String],
        return_types: &[String],
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = match return_types.len() {
            0 => String::new(),
            1 => format!(" {}", return_types[0]),
            _ => format!(" ({})", return_types.join(", ")),
        };

        if func_keyword.is_empty() {
            format!("{}{}{}", name, params, return_part)
        } else {
            format!("{} {}{}{}", func_keyword, name, params, return_part)
        }
    }

    fn extract_interface_body(&self, interface_node: Node) -> String {
        let mut body_parts = Vec::new();
        let mut cursor = interface_node.walk();

        for child in interface_node.children(&mut cursor) {
            if child.kind() == "type_elem" {
                body_parts.push(self.get_node_text(child));
            }
        }

        body_parts.join("; ")
    }

    fn extract_field(&mut self, _node: Node, _parent_id: Option<&str>) -> Option<Symbol> {
        // Stub - will implement
        None
    }

    fn extract_from_error_node(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Try to recover function signatures from ERROR nodes
        // Look for identifier + parenthesized_type pattern (function signature)
        let mut cursor = node.walk();
        let mut identifier = None;
        let mut param_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => identifier = Some(child),
                "parenthesized_type" => param_type = Some(child),
                _ => {}
            }
        }

        if let (Some(id_node), Some(param_node)) = (identifier, param_type) {
            let name = self.get_node_text(id_node);
            let params = self.get_node_text(param_node);

            // This looks like a function signature trapped in an ERROR node
            let signature = format!("func {}{}", name, params);

            return Some(self.base.create_symbol(
                &node,
                name.clone(),
                SymbolKind::Function,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: if self.is_public(&name) {
                        Some(Visibility::Public)
                    } else {
                        Some(Visibility::Private)
                    },
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment: None,
                },
            ));
        }

        None
    }

    // Helper methods for specific Go constructs
    fn extract_import_spec(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let mut alias = None;
        let mut path = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "package_identifier" => alias = Some(self.get_node_text(child)), // Miller uses package_identifier for alias
                "interpreted_string_literal" => path = Some(self.get_node_text(child)),
                _ => {}
            }
        }

        if let Some(import_path) = path {
            // Skip blank imports (_)
            if alias.as_deref() == Some("_") {
                return None;
            }

            // Extract package name from path
            let package_name = if let Some(ref a) = alias {
                a.clone()
            } else {
                // Extract package name from import path
                import_path
                    .trim_matches('"')
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
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "map_type" => {
                    var_type = Some(self.extract_type_from_node(child));
                }
                "expression_list" => {
                    // Extract the first expression as the value
                    let mut expr_cursor = child.walk();
                    for expr_child in child.children(&mut expr_cursor) {
                        if !matches!(expr_child.kind(), "," | " ") {
                            value = Some(self.get_node_text(expr_child));
                            break;
                        }
                    }
                }
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
                }
                "expression_list" => {
                    // Extract the first expression as the value
                    let mut expr_cursor = child.walk();
                    for expr_child in child.children(&mut expr_cursor) {
                        if !matches!(expr_child.kind(), "," | " ") {
                            value = Some(self.get_node_text(expr_child));
                            break;
                        }
                    }
                }
                _ if child.kind().starts_with("literal")
                    || matches!(child.kind(), "true" | "false" | "nil") =>
                {
                    value = Some(self.get_node_text(child));
                }
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

    fn extract_method_relationships_from_node(
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

    fn extract_receiver_type_from_param(&self, param_decl: Node) -> String {
        // Extract type from receiver parameter (handle *Type and Type)
        let mut cursor = param_decl.walk();
        for child in param_decl.children(&mut cursor) {
            if child.kind() == "type_identifier" {
                return self.get_node_text(child);
            } else if child.kind() == "pointer_type" {
                // Handle pointer types like *User
                let type_id = child
                    .children(&mut child.walk())
                    .find(|c| c.kind() == "type_identifier");
                return type_id
                    .map(|tid| self.get_node_text(tid))
                    .unwrap_or_default();
            }
        }
        String::new()
    }

    fn extract_embedding_relationships(
        &self,
        _node: Node,
        _symbol_map: &HashMap<String, &Symbol>,
        _relationships: &mut Vec<Relationship>,
    ) {
        // Go struct embedding creates implicit relationships
        // This would need more complex parsing to detect embedded types
        // For now, we'll skip this advanced feature
    }

    fn extract_return_type_from_signature(&self, signature: &str) -> Option<String> {
        // Extract return type from function signatures like "func getName() string"
        if let Some(paren_end) = signature.rfind(')') {
            let after_paren = signature[paren_end + 1..].trim();
            if !after_paren.is_empty() && after_paren != "{" {
                return Some(
                    after_paren
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_string(),
                );
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

    fn extract_type_alias(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Parse type_alias node: "TypeAlias = string"
        let mut cursor = node.walk();
        let mut alias_name = None;
        let mut target_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" if alias_name.is_none() => alias_name = Some(child),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "map_type" | "array_type" | "channel_type" | "function_type"
                | "qualified_type" | "generic_type"
                    if alias_name.is_some() =>
                {
                    target_type = Some(child);
                }
                _ => {}
            }
        }

        if let (Some(alias_node), Some(target_node)) = (alias_name, target_type) {
            let name = self.get_node_text(alias_node);
            let target_type_text = self.extract_type_from_node(target_node);
            let signature = format!("type {} = {}", name, target_type_text);

            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "alias_target".to_string(),
                serde_json::Value::String(target_type_text),
            );

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Type,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some(metadata),
                    doc_comment: None,
                },
            ));
        }

        None
    }
}
