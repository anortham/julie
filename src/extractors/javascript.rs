// JavaScript Extractor for Julie
//
// Direct port of Miller's JavaScript extractor logic ported to idiomatic Rust
// Original: /Users/murphy/Source/miller/src/extractors/javascript-extractor.ts
//
// This follows the exact extraction strategy from Miller while using Rust patterns:
// - Uses Miller's node type switch statement logic
// - Preserves Miller's signature building algorithms
// - Maintains Miller's same edge case handling
// - Converts to Rust Option<T>, Result<T>, iterators, ownership system

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, SymbolOptions, Visibility};
use tree_sitter::Tree;
use std::collections::HashMap;
use serde_json::json;

pub struct JavaScriptExtractor {
    base: BaseExtractor,
}

impl JavaScriptExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_node_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    /// Main tree traversal - ports Miller's visitNode function exactly
    fn visit_node(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        let mut symbol: Option<Symbol> = None;

        // Port Miller's switch statement exactly
        match node.kind() {
            "class_declaration" => {
                symbol = Some(self.extract_class(node, parent_id.clone()));
            }
            "function_declaration" | "function" | "arrow_function" | "function_expression" | "generator_function" | "generator_function_declaration" => {
                symbol = Some(self.extract_function(node, parent_id.clone()));
            }
            "method_definition" => {
                symbol = Some(self.extract_method(node, parent_id.clone()));
            }
            "variable_declarator" => {
                // Handle destructuring patterns that create multiple symbols (Miller's logic)
                let name_node = node.child_by_field_name("name");
                if let Some(name) = name_node {
                    if name.kind() == "object_pattern" || name.kind() == "array_pattern" {
                        let destructured_symbols = self.extract_destructuring_variables(node, parent_id.clone());
                        symbols.extend(destructured_symbols);
                    } else {
                        symbol = Some(self.extract_variable(node, parent_id.clone()));
                    }
                } else {
                    symbol = Some(self.extract_variable(node, parent_id.clone()));
                }
            }
            "import_statement" | "import_declaration" => {
                // Handle multiple import specifiers (Miller's logic)
                let import_symbols = self.extract_import_specifiers(&node);
                for specifier in import_symbols {
                    let import_symbol = self.create_import_symbol(node, &specifier, parent_id.clone());
                    symbols.push(import_symbol);
                }
            }
            "export_statement" | "export_declaration" => {
                symbol = Some(self.extract_export(node, parent_id.clone()));
            }
            "property_definition" | "public_field_definition" | "field_definition" | "pair" => {
                symbol = Some(self.extract_property(node, parent_id.clone()));
            }
            "assignment_expression" => {
                if let Some(assignment_symbol) = self.extract_assignment(node, parent_id.clone()) {
                    symbol = Some(assignment_symbol);
                }
            }
            _ => {}
        }

        let current_parent_id = if let Some(sym) = &symbol {
            symbols.push(sym.clone());
            Some(sym.id.clone())
        } else {
            parent_id
        };

        // Recursively visit children (Miller's pattern)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract class declarations - direct port of Miller's extractClass
    fn extract_class(&mut self, node: tree_sitter::Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node.map(|n| self.base.get_node_text(&n)).unwrap_or_else(|| "Anonymous".to_string());

        // Extract extends clause (Miller's logic)
        let heritage = node.child_by_field_name("heritage")
            .or_else(|| node.children(&mut node.walk()).find(|c| c.kind() == "class_heritage"));

        let extends_clause = heritage.and_then(|h|
            h.children(&mut h.walk()).find(|c| c.kind() == "extends_clause")
        );

        let signature = self.build_class_signature(&node);

        let mut metadata = HashMap::new();
        metadata.insert("extends".to_string(), json!(extends_clause.map(|ec| self.base.get_node_text(&ec))));
        metadata.insert("isGenerator".to_string(), json!(false)); // JavaScript classes are not generators
        metadata.insert("hasPrivateFields".to_string(), json!(self.has_private_fields(&node)));

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract function declarations - direct port of Miller's extractFunction
    fn extract_function(&mut self, node: tree_sitter::Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let mut name = name_node.map(|n| self.base.get_node_text(&n)).unwrap_or_else(|| "Anonymous".to_string());

        // Handle arrow functions assigned to variables (Miller's logic)
        if node.kind() == "arrow_function" || node.kind() == "function_expression" {
            if let Some(parent) = node.parent() {
                if parent.kind() == "variable_declarator" {
                    if let Some(var_name_node) = parent.child_by_field_name("name") {
                        name = self.base.get_node_text(&var_name_node);
                    }
                } else if parent.kind() == "assignment_expression" {
                    if let Some(left_node) = parent.child_by_field_name("left") {
                        name = self.base.get_node_text(&left_node);
                    }
                } else if parent.kind() == "pair" {
                    if let Some(key_node) = parent.child_by_field_name("key") {
                        name = self.base.get_node_text(&key_node);
                    }
                }
            }
        }

        let signature = self.build_function_signature(&node, &name);

        let mut metadata = HashMap::new();
        metadata.insert("isAsync".to_string(), json!(self.is_async(&node)));
        metadata.insert("isGenerator".to_string(), json!(self.is_generator(&node)));
        metadata.insert("isArrowFunction".to_string(), json!(node.kind() == "arrow_function"));
        metadata.insert("parameters".to_string(), json!(self.extract_parameters(&node)));
        metadata.insert("isExpression".to_string(), json!(node.kind() == "function_expression"));

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract method definitions - direct port of Miller's extractMethod
    fn extract_method(&mut self, node: tree_sitter::Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name")
            .or_else(|| node.child_by_field_name("property"))
            .or_else(|| node.child_by_field_name("key"));

        let name = name_node.map(|n| self.base.get_node_text(&n)).unwrap_or_else(|| "Anonymous".to_string());

        let signature = self.build_method_signature(&node, &name);

        // Determine if it's a constructor (Miller's logic)
        let symbol_kind = if name == "constructor" {
            SymbolKind::Constructor
        } else {
            SymbolKind::Method
        };

        // Check for getters and setters (Miller's logic)
        let is_getter = node.children(&mut node.walk()).any(|c| c.kind() == "get");
        let is_setter = node.children(&mut node.walk()).any(|c| c.kind() == "set");

        let mut metadata = HashMap::new();
        metadata.insert("isStatic".to_string(), json!(node.children(&mut node.walk()).any(|c| c.kind() == "static")));
        metadata.insert("isAsync".to_string(), json!(self.is_async(&node)));
        metadata.insert("isGenerator".to_string(), json!(self.is_generator(&node)));
        metadata.insert("isGetter".to_string(), json!(is_getter));
        metadata.insert("isSetter".to_string(), json!(is_setter));
        metadata.insert("isPrivate".to_string(), json!(name.starts_with('#')));
        metadata.insert("parameters".to_string(), json!(self.extract_parameters(&node)));

        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract variable declarations - direct port of Miller's extractVariable
    fn extract_variable(&mut self, node: tree_sitter::Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node.map(|n| self.base.get_node_text(&n)).unwrap_or_else(|| "Anonymous".to_string());

        let value_node = node.child_by_field_name("value");
        let signature = self.build_variable_signature(&node, &name);

        // Check if this is a CommonJS require statement (Miller's logic)
        if let Some(value) = &value_node {
            if self.is_require_call(value) {
                let mut metadata = HashMap::new();
                metadata.insert("source".to_string(), json!(self.extract_require_source(value)));
                metadata.insert("isCommonJS".to_string(), json!(true));

                return self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Import,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(self.extract_visibility(&node)),
                        parent_id,
                        metadata: Some(metadata),
                        doc_comment: None,
                    },
                );
            }

            // For function expressions, create a function symbol with the variable's name (Miller's logic)
            if value.kind() == "arrow_function" || value.kind() == "function_expression" || value.kind() == "generator_function" {
                let mut metadata = HashMap::new();
                metadata.insert("isAsync".to_string(), json!(self.is_async(value)));
                metadata.insert("isGenerator".to_string(), json!(self.is_generator(value)));
                metadata.insert("isArrowFunction".to_string(), json!(value.kind() == "arrow_function"));
                metadata.insert("isExpression".to_string(), json!(true));
                metadata.insert("parameters".to_string(), json!(self.extract_parameters(value)));

                return self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Function,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(self.extract_visibility(&node)),
                        parent_id,
                        metadata: Some(metadata),
                        doc_comment: None,
                    },
                );
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("declarationType".to_string(), json!(self.get_declaration_type(&node)));
        metadata.insert("initializer".to_string(), json!(value_node.map(|v| self.base.get_node_text(&v))));
        metadata.insert("isConst".to_string(), json!(self.is_const_declaration(&node)));
        metadata.insert("isLet".to_string(), json!(self.is_let_declaration(&node)));

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract property definitions - direct port of Miller's extractProperty
    fn extract_property(&mut self, node: tree_sitter::Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("key")
            .or_else(|| node.child_by_field_name("name"))
            .or_else(|| node.child_by_field_name("property"));

        let name = name_node.map(|n| self.base.get_node_text(&n)).unwrap_or_else(|| "Anonymous".to_string());
        let value_node = node.child_by_field_name("value");
        let signature = self.build_property_signature(&node, &name);

        // If the value is a function, treat it as a method (Miller's logic)
        if let Some(value) = &value_node {
            if value.kind() == "arrow_function" || value.kind() == "function_expression" || value.kind() == "generator_function" {
                let method_signature = self.build_method_signature(value, &name);

                let mut metadata = HashMap::new();
                metadata.insert("isAsync".to_string(), json!(self.is_async(value)));
                metadata.insert("isGenerator".to_string(), json!(self.is_generator(value)));
                metadata.insert("parameters".to_string(), json!(self.extract_parameters(value)));

                return self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Method,
                    SymbolOptions {
                        signature: Some(method_signature),
                        visibility: Some(self.extract_visibility(&node)),
                        parent_id,
                        metadata: Some(metadata),
                        doc_comment: None,
                    },
                );
            }
        }

        // Determine if this is a class field or regular property (Miller's logic)
        let symbol_kind = match node.kind() {
            "public_field_definition" | "field_definition" | "property_definition" => SymbolKind::Field,
            _ => SymbolKind::Property,
        };

        let mut metadata = HashMap::new();
        metadata.insert("value".to_string(), json!(value_node.map(|v| self.base.get_node_text(&v))));
        metadata.insert("isComputed".to_string(), json!(self.is_computed_property(&node)));
        metadata.insert("isPrivate".to_string(), json!(name.starts_with('#')));

        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Create import symbol - direct port of Miller's createImportSymbol
    fn create_import_symbol(&mut self, node: tree_sitter::Node, specifier: &str, parent_id: Option<String>) -> Symbol {
        let source = node.child_by_field_name("source");
        let source_path = source.map(|s| self.base.get_node_text(&s).replace(&['\'', '"', '`'][..], ""))
            .unwrap_or_else(|| "unknown".to_string());

        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), json!(source_path));
        metadata.insert("specifier".to_string(), json!(specifier));
        metadata.insert("isDefault".to_string(), json!(self.has_default_import(&node)));
        metadata.insert("isNamespace".to_string(), json!(self.has_namespace_import(&node)));

        self.base.create_symbol(
            &node,
            specifier.to_string(),
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(self.base.get_node_text(&node)),
                visibility: None,
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract export declarations - direct port of Miller's extractExport
    fn extract_export(&mut self, node: tree_sitter::Node, parent_id: Option<String>) -> Symbol {
        let exported_name = self.extract_exported_name(&node);
        let signature = self.base.get_node_text(&node);

        let mut metadata = HashMap::new();
        metadata.insert("exportedName".to_string(), json!(exported_name));
        metadata.insert("isDefault".to_string(), json!(self.is_default_export(&node)));
        metadata.insert("isNamed".to_string(), json!(self.is_named_export(&node)));

        self.base.create_symbol(
            &node,
            exported_name.clone(),
            SymbolKind::Export,
            SymbolOptions {
                signature: Some(signature),
                visibility: None,
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract assignment expressions - direct port of Miller's extractAssignment
    fn extract_assignment(&mut self, node: tree_sitter::Node, parent_id: Option<String>) -> Option<Symbol> {
        let left_node = node.child_by_field_name("left");
        let right_node = node.child_by_field_name("right");

        if let Some(left) = left_node {
            // Handle member expression assignments like: Constructor.prototype.method = function() {} (Miller's logic)
            if left.kind() == "member_expression" {
                let object_node = left.child_by_field_name("object");
                let property_node = left.child_by_field_name("property");

                if let (Some(object), Some(property)) = (object_node, property_node) {
                    let object_text = self.base.get_node_text(&object);
                    let property_name = self.base.get_node_text(&property);
                    let signature = self.base.get_node_text(&node);

                    // Check if this is a prototype assignment (Miller's logic)
                    if object_text.contains(".prototype") {
                        let mut metadata = HashMap::new();
                        metadata.insert("isPrototypeMethod".to_string(), json!(true));
                        metadata.insert("isFunction".to_string(), json!(
                            right_node.map(|r| r.kind() == "function_expression" || r.kind() == "arrow_function").unwrap_or(false)
                        ));

                        return Some(self.base.create_symbol(
                            &node,
                            property_name,
                            SymbolKind::Method,
                            SymbolOptions {
                                signature: Some(signature),
                                visibility: Some(Visibility::Public),
                                parent_id,
                                metadata: Some(metadata),
                                doc_comment: None,
                            },
                        ));
                    }
                    // Check if this is a static method assignment (Miller's logic)
                    else if let Some(right) = right_node {
                        if right.kind() == "function_expression" || right.kind() == "arrow_function" {
                            let mut metadata = HashMap::new();
                            metadata.insert("isStaticMethod".to_string(), json!(true));
                            metadata.insert("isFunction".to_string(), json!(true));
                            metadata.insert("className".to_string(), json!(object_text));

                            return Some(self.base.create_symbol(
                                &node,
                                property_name,
                                SymbolKind::Method,
                                SymbolOptions {
                                    signature: Some(signature),
                                    visibility: Some(Visibility::Public),
                                    parent_id,
                                    metadata: Some(metadata),
                                    doc_comment: None,
                                },
                            ));
                        }
                    }
                }
            }
        }

        None
    }

    // Helper methods - direct ports of Miller's helper functions

    /// Build class signature - direct port of Miller's buildClassSignature
    fn build_class_signature(&self, node: &tree_sitter::Node) -> String {
        let name_node = node.child_by_field_name("name");
        let name = name_node.map(|n| self.base.get_node_text(&n)).unwrap_or_else(|| "Anonymous".to_string());

        let mut signature = format!("class {}", name);

        // Look for extends clause (Miller's logic)
        let heritage = node.child_by_field_name("superclass")
            .or_else(|| node.children(&mut node.walk()).find(|c| c.kind() == "class_heritage"));

        if let Some(h) = heritage {
            if h.kind() == "identifier" {
                // Direct superclass reference
                signature.push_str(&format!(" extends {}", self.base.get_node_text(&h)));
            } else {
                // Look within class_heritage for extends_clause or identifier
                for child in h.children(&mut h.walk()) {
                    if child.kind() == "identifier" {
                        signature.push_str(&format!(" extends {}", self.base.get_node_text(&child)));
                        break;
                    }
                }
            }
        }

        signature
    }

    /// Build function signature - direct port of Miller's buildFunctionSignature
    fn build_function_signature(&self, node: &tree_sitter::Node, name: &str) -> String {
        let is_async = self.is_async(node);
        let is_generator = self.is_generator(node);
        let parameters = self.extract_parameters(node);

        let mut signature = String::new();

        if is_async {
            signature.push_str("async ");
        }

        match node.kind() {
            "arrow_function" => {
                if is_generator {
                    signature.push_str("function* ");
                }
                signature.push_str(&format!("{} = ({}) => ", name, parameters.join(", ")));
            }
            "function_expression" => {
                if is_generator {
                    signature.push_str("function* ");
                } else {
                    signature.push_str("function ");
                }
                signature.push_str(&format!("{}({})", name, parameters.join(", ")));
            }
            _ => {
                if is_generator {
                    signature.push_str("function* ");
                } else {
                    signature.push_str("function ");
                }
                signature.push_str(&format!("{}({})", name, parameters.join(", ")));
            }
        }

        signature
    }

    /// Build method signature - direct port of Miller's buildMethodSignature
    fn build_method_signature(&self, node: &tree_sitter::Node, name: &str) -> String {
        let is_async = self.is_async(node);
        let is_generator = self.is_generator(node);
        let is_static = node.children(&mut node.walk()).any(|c| c.kind() == "static");
        let is_getter = node.children(&mut node.walk()).any(|c| c.kind() == "get");
        let is_setter = node.children(&mut node.walk()).any(|c| c.kind() == "set");
        let parameters = self.extract_parameters(node);

        let mut signature = String::new();

        if is_static { signature.push_str("static "); }
        if is_async { signature.push_str("async "); }
        if is_getter { signature.push_str("get "); }
        if is_setter { signature.push_str("set "); }
        if is_generator { signature.push('*'); }

        signature.push_str(&format!("{}({})", name, parameters.join(", ")));

        signature
    }

    /// Build variable signature - direct port of Miller's buildVariableSignature
    fn build_variable_signature(&self, node: &tree_sitter::Node, name: &str) -> String {
        let declaration_type = self.get_declaration_type(node);
        let value_node = node.child_by_field_name("value");

        let mut signature = format!("{} {}", declaration_type, name);

        if let Some(value) = value_node {
            match value.kind() {
                "function_expression" => {
                    signature.push_str(" = function");
                    let params = self.extract_parameters(&value);
                    signature.push_str(&format!("({})", params.join(", ")));
                }
                "arrow_function" => {
                    let is_async = self.is_async(&value);
                    if is_async {
                        signature.push_str(" = async ");
                    } else {
                        signature.push_str(" = ");
                    }

                    let params = self.extract_parameters(&value);
                    signature.push_str(&format!("({}) =>", params.join(", ")));

                    // For simple arrow functions, include the body if it's a simple expression (Miller's logic)
                    let body_node = value.children(&mut value.walk()).find(|c| {
                        matches!(c.kind(), "expression" | "binary_expression" | "call_expression" | "identifier" | "number" | "string")
                    });

                    if let Some(body) = body_node {
                        let body_text = self.base.get_node_text(&body);
                        if body_text.len() <= 30 {
                            signature.push_str(&format!(" {}", body_text));
                        }
                    }
                }
                _ => {
                    let value_text = self.base.get_node_text(&value);
                    // Truncate very long values (Miller's logic)
                    let truncated_value = if value_text.len() > 50 {
                        format!("{}...", &value_text[..50])
                    } else {
                        value_text
                    };
                    signature.push_str(&format!(" = {}", truncated_value));
                }
            }
        }

        signature
    }

    /// Build property signature - direct port of Miller's buildPropertySignature
    fn build_property_signature(&self, node: &tree_sitter::Node, name: &str) -> String {
        let value_node = node.child_by_field_name("value");

        let mut signature = name.to_string();

        if let Some(value) = value_node {
            let value_text = self.base.get_node_text(&value);
            let truncated_value = if value_text.len() > 30 {
                format!("{}...", &value_text[..30])
            } else {
                value_text
            };
            signature.push_str(&format!(": {}", truncated_value));
        }

        signature
    }

    /// Get declaration type - direct port of Miller's getDeclarationType
    fn get_declaration_type(&self, node: &tree_sitter::Node) -> String {
        let mut current = node.parent();
        while let Some(current_node) = current {
            if current_node.kind() == "variable_declaration" || current_node.kind() == "lexical_declaration" {
                // Look for the keyword in the first child (Miller's logic)
                if let Some(first_child) = current_node.child(0) {
                    let text = self.base.get_node_text(&first_child);
                    if ["const", "let", "var"].contains(&text.as_str()) {
                        return text;
                    }
                }
                // Fallback: look through all children for keywords (Miller's logic)
                for child in current_node.children(&mut current_node.walk()) {
                    let text = self.base.get_node_text(&child);
                    if ["const", "let", "var"].contains(&text.as_str()) {
                        return text;
                    }
                }
            }
            current = current_node.parent();
        }
        "var".to_string()
    }

    /// Check if function is async - direct port of Miller's isAsync
    fn is_async(&self, node: &tree_sitter::Node) -> bool {
        // Direct check: node has async child (Miller's logic)
        if node.children(&mut node.walk()).any(|c| self.base.get_node_text(&c) == "async" || c.kind() == "async") {
            return true;
        }

        // For arrow functions: check if first child is async (Miller's logic)
        if node.kind() == "arrow_function" {
            if let Some(first_child) = node.child(0) {
                if self.base.get_node_text(&first_child) == "async" {
                    return true;
                }
            }
        }

        // For function expressions and arrow functions assigned to variables, check parent (Miller's logic)
        let mut current = node.parent();
        while let Some(current_node) = current {
            if current_node.kind() == "program" {
                break;
            }
            if current_node.children(&mut current_node.walk()).any(|c| self.base.get_node_text(&c) == "async") {
                return true;
            }
            current = current_node.parent();
        }

        false
    }

    /// Check if function is generator - direct port of Miller's isGenerator
    fn is_generator(&self, node: &tree_sitter::Node) -> bool {
        node.kind().contains("generator") ||
        node.children(&mut node.walk()).any(|c| c.kind() == "*") ||
        node.parent().map(|p| p.children(&mut p.walk()).any(|c| c.kind() == "*")).unwrap_or(false)
    }

    /// Check if declaration is const - direct port of Miller's isConstDeclaration
    fn is_const_declaration(&self, node: &tree_sitter::Node) -> bool {
        self.get_declaration_type(node) == "const"
    }

    /// Check if declaration is let - direct port of Miller's isLetDeclaration
    fn is_let_declaration(&self, node: &tree_sitter::Node) -> bool {
        self.get_declaration_type(node) == "let"
    }

    /// Check if class has private fields - direct port of Miller's hasPrivateFields
    fn has_private_fields(&self, node: &tree_sitter::Node) -> bool {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "class_body" {
                for member in child.children(&mut child.walk()) {
                    let name_node = member.child_by_field_name("name")
                        .or_else(|| member.child_by_field_name("property"));
                    if let Some(name) = name_node {
                        if self.base.get_node_text(&name).starts_with('#') {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Check if property is computed - direct port of Miller's isComputedProperty
    fn is_computed_property(&self, node: &tree_sitter::Node) -> bool {
        node.child_by_field_name("key")
            .map(|key| key.kind() == "computed_property_name")
            .unwrap_or(false)
    }

    /// Check if import has default - direct port of Miller's hasDefaultImport
    fn has_default_import(&self, node: &tree_sitter::Node) -> bool {
        node.children(&mut node.walk()).any(|c| c.kind() == "import_default_specifier")
    }

    /// Check if import has namespace - direct port of Miller's hasNamespaceImport
    fn has_namespace_import(&self, node: &tree_sitter::Node) -> bool {
        node.children(&mut node.walk()).any(|c| c.kind() == "namespace_import")
    }

    /// Check if export is default - direct port of Miller's isDefaultExport
    fn is_default_export(&self, node: &tree_sitter::Node) -> bool {
        node.children(&mut node.walk()).any(|c| c.kind() == "default")
    }

    /// Check if export is named - direct port of Miller's isNamedExport
    fn is_named_export(&self, node: &tree_sitter::Node) -> bool {
        !self.is_default_export(node)
    }

    /// Extract function parameters - direct port of Miller's extractParameters
    fn extract_parameters(&self, node: &tree_sitter::Node) -> Vec<String> {
        // Look for formal_parameters node (Miller's logic)
        let formal_params = node.children(&mut node.walk()).find(|c| c.kind() == "formal_parameters");
        if let Some(params) = formal_params {
            let mut parameters = Vec::new();
            for child in params.children(&mut params.walk()) {
                if matches!(child.kind(),
                    "identifier" | "rest_pattern" | "object_pattern" | "array_pattern" |
                    "assignment_pattern" | "object_assignment_pattern" | "shorthand_property_identifier_pattern"
                ) {
                    parameters.push(self.base.get_node_text(&child));
                }
            }
            return parameters;
        }
        Vec::new()
    }

    /// Extract import specifiers - direct port of Miller's extractImportSpecifiers
    fn extract_import_specifiers(&self, node: &tree_sitter::Node) -> Vec<String> {
        let mut specifiers = Vec::new();

        // Look for import clause which contains the specifiers (Miller's logic)
        let import_clause = node.children(&mut node.walk()).find(|c| c.kind() == "import_clause");
        if let Some(clause) = import_clause {
            for child in clause.children(&mut clause.walk()) {
                match child.kind() {
                    "import_specifier" => {
                        // For named imports like { debounce, throttle } (Miller's logic)
                        if let Some(name_node) = child.child_by_field_name("name") {
                            specifiers.push(self.base.get_node_text(&name_node));
                        }
                        if let Some(alias_node) = child.child_by_field_name("alias") {
                            specifiers.push(self.base.get_node_text(&alias_node));
                        }
                    }
                    "identifier" => {
                        // For default imports like React (Miller's logic)
                        specifiers.push(self.base.get_node_text(&child));
                    }
                    "namespace_import" => {
                        // For namespace imports like * as name (Miller's logic)
                        specifiers.push(self.base.get_node_text(&child));
                    }
                    "named_imports" => {
                        // Look inside named_imports for specifiers (Miller's logic)
                        for named_child in child.children(&mut child.walk()) {
                            if named_child.kind() == "import_specifier" {
                                if let Some(name_node) = named_child.child_by_field_name("name") {
                                    specifiers.push(self.base.get_node_text(&name_node));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        specifiers
    }

    /// Extract exported name - direct port of Miller's extractExportedName
    fn extract_exported_name(&self, node: &tree_sitter::Node) -> String {
        // Handle different export patterns (Miller's logic)
        for child in node.children(&mut node.walk()) {
            match child.kind() {
                // Direct exports: export const Component = ..., export function foo() {}, export class Bar {}
                "variable_declaration" | "lexical_declaration" => {
                    let declarator = child.children(&mut child.walk()).find(|c| c.kind() == "variable_declarator");
                    if let Some(decl) = declarator {
                        if let Some(name_node) = decl.child_by_field_name("name") {
                            return self.base.get_node_text(&name_node);
                        }
                    }
                }
                "class_declaration" | "function_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        return self.base.get_node_text(&name_node);
                    }
                }
                "identifier" => {
                    // Simple export: export identifier
                    return self.base.get_node_text(&child);
                }
                "export_clause" => {
                    // Handle export { default as Component } patterns (Miller's logic)
                    for clause_child in child.children(&mut child.walk()) {
                        if clause_child.kind() == "export_specifier" {
                            let children: Vec<_> = clause_child.children(&mut clause_child.walk()).collect();
                            for i in 0..children.len() {
                                if self.base.get_node_text(&children[i]) == "as" && i + 1 < children.len() {
                                    return self.base.get_node_text(&children[i + 1]);
                                }
                            }
                            // If no "as", return the export name
                            if let Some(name_node) = children.iter().find(|c| c.kind() == "identifier") {
                                return self.base.get_node_text(name_node);
                            }
                        }
                    }
                }
                "export_specifier" => {
                    // Named export specifier (direct child)
                    if let Some(name_node) = child.child_by_field_name("name") {
                        return self.base.get_node_text(&name_node);
                    }
                }
                _ => {}
            }
        }

        // Look for default exports (Miller's logic)
        if self.is_default_export(node) {
            return "default".to_string();
        }

        "unknown".to_string()
    }

    /// Extract visibility - direct port of Miller's extractVisibility
    fn extract_visibility(&self, node: &tree_sitter::Node) -> Visibility {
        // JavaScript doesn't have explicit visibility modifiers like TypeScript
        // But we can infer from naming conventions (Miller's logic)
        let name_node = node.child_by_field_name("name")
            .or_else(|| node.child_by_field_name("property"));

        if let Some(name) = name_node {
            let name_text = self.base.get_node_text(&name);
            if name_text.starts_with('#') {
                return Visibility::Private;
            }
            if name_text.starts_with('_') {
                return Visibility::Protected; // Convention
            }
        }

        Visibility::Public
    }

    /// Check if node is require call - direct port of Miller's isRequireCall
    fn is_require_call(&self, node: &tree_sitter::Node) -> bool {
        if node.kind() == "call_expression" {
            if let Some(function_node) = node.child_by_field_name("function") {
                return self.base.get_node_text(&function_node) == "require";
            }
        }
        false
    }

    /// Extract require source - direct port of Miller's extractRequireSource
    fn extract_require_source(&self, node: &tree_sitter::Node) -> String {
        if node.kind() == "call_expression" {
            if let Some(args) = node.child_by_field_name("arguments") {
                for child in args.children(&mut args.walk()) {
                    if child.kind() == "string" {
                        return self.base.get_node_text(&child).replace(&['\'', '"', '`'][..], "");
                    }
                }
            }
        }
        "unknown".to_string()
    }

    /// Extract destructuring variables - direct port of Miller's extractDestructuringVariables
    fn extract_destructuring_variables(&mut self, node: tree_sitter::Node, parent_id: Option<String>) -> Vec<Symbol> {
        let name_node = node.child_by_field_name("name");
        let value_node = node.child_by_field_name("value");
        let mut symbols = Vec::new();

        if let Some(name) = name_node {
            let declaration_type = self.get_declaration_type(&node);
            let value_text = value_node.map(|v| self.base.get_node_text(&v)).unwrap_or_default();

            match name.kind() {
                "object_pattern" => {
                    // Handle object destructuring: const { name, age, ...rest } = user (Miller's logic)
                    for child in name.children(&mut name.walk()) {
                        match child.kind() {
                            "shorthand_property_identifier_pattern" | "property_identifier" | "identifier" => {
                                let var_name = self.base.get_node_text(&child);
                                let signature = format!("{} {{ {} }} = {}", declaration_type, var_name, value_text);

                                let mut metadata = HashMap::new();
                                metadata.insert("declarationType".to_string(), json!(declaration_type));
                                metadata.insert("isDestructured".to_string(), json!(true));
                                metadata.insert("destructuringType".to_string(), json!("object"));

                                symbols.push(self.base.create_symbol(
                                    &node,
                                    var_name,
                                    SymbolKind::Variable,
                                    SymbolOptions {
                                        signature: Some(signature),
                                        visibility: Some(self.extract_visibility(&node)),
                                        parent_id: parent_id.clone(),
                                        metadata: Some(metadata),
                                        doc_comment: None,
                                    },
                                ));
                            }
                            "rest_pattern" => {
                                // Handle rest parameters: const { name, ...rest } = user (Miller's logic)
                                if let Some(rest_identifier) = child.children(&mut child.walk()).find(|c| c.kind() == "identifier") {
                                    let var_name = self.base.get_node_text(&rest_identifier);
                                    let signature = format!("{} {{ ...{} }} = {}", declaration_type, var_name, value_text);

                                    let mut metadata = HashMap::new();
                                    metadata.insert("declarationType".to_string(), json!(declaration_type));
                                    metadata.insert("isDestructured".to_string(), json!(true));
                                    metadata.insert("destructuringType".to_string(), json!("object"));
                                    metadata.insert("isRestParameter".to_string(), json!(true));

                                    symbols.push(self.base.create_symbol(
                                        &node,
                                        var_name,
                                        SymbolKind::Variable,
                                        SymbolOptions {
                                            signature: Some(signature),
                                            visibility: Some(self.extract_visibility(&node)),
                                            parent_id: parent_id.clone(),
                                            metadata: Some(metadata),
                                            doc_comment: None,
                                        },
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "array_pattern" => {
                    // Handle array destructuring: const [first, second] = array (Miller's logic)
                    let mut index = 0;
                    for child in name.children(&mut name.walk()) {
                        if child.kind() == "identifier" {
                            let var_name = self.base.get_node_text(&child);
                            let signature = format!("{} [{}] = {}", declaration_type, var_name, value_text);

                            let mut metadata = HashMap::new();
                            metadata.insert("declarationType".to_string(), json!(declaration_type));
                            metadata.insert("isDestructured".to_string(), json!(true));
                            metadata.insert("destructuringType".to_string(), json!("array"));
                            metadata.insert("destructuringIndex".to_string(), json!(index));

                            symbols.push(self.base.create_symbol(
                                &node,
                                var_name,
                                SymbolKind::Variable,
                                SymbolOptions {
                                    signature: Some(signature),
                                    visibility: Some(self.extract_visibility(&node)),
                                    parent_id: parent_id.clone(),
                                    metadata: Some(metadata),
                                    doc_comment: None,
                                },
                            ));
                            index += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
    }

    /// Visit node for relationships - placeholder for relationship extraction
    fn visit_node_for_relationships(&self, node: tree_sitter::Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        // TODO: Implement relationship extraction following Miller's extractRelationships method
        // This is a placeholder to make the interface complete

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }
}