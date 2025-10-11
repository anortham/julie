// Lua Extractor Implementation
//
// Port of Miller's Lua extractor with idiomatic Rust patterns
// Original: /Users/murphy/Source/miller/src/extractors/lua-extractor.ts

use crate::extractors::base::{
    BaseExtractor, Identifier, IdentifierKind, Relationship, Symbol, SymbolKind, SymbolOptions,
    Visibility,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use tree_sitter::{Node, Tree};

// Static regex compiled once for performance
static SETMETATABLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"setmetatable\(\s*\{\s*\}\s*,\s*(\w+)\s*\)").unwrap());

pub struct LuaExtractor {
    base: BaseExtractor,
    symbols: Vec<Symbol>,
    relationships: Vec<Relationship>,
}

impl LuaExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
            symbols: Vec::new(),
            relationships: Vec::new(),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        self.symbols.clear();
        self.relationships.clear();

        // Traverse the root node directly - it contains the statements we need
        self.traverse_node(tree.root_node(), None);

        // Post-process to detect Lua class patterns
        self.detect_lua_classes();

        self.symbols.clone()
    }

    pub fn extract_relationships(
        &mut self,
        _tree: &Tree,
        _symbols: &[Symbol],
    ) -> Vec<Relationship> {
        self.relationships.clone()
    }

    fn traverse_node(&mut self, node: Node, parent_id: Option<String>) {
        let mut symbol: Option<Symbol> = None;

        match node.kind() {
            "function_definition_statement" | "function_declaration" => {
                symbol = self.extract_function_definition_statement(node, parent_id.as_deref());
            }
            "local_function_definition_statement" | "local_function_declaration" => {
                symbol =
                    self.extract_local_function_definition_statement(node, parent_id.as_deref());
            }
            "local_variable_declaration" | "variable_declaration" => {
                symbol = self.extract_local_variable_declaration(node, parent_id.as_deref());
            }
            "assignment_statement" => {
                symbol = self.extract_assignment_statement(node, parent_id.as_deref());
            }
            "variable_assignment" => {
                symbol = self.extract_variable_assignment(node, parent_id.as_deref());
            }
            "table_constructor" | "table" => {
                // Table constructors can contain fields that should be extracted as child symbols
                self.extract_table_fields(node, parent_id.as_deref());
                return; // Table constructor itself doesn't create a symbol, just its fields
            }
            _ => {}
        }

        // Traverse children with current symbol as parent
        let current_parent_id = symbol.as_ref().map(|s| s.id.clone()).or(parent_id);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_node(child, current_parent_id.clone());
        }
    }

    fn extract_function_definition_statement(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Handle both regular functions and colon syntax methods
        let mut name_node = self.find_child_by_type(node, "identifier");
        let name: String;
        let mut kind = SymbolKind::Function;
        let mut method_parent_id = parent_id.map(|s| s.to_string());

        if let Some(name_n) = name_node {
            name = self.base.get_node_text(&name_n);
        } else {
            // Check for colon syntax: function obj:method() or dot syntax: function obj.method()
            if let Some(variable_node) = self
                .find_child_by_type(node, "variable")
                .or_else(|| self.find_child_by_type(node, "dot_index_expression"))
                .or_else(|| self.find_child_by_type(node, "method_index_expression"))
            {
                let full_name = self.base.get_node_text(&variable_node);

                // Handle colon syntax: function obj:method()
                if full_name.contains(':') {
                    let parts: Vec<&str> = full_name.split(':').collect();
                    if parts.len() == 2 {
                        let object_name = parts[0];
                        let method_name = parts[1];
                        name = method_name.to_string();
                        name_node = Some(variable_node);
                        kind = SymbolKind::Method;

                        // Try to find the object this method belongs to
                        if let Some(object_symbol) =
                            self.symbols.iter().find(|s| s.name == object_name)
                        {
                            method_parent_id = Some(object_symbol.id.clone());
                        }
                    } else {
                        return None;
                    }
                }
                // Handle dot syntax: function obj.method()
                else if full_name.contains('.') {
                    let parts: Vec<&str> = full_name.split('.').collect();
                    if parts.len() == 2 {
                        let object_name = parts[0];
                        let method_name = parts[1];
                        name = method_name.to_string();
                        name_node = Some(variable_node);
                        kind = SymbolKind::Method;

                        // Try to find the object this method belongs to
                        if let Some(object_symbol) =
                            self.symbols.iter().find(|s| s.name == object_name)
                        {
                            method_parent_id = Some(object_symbol.id.clone());
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }

        let signature = self.base.get_node_text(&node);

        // Determine visibility: check if function is local (contains "local" keyword) or uses underscore prefix
        let node_text = self.base.get_node_text(&node);
        let is_local = node_text.trim_start().starts_with("local function");
        let has_underscore = name.starts_with('_');
        let visibility = if is_local || has_underscore {
            Visibility::Private
        } else {
            Visibility::Public
        };

        let options = SymbolOptions {
            signature: Some(signature),
            parent_id: method_parent_id,
            visibility: Some(visibility),
            ..Default::default()
        };

        let symbol = self
            .base
            .create_symbol(&name_node.unwrap_or(node), name, kind, options);
        self.symbols.push(symbol.clone());
        Some(symbol)
    }

    fn extract_local_function_definition_statement(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let name_node = self.find_child_by_type(node, "identifier")?;
        let name = self.base.get_node_text(&name_node);
        let signature = self.base.get_node_text(&node);

        // Local functions are always private (regardless of underscore prefix)
        let options = SymbolOptions {
            signature: Some(signature),
            parent_id: parent_id.map(|s| s.to_string()),
            visibility: Some(Visibility::Private),
            ..Default::default()
        };

        let symbol = self
            .base
            .create_symbol(&name_node, name, SymbolKind::Function, options);
        self.symbols.push(symbol.clone());
        Some(symbol)
    }

    fn extract_local_variable_declaration(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Get the assignment_statement child first
        let assignment_statement = self.find_child_by_type(node, "assignment_statement")?;

        // Now get variable_list and expression_list from assignment_statement
        let variable_list = self.find_child_by_type(assignment_statement, "variable_list")?;
        let expression_list = self.find_child_by_type(assignment_statement, "expression_list");

        let signature = self.base.get_node_text(&node);
        let mut cursor = variable_list.walk();
        let variables: Vec<Node> = variable_list
            .children(&mut cursor)
            .filter(|child| child.kind() == "variable" || child.kind() == "identifier")
            .collect();

        // Get the corresponding expressions if they exist
        let expressions: Vec<Node> = if let Some(expr_list) = expression_list {
            let mut expr_cursor = expr_list.walk();
            expr_list
                .children(&mut expr_cursor)
                .filter(|child| child.kind() != ",") // Filter out commas
                .collect()
        } else {
            Vec::new()
        };

        // Create symbols for each local variable
        for (i, var_node) in variables.iter().enumerate() {
            let name_node = if var_node.kind() == "identifier" {
                Some(*var_node)
            } else if var_node.kind() == "variable" {
                self.find_child_by_type(*var_node, "identifier")
            } else {
                None
            };

            if let Some(name_node) = name_node {
                let name = self.base.get_node_text(&name_node);

                // Check if the corresponding expression is a function or import
                let expression = expressions.get(i);
                let mut kind = SymbolKind::Variable;
                let mut data_type = "unknown".to_string();

                if let Some(expression) = expression {
                    match expression.kind() {
                        "function_definition" | "function" | "function_expression" => {
                            kind = SymbolKind::Function;
                            data_type = "function".to_string();
                        }
                        "expression_list" => {
                            // Check if expression_list contains a function_definition (for anonymous functions)
                            if self.contains_function_definition(*expression) {
                                kind = SymbolKind::Function;
                                data_type = "function".to_string();
                            } else {
                                data_type = self.infer_type_from_expression(*expression);
                                if data_type == "import" {
                                    kind = SymbolKind::Import;
                                }
                            }
                        }
                        _ => {
                            data_type = self.infer_type_from_expression(*expression);
                            if data_type == "import" {
                                kind = SymbolKind::Import;
                            }
                        }
                    }
                }

                let mut metadata = HashMap::new();
                metadata.insert("dataType".to_string(), data_type.clone().into());

                let options = SymbolOptions {
                    signature: Some(signature.clone()),
                    parent_id: parent_id.map(|s| s.to_string()),
                    visibility: Some(Visibility::Private),
                    metadata: Some(metadata),
                    ..Default::default()
                };

                let mut symbol = self.base.create_symbol(&name_node, name, kind, options);

                // Set dataType as direct property for tests (matching Miller's pattern)
                if let Some(ref mut metadata) = symbol.metadata {
                    metadata.insert("dataType".to_string(), data_type.into());
                } else {
                    let mut metadata = HashMap::new();
                    metadata.insert("dataType".to_string(), data_type.into());
                    symbol.metadata = Some(metadata);
                }

                self.symbols.push(symbol);

                // If this is a table, extract its fields with this symbol as parent
                if let Some(expression) = expression {
                    if expression.kind() == "table_constructor" || expression.kind() == "table" {
                        let parent_id = self.symbols.last().unwrap().id.clone();
                        self.extract_table_fields(*expression, Some(&parent_id));
                    }
                }
            }
        }

        None
    }

    fn infer_type_from_expression(&self, node: Node) -> String {
        match node.kind() {
            "string" => "string".to_string(),
            "number" => "number".to_string(),
            "true" | "false" => "boolean".to_string(),
            "nil" => "nil".to_string(),
            "function_definition" => "function".to_string(),
            "table_constructor" | "table" => "table".to_string(),
            "function_call" => {
                // Check if this is a require() call
                if let Some(identifier) = self.find_child_by_type(node, "identifier") {
                    if self.base.get_node_text(&identifier) == "require" {
                        return "import".to_string();
                    }
                }
                "unknown".to_string()
            }
            _ => "unknown".to_string(),
        }
    }

    fn extract_assignment_statement(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        if children.len() < 3 {
            return None;
        }

        let left = children[0];
        let right = children[2]; // Skip the '=' operator

        // Handle variable_list assignments
        if left.kind() == "variable_list" {
            let mut left_cursor = left.walk();
            let variables: Vec<Node> = left
                .children(&mut left_cursor)
                .filter(|child| {
                    child.kind() == "variable"
                        || child.kind() == "identifier"
                        || child.kind() == "dot_index_expression"
                })
                .collect();

            for (i, var_node) in variables.iter().enumerate() {
                // Handle "variable" nodes, direct "identifier" nodes, and "dot_index_expression" nodes
                let name_node = if var_node.kind() == "identifier" {
                    *var_node
                } else if var_node.kind() == "dot_index_expression" {
                    // Handle dot notation assignments like M.PI = 3.14159
                    *var_node
                } else {
                    self.find_child_by_type(*var_node, "identifier")?
                };

                let name = self.base.get_node_text(&name_node);
                let signature = self.base.get_node_text(&node);

                // Handle dot notation assignments like M.PI = 3.14159
                let (actual_name, parent_symbol_id, kind_override) =
                    if var_node.kind() == "dot_index_expression" && name.contains('.') {
                        let parts: Vec<&str> = name.split('.').collect();
                        if parts.len() == 2 {
                            let object_name = parts[0];
                            let property_name = parts[1];

                            // Find the parent object
                            let parent_id = self
                                .symbols
                                .iter()
                                .find(|s| s.name == object_name)
                                .map(|s| s.id.clone());

                            (
                                property_name.to_string(),
                                parent_id,
                                Some(SymbolKind::Field),
                            )
                        } else {
                            (name, None, None)
                        }
                    } else {
                        (name, None, None)
                    };

                // Determine kind and type based on the assignment
                let is_field_assignment = matches!(kind_override, Some(SymbolKind::Field));
                let mut kind = kind_override.unwrap_or(SymbolKind::Variable);
                let mut data_type = "unknown".to_string();

                if right.kind() == "expression_list" {
                    let mut right_cursor = right.walk();
                    let expressions: Vec<Node> = right
                        .children(&mut right_cursor)
                        .filter(|child| child.kind() != ",")
                        .collect();

                    if let Some(expression) = expressions.get(i) {
                        if expression.kind() == "function_definition" {
                            // Override kind based on context
                            kind = if is_field_assignment {
                                SymbolKind::Method // Function assigned to object property = Method
                            } else {
                                SymbolKind::Function
                            };
                            data_type = "function".to_string();
                        } else {
                            data_type = self.infer_type_from_expression(*expression);
                        }
                    }
                } else if right.kind() == "function_definition" {
                    kind = SymbolKind::Function;
                    data_type = "function".to_string();
                } else {
                    data_type = self.infer_type_from_expression(right);
                    // Update kind based on inferred type
                    if data_type == "import" {
                        kind = SymbolKind::Import;
                    }
                }

                let mut metadata = HashMap::new();
                metadata.insert("dataType".to_string(), data_type.clone().into());

                let options = SymbolOptions {
                    signature: Some(signature),
                    parent_id: parent_symbol_id,
                    visibility: Some(Visibility::Public),
                    metadata: Some(metadata),
                    ..Default::default()
                };

                let symbol = self
                    .base
                    .create_symbol(&name_node, actual_name, kind, options);
                self.symbols.push(symbol);
            }
        }
        // Handle simple identifier assignments and dot notation
        else if left.kind() == "variable" {
            let full_variable_name = self.base.get_node_text(&left);

            // Handle dot notation assignments: M.PI = 3.14159
            if full_variable_name.contains('.') {
                let parts: Vec<&str> = full_variable_name.split('.').collect();
                if parts.len() == 2 {
                    let object_name = parts[0];
                    let property_name = parts[1];
                    let signature = self.base.get_node_text(&node);

                    // Determine kind and type based on the assignment
                    let mut kind = SymbolKind::Field; // Property assignments are fields
                    let data_type = if right.kind() == "function_definition" {
                        kind = SymbolKind::Method; // Methods on objects
                        "function".to_string()
                    } else {
                        self.infer_type_from_expression(right)
                    };

                    // Find the object this property belongs to
                    let property_parent_id = self
                        .symbols
                        .iter()
                        .find(|s| s.name == object_name)
                        .map(|s| s.id.clone())
                        .or_else(|| parent_id.map(|s| s.to_string()));

                    let mut metadata = HashMap::new();
                    metadata.insert("dataType".to_string(), data_type.clone().into());

                    let options = SymbolOptions {
                        signature: Some(signature),
                        parent_id: property_parent_id,
                        visibility: Some(Visibility::Public),
                        metadata: Some(metadata),
                        ..Default::default()
                    };

                    let symbol =
                        self.base
                            .create_symbol(&left, property_name.to_string(), kind, options);
                    self.symbols.push(symbol);
                }
            }
            // Handle simple identifier assignments: PI = 3.14159
            else if let Some(name_node) = self.find_child_by_type(left, "identifier") {
                let name = self.base.get_node_text(&name_node);
                let signature = self.base.get_node_text(&node);

                // Determine kind and type based on the assignment
                let mut kind = SymbolKind::Variable;
                let data_type = if right.kind() == "function_definition" {
                    kind = SymbolKind::Function;
                    "function".to_string()
                } else {
                    self.infer_type_from_expression(right)
                };

                let mut metadata = HashMap::new();
                metadata.insert("dataType".to_string(), data_type.clone().into());

                let options = SymbolOptions {
                    signature: Some(signature),
                    parent_id: parent_id.map(|s| s.to_string()),
                    visibility: Some(Visibility::Public), // Global assignments are public
                    metadata: Some(metadata),
                    ..Default::default()
                };

                let symbol = self.base.create_symbol(&name_node, name, kind, options);
                self.symbols.push(symbol);
            }
        }

        None
    }

    fn extract_variable_assignment(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract global variable assignments like: PI = 3.14159
        let variable_list = self.find_child_by_type(node, "variable_list")?;
        let expression_list = self.find_child_by_type(node, "expression_list");

        let signature = self.base.get_node_text(&node);
        let mut var_cursor = variable_list.walk();
        let variables: Vec<Node> = variable_list
            .children(&mut var_cursor)
            .filter(|child| child.kind() == "variable")
            .collect();

        let expressions: Vec<Node> = if let Some(expr_list) = expression_list {
            let mut expr_cursor = expr_list.walk();
            expr_list
                .children(&mut expr_cursor)
                .filter(|child| child.kind() != ",") // Filter out commas
                .collect()
        } else {
            Vec::new()
        };

        // Create symbols for each variable
        for (i, var_node) in variables.iter().enumerate() {
            let full_variable_name = self.base.get_node_text(var_node);

            // Handle dot notation: M.PI = 3.14159
            if full_variable_name.contains('.') {
                let parts: Vec<&str> = full_variable_name.split('.').collect();
                if parts.len() == 2 {
                    let object_name = parts[0];
                    let property_name = parts[1];

                    // Determine kind and type based on the assignment
                    // Module properties (M.PI) should be classified as Field
                    let mut kind = SymbolKind::Field;
                    let mut data_type = "unknown".to_string();

                    if let Some(expression) = expressions.get(i) {
                        if expression.kind() == "function_definition" {
                            kind = SymbolKind::Method; // Module methods should be Method, not Function
                            data_type = "function".to_string();
                        } else {
                            data_type = self.infer_type_from_expression(*expression);
                        }
                    }

                    // Find the object this property belongs to
                    let property_parent_id = self
                        .symbols
                        .iter()
                        .find(|s| s.name == object_name)
                        .map(|s| s.id.clone())
                        .or_else(|| parent_id.map(|s| s.to_string()));

                    let mut metadata = HashMap::new();
                    metadata.insert("dataType".to_string(), data_type.clone().into());

                    let options = SymbolOptions {
                        signature: Some(signature.clone()),
                        parent_id: property_parent_id,
                        visibility: Some(Visibility::Public),
                        metadata: Some(metadata),
                        ..Default::default()
                    };

                    let symbol =
                        self.base
                            .create_symbol(var_node, property_name.to_string(), kind, options);
                    self.symbols.push(symbol);

                    // If this is a table, extract its fields with this symbol as parent
                    if let Some(expression) = expressions.get(i) {
                        if expression.kind() == "table_constructor" || expression.kind() == "table"
                        {
                            let parent_id = self.symbols.last().unwrap().id.clone();
                            self.extract_table_fields(*expression, Some(&parent_id));
                        }
                    }
                }
            }
            // Handle simple variable: PI = 3.14159
            else if let Some(name_node) = self.find_child_by_type(*var_node, "identifier") {
                let name = self.base.get_node_text(&name_node);

                // Determine kind and type based on the assignment
                let mut kind = SymbolKind::Variable;
                let mut data_type = "unknown".to_string();

                if let Some(expression) = expressions.get(i) {
                    if expression.kind() == "function_definition" {
                        kind = SymbolKind::Function;
                        data_type = "function".to_string();
                    } else {
                        data_type = self.infer_type_from_expression(*expression);
                    }
                }

                let mut metadata = HashMap::new();
                metadata.insert("dataType".to_string(), data_type.clone().into());

                let options = SymbolOptions {
                    signature: Some(signature.clone()),
                    parent_id: parent_id.map(|s| s.to_string()),
                    visibility: Some(Visibility::Public), // Global variables are public
                    metadata: Some(metadata),
                    ..Default::default()
                };

                let symbol = self.base.create_symbol(&name_node, name, kind, options);
                self.symbols.push(symbol);

                // If this is a table, extract its fields with this symbol as parent
                if let Some(expression) = expressions.get(i) {
                    if expression.kind() == "table_constructor" || expression.kind() == "table" {
                        let parent_id = self.symbols.last().unwrap().id.clone();
                        self.extract_table_fields(*expression, Some(&parent_id));
                    }
                }
            }
        }

        None
    }

    fn extract_table_fields(&mut self, node: Node, parent_id: Option<&str>) {
        // Extract fields from table constructor: { field = value, method = function() end }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "field" {
                if let Some(field_symbol) = self.extract_table_field_symbol(child, parent_id) {
                    self.symbols.push(field_symbol);
                }
            }
        }
    }

    #[allow(dead_code)]
    fn extract_table_field(&mut self, node: Node, parent_id: Option<&str>) {
        // Handle field definitions like: field = value or field = function() end
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        if children.len() < 3 {
            return;
        }

        let name_node = children[0]; // field name
        let equal_node = children[1]; // '=' operator
        let value_node = children[2]; // field value

        if equal_node.kind() != "=" || name_node.kind() != "identifier" {
            return;
        }

        let name = self.base.get_node_text(&name_node);
        let signature = self.base.get_node_text(&node);

        // Determine if this is a method (function) or field (value)
        let mut kind = SymbolKind::Field;
        let data_type = if value_node.kind() == "function_definition" {
            kind = SymbolKind::Method;
            "function".to_string()
        } else {
            self.infer_type_from_expression(value_node)
        };

        let mut metadata = HashMap::new();
        metadata.insert("dataType".to_string(), data_type.clone().into());

        let options = SymbolOptions {
            signature: Some(signature),
            parent_id: parent_id.map(|s| s.to_string()),
            visibility: Some(Visibility::Public),
            metadata: Some(metadata),
            ..Default::default()
        };

        let symbol = self.base.create_symbol(&name_node, name, kind, options);
        self.symbols.push(symbol);
    }

    fn extract_table_field_symbol(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Handle field definitions like: field = value or field = function() end
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        if children.len() < 3 {
            return None;
        }
        let name_node = children[0]; // field name
        let equal_node = children[1]; // '=' operator
        let value_node = children[2]; // field value
        if equal_node.kind() != "=" || name_node.kind() != "identifier" {
            return None;
        }
        let name = self.base.get_node_text(&name_node);
        let signature = self.base.get_node_text(&node);
        // Determine if this is a method (function) or field (value)
        let mut kind = SymbolKind::Field;
        let data_type = if value_node.kind() == "function_definition" {
            kind = SymbolKind::Method;
            "function".to_string()
        } else {
            self.infer_type_from_expression(value_node)
        };
        let mut metadata = HashMap::new();
        metadata.insert("dataType".to_string(), data_type.clone().into());
        let options = SymbolOptions {
            signature: Some(signature),
            parent_id: parent_id.map(|s| s.to_string()),
            visibility: Some(Visibility::Public),
            metadata: Some(metadata),
            ..Default::default()
        };

        Some(self.base.create_symbol(&name_node, name, kind, options))
    }

    #[allow(dead_code)]
    fn is_table_handled_by_parent(&self, node: Node) -> bool {
        // Check if this table is part of a variable assignment
        // Look for patterns: local var = { ... } or var = { ... }
        let parent = node.parent();
        if parent.is_none() {
            return false;
        }

        let parent = parent.unwrap();

        // Check if parent is expression_list and grandparent is local_variable_declaration
        if parent.kind() == "expression_list" {
            let grandparent = parent.parent();
            if let Some(grandparent) = grandparent {
                if grandparent.kind() == "local_variable_declaration"
                    || grandparent.kind() == "variable_assignment"
                {
                    return true;
                }
            }
        }

        false
    }

    fn detect_lua_classes(&mut self) {
        // Post-process all symbols to detect Lua class patterns
        let mut class_upgrades = Vec::new();

        for (index, symbol) in self.symbols.iter().enumerate() {
            if symbol.kind == SymbolKind::Variable {
                let class_name = &symbol.name;

                // Pattern 1: Tables with metatable setup (local Class = {})
                let is_table = symbol
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("dataType"))
                    .map(|dt| dt.as_str() == Some("table"))
                    .unwrap_or(false);

                // Pattern 2: Variables created with setmetatable (local Dog = setmetatable({}, Animal))
                let is_setmetatable = symbol
                    .signature
                    .as_ref()
                    .map(|s| s.contains("setmetatable("))
                    .unwrap_or(false);

                // Only check class patterns for tables or setmetatable creations
                if is_table || is_setmetatable {
                    // Look for metatable patterns that indicate this is a class
                    let has_index_pattern = self.symbols.iter().any(|s| {
                        s.signature
                            .as_ref()
                            .map(|sig| {
                                sig.contains(&format!("{}.__index = {}", class_name, class_name))
                            })
                            .unwrap_or(false)
                    });

                    let has_new_method = self.symbols.iter().any(|s| {
                        s.name == "new"
                            && s.signature
                                .as_ref()
                                .map(|sig| sig.contains(&format!("{}.new", class_name)))
                                .unwrap_or(false)
                    });

                    let has_colon_methods = self.symbols.iter().any(|s| {
                        s.kind == SymbolKind::Method
                            && s.signature
                                .as_ref()
                                .map(|sig| sig.contains(&format!("{}:", class_name)))
                                .unwrap_or(false)
                    });

                    // If it has metatable patterns, upgrade to Class
                    if has_index_pattern || (has_new_method && has_colon_methods) || is_setmetatable
                    {
                        class_upgrades.push((index, is_setmetatable, symbol.signature.clone()));
                    }
                }
            }
        }

        // Apply class upgrades
        for (index, is_setmetatable, signature) in class_upgrades {
            self.symbols[index].kind = SymbolKind::Class;

            // Extract inheritance information from setmetatable pattern
            if is_setmetatable {
                if let Some(captures) = signature.as_ref().and_then(|s| SETMETATABLE_RE.captures(s))
                {
                    if let Some(parent_class_name) = captures.get(1) {
                        let parent_class_name = parent_class_name.as_str();
                        // Verify the parent class exists in our symbols
                        let parent_exists = self.symbols.iter().any(|s| {
                            s.name == parent_class_name
                                && (s.kind == SymbolKind::Class
                                    || s.metadata
                                        .as_ref()
                                        .and_then(|m| m.get("dataType"))
                                        .map(|dt| dt.as_str() == Some("table"))
                                        .unwrap_or(false))
                        });

                        if parent_exists {
                            let metadata = self.symbols[index]
                                .metadata
                                .get_or_insert_with(HashMap::new);
                            metadata.insert("baseClass".to_string(), parent_class_name.into());
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::manual_find)] // Manual loop required for borrow checker
    fn find_child_by_type<'a>(&self, node: Node<'a>, node_type: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == node_type {
                return Some(child);
            }
        }
        None
    }

    fn contains_function_definition(&self, node: Node) -> bool {
        // Check if this node contains a function_definition child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "function_definition" {
                return true;
            }
        }
        false
    }

    // ========================================================================
    // Identifier Extraction (for LSP-quality find_references)
    // ========================================================================

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);

        // Return the collected identifiers
        self.base.identifiers.clone()
    }

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(
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
    fn extract_identifier_from_node(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // Function calls: foo(), require("module")
            "function_call" => {
                // Try to get the function name from the identifier child
                if let Some(name_node) = self.find_child_by_type(node, "identifier") {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
                // If no direct identifier, check for dot_index_expression (like math.sqrt())
                else if let Some(dot_index) = self.find_child_by_type(node, "dot_index_expression")
                {
                    // Extract the rightmost identifier (the method name)
                    if let Some(_method_node) = self.find_child_by_type(dot_index, "identifier") {
                        // Get all identifiers and use the last one (rightmost)
                        let mut cursor = dot_index.walk();
                        let identifiers: Vec<Node> = dot_index
                            .children(&mut cursor)
                            .filter(|c| c.kind() == "identifier")
                            .collect();

                        if let Some(last_identifier) = identifiers.last() {
                            let name = self.base.get_node_text(last_identifier);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                last_identifier,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                }
            }

            // Method calls with colon syntax: obj:method()
            "method_index_expression" => {
                // Extract the method name (rightmost identifier)
                let mut cursor = node.walk();
                let identifiers: Vec<Node> = node
                    .children(&mut cursor)
                    .filter(|c| c.kind() == "identifier")
                    .collect();

                if let Some(method_node) = identifiers.last() {
                    let name = self.base.get_node_text(method_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        method_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }

            // Member access with dot: obj.field, obj.field.nested
            "dot_index_expression" => {
                // Only extract if it's NOT part of a function_call or method_index_expression
                // (we handle those in the cases above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "function_call" || parent.kind() == "method_index_expression"
                    {
                        return; // Skip - handled by function/method call
                    }
                }

                // Extract the rightmost identifier (the member name)
                let mut cursor = node.walk();
                let identifiers: Vec<Node> = node
                    .children(&mut cursor)
                    .filter(|c| c.kind() == "identifier")
                    .collect();

                if let Some(member_node) = identifiers.last() {
                    let name = self.base.get_node_text(member_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        member_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            _ => {
                // Skip other node types for now
                // Future: type usage, import statements, etc.
            }
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
    fn find_containing_symbol_id(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        // CRITICAL FIX: Only search symbols from THIS FILE, not all files
        // Bug was: searching all symbols in DB caused wrong file symbols to match
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
