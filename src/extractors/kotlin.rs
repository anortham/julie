// Kotlin Extractor
//
// Port of Miller's Kotlin extractor to idiomatic Rust
// Original: /Users/murphy/Source/miller/src/extractors/kotlin-extractor.ts
//
// This extractor handles comprehensive Kotlin symbol extraction including:
// - Classes, data classes, sealed classes, enums
// - Objects, companion objects
// - Functions, extension functions, operators
// - Interfaces, type aliases, annotations
// - Generics with variance
// - Property delegation
// - Constructor parameters

use crate::extractors::base::{
    BaseExtractor, Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct KotlinExtractor {
    base: BaseExtractor,
}

impl KotlinExtractor {
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

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if !node.is_named() {
            return; // Skip unnamed nodes
        }

        let mut symbol: Option<Symbol> = None;
        let mut new_parent_id = parent_id.clone();

        match node.kind() {
            "class_declaration" | "enum_declaration" => {
                symbol = Some(self.extract_class(&node, parent_id.as_deref()));
            }
            "interface_declaration" => {
                symbol = Some(self.extract_interface(&node, parent_id.as_deref()));
            }
            "object_declaration" => {
                symbol = Some(self.extract_object(&node, parent_id.as_deref()));
            }
            "companion_object" => {
                symbol = Some(self.extract_companion_object(&node, parent_id.as_deref()));
            }
            "function_declaration" => {
                symbol = Some(self.extract_function(&node, parent_id.as_deref()));
            }
            "property_declaration" | "property_signature" => {
                symbol = Some(self.extract_property(&node, parent_id.as_deref()));
            }
            "enum_class_body" => {
                self.extract_enum_members(&node, symbols, parent_id.as_deref());
            }
            "primary_constructor" => {
                self.extract_constructor_parameters(&node, symbols, parent_id.as_deref());
            }
            "package_header" => {
                symbol = Some(self.extract_package(&node, parent_id.as_deref()));
            }
            "import" => {
                symbol = Some(self.extract_import(&node, parent_id.as_deref()));
            }
            "type_alias" => {
                symbol = Some(self.extract_type_alias(&node, parent_id.as_deref()));
            }
            _ => {}
        }

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            new_parent_id = Some(sym.id.clone());
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, new_parent_id.clone());
        }
    }

    fn extract_class(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownClass".to_string());

        // Check if this is actually an interface by looking for 'interface' child node
        let is_interface = node
            .children(&mut node.walk())
            .any(|n| n.kind() == "interface");

        let modifiers = self.extract_modifiers(node);
        let type_params = self.extract_type_parameters(node);
        let super_types = self.extract_super_types(node);
        let constructor_params = self.extract_primary_constructor_signature(node);

        // Determine if this is an enum class
        let is_enum = self.determine_class_kind(&modifiers, node) == SymbolKind::Enum;

        // Check for fun interface by looking for direct 'fun' child
        let has_fun_keyword = node
            .children(&mut node.walk())
            .any(|n| self.base.get_node_text(&n) == "fun");

        let mut signature = if is_interface {
            if has_fun_keyword {
                format!("fun interface {}", name)
            } else {
                format!("interface {}", name)
            }
        } else if is_enum {
            format!("enum class {}", name)
        } else {
            format!("class {}", name)
        };

        // For enum classes, don't include 'enum' in modifiers since it's already in the signature
        // For fun interfaces, don't include 'fun' in modifiers since it's already in the signature
        let final_modifiers: Vec<String> = if is_enum {
            modifiers.into_iter().filter(|m| m != "enum").collect()
        } else if has_fun_keyword {
            modifiers.into_iter().filter(|m| m != "fun").collect()
        } else {
            modifiers
        };

        if !final_modifiers.is_empty() {
            signature = format!("{} {}", final_modifiers.join(" "), signature);
        }

        if let Some(type_params) = type_params {
            signature.push_str(&type_params);
        }

        // Add primary constructor parameters to signature if present
        if let Some(constructor_params) = constructor_params {
            signature.push_str(&constructor_params);
        }

        if let Some(super_types) = super_types {
            signature.push_str(&format!(" : {}", super_types));
        }

        let symbol_kind = if is_interface {
            SymbolKind::Interface
        } else {
            self.determine_class_kind(&final_modifiers, node)
        };

        let visibility = self.determine_visibility(&final_modifiers);

        self.base.create_symbol(
            node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(HashMap::from([
                    ("type".to_string(), Value::String("class".to_string())),
                    (
                        "modifiers".to_string(),
                        Value::String(final_modifiers.join(",")),
                    ),
                ])),
                doc_comment: None,
            },
        )
    }

    fn extract_interface(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownInterface".to_string());

        let modifiers = self.extract_modifiers(node);
        let type_params = self.extract_type_parameters(node);
        let super_types = self.extract_super_types(node);

        let mut signature = format!("interface {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(type_params) = type_params {
            signature.push_str(&type_params);
        }

        if let Some(super_types) = super_types {
            signature.push_str(&format!(" : {}", super_types));
        }

        let visibility = self.determine_visibility(&modifiers);

        self.base.create_symbol(
            node,
            name,
            SymbolKind::Interface,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(HashMap::from([
                    ("type".to_string(), Value::String("interface".to_string())),
                    ("modifiers".to_string(), Value::String(modifiers.join(","))),
                ])),
                doc_comment: None,
            },
        )
    }

    fn extract_object(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownObject".to_string());

        let modifiers = self.extract_modifiers(node);
        let super_types = self.extract_super_types(node);

        let mut signature = format!("object {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(super_types) = super_types {
            signature.push_str(&format!(" : {}", super_types));
        }

        let visibility = self.determine_visibility(&modifiers);

        self.base.create_symbol(
            node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(HashMap::from([
                    ("type".to_string(), Value::String("object".to_string())),
                    ("modifiers".to_string(), Value::String(modifiers.join(","))),
                ])),
                doc_comment: None,
            },
        )
    }

    fn extract_companion_object(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        // Companion objects always have the name "Companion"
        let name = "Companion".to_string();

        let mut signature = "companion object".to_string();

        // Check if companion object has a custom name
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "identifier");
        if let Some(name_node) = name_node {
            let custom_name = self.base.get_node_text(&name_node);
            signature.push_str(&format!(" {}", custom_name));
        }

        self.base.create_symbol(
            node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(HashMap::from([(
                    "type".to_string(),
                    Value::String("companion-object".to_string()),
                )])),
                doc_comment: None,
            },
        )
    }

    fn extract_function(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownFunction".to_string());

        let modifiers = self.extract_modifiers(node);
        let type_params = self.extract_type_parameters(node);
        let receiver_type = self.extract_receiver_type(node);
        let parameters = self.extract_parameters(node);
        let return_type = self.extract_return_type(node);

        // Correct Kotlin signature order: modifiers + fun + typeParams + name
        let mut signature = "fun".to_string();

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(type_params) = type_params {
            signature.push_str(&format!(" {}", type_params));
        }

        // Add receiver type for extension functions (e.g., String.functionName)
        if let Some(receiver_type) = receiver_type {
            signature.push_str(&format!(" {}.{}", receiver_type, name));
        } else {
            signature.push_str(&format!(" {}", name));
        }

        signature.push_str(&parameters.unwrap_or_else(|| "()".to_string()));

        if let Some(return_type) = return_type {
            signature.push_str(&format!(": {}", return_type));
        }

        // Check for where clause (sibling node)
        if let Some(where_clause) = self.extract_where_clause(node) {
            signature.push_str(&format!(" {}", where_clause));
        }

        // Check for expression body (= expression)
        let function_body = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "function_body");
        if let Some(function_body) = function_body {
            let body_text = self.base.get_node_text(&function_body);
            if body_text.starts_with('=') {
                signature.push_str(&format!(" {}", body_text));
            }
        }

        // Determine symbol kind based on modifiers and context
        let symbol_kind = if modifiers.contains(&"operator".to_string()) {
            SymbolKind::Operator
        } else if parent_id.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };

        let visibility = self.determine_visibility(&modifiers);
        let return_type = self.extract_return_type(node);

        let mut metadata = HashMap::from([
            (
                "type".to_string(),
                Value::String(
                    if parent_id.is_some() {
                        "method"
                    } else {
                        "function"
                    }
                    .to_string(),
                ),
            ),
            ("modifiers".to_string(), Value::String(modifiers.join(","))),
        ]);

        // Store return type for type inference
        if let Some(return_type) = return_type {
            metadata.insert("returnType".to_string(), Value::String(return_type));
        }

        self.base.create_symbol(
            node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    fn extract_property(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        // Look for name in variable_declaration first (the proper place for property names)
        let mut name_node = None;
        let var_decl = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "variable_declaration");
        if let Some(var_decl) = var_decl {
            name_node = var_decl
                .children(&mut var_decl.walk())
                .find(|n| n.kind() == "identifier");
        }

        // Fallback: look for identifier at top level (for interface properties)
        if name_node.is_none() {
            name_node = node
                .children(&mut node.walk())
                .find(|n| n.kind() == "identifier");
        }
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownProperty".to_string());

        let modifiers = self.extract_modifiers(node);
        let property_type = self.extract_property_type(node);

        // Check for val/var in binding_pattern_kind for interface properties
        let mut is_val = node.children(&mut node.walk()).any(|n| n.kind() == "val");
        let mut is_var = node.children(&mut node.walk()).any(|n| n.kind() == "var");

        if !is_val && !is_var {
            let binding_pattern = node
                .children(&mut node.walk())
                .find(|n| n.kind() == "binding_pattern_kind");
            if let Some(binding_pattern) = binding_pattern {
                is_val = binding_pattern
                    .children(&mut binding_pattern.walk())
                    .any(|n| n.kind() == "val");
                is_var = binding_pattern
                    .children(&mut binding_pattern.walk())
                    .any(|n| n.kind() == "var");
            }
        }

        let binding = if is_val {
            "val"
        } else if is_var {
            "var"
        } else {
            "val"
        };
        let mut signature = format!("{} {}", binding, name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(property_type) = property_type {
            signature.push_str(&format!(": {}", property_type));
        }

        // Add initializer value if present (especially for const val)
        if let Some(initializer) = self.extract_property_initializer(node) {
            signature.push_str(&format!(" = {}", initializer));
        }

        // Check for property delegation (by lazy, by Delegates.notNull(), etc.)
        if let Some(delegation) = self.extract_property_delegation(node) {
            signature.push_str(&format!(" {}", delegation));
        }

        // Determine symbol kind - const val should be Constant
        let is_const = modifiers.contains(&"const".to_string());
        let symbol_kind = if is_const && is_val {
            SymbolKind::Constant
        } else {
            SymbolKind::Property
        };

        let visibility = self.determine_visibility(&modifiers);
        let property_type = self.extract_property_type(node);

        let mut metadata = HashMap::from([
            (
                "type".to_string(),
                Value::String(if is_const { "constant" } else { "property" }.to_string()),
            ),
            ("modifiers".to_string(), Value::String(modifiers.join(","))),
            ("isVal".to_string(), Value::String(is_val.to_string())),
            ("isVar".to_string(), Value::String(is_var.to_string())),
        ]);

        // Store property type for type inference
        if let Some(property_type) = property_type {
            metadata.insert("propertyType".to_string(), Value::String(property_type));
        }

        self.base.create_symbol(
            node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    fn extract_enum_members(
        &mut self,
        node: &Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "enum_entry" {
                let name_node = child
                    .children(&mut child.walk())
                    .find(|n| n.kind() == "identifier");
                if let Some(name_node) = name_node {
                    let name = self.base.get_node_text(&name_node);

                    // Check for constructor parameters
                    let mut signature = name.clone();
                    let value_args = child
                        .children(&mut child.walk())
                        .find(|n| n.kind() == "value_arguments");
                    if let Some(value_args) = value_args {
                        let args = self.base.get_node_text(&value_args);
                        signature.push_str(&args);
                    }

                    let symbol = self.base.create_symbol(
                        &child,
                        name,
                        SymbolKind::EnumMember,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: Some(HashMap::from([(
                                "type".to_string(),
                                Value::String("enum-member".to_string()),
                            )])),
                            doc_comment: None,
                        },
                    );
                    symbols.push(symbol);
                }
            }
        }
    }

    fn extract_package(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        // Look for qualified_identifier which contains the full package name
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "qualified_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownPackage".to_string());

        self.base.create_symbol(
            node,
            name.clone(),
            SymbolKind::Namespace,
            SymbolOptions {
                signature: Some(format!("package {}", name)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(HashMap::from([(
                    "type".to_string(),
                    Value::String("package".to_string()),
                )])),
                doc_comment: None,
            },
        )
    }

    fn extract_import(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        // Look for qualified_identifier which contains the full import name
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "qualified_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownImport".to_string());

        self.base.create_symbol(
            node,
            name.clone(),
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(format!("import {}", name)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(HashMap::from([(
                    "type".to_string(),
                    Value::String("import".to_string()),
                )])),
                doc_comment: None,
            },
        )
    }

    fn extract_type_alias(&mut self, node: &Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownTypeAlias".to_string());

        let modifiers = self.extract_modifiers(node);
        let type_params = self.extract_type_parameters(node);

        // Find the aliased type (after =) - may consist of multiple nodes
        let mut aliased_type = String::new();
        let children: Vec<Node> = node.children(&mut node.walk()).collect();
        if let Some(equal_index) = children
            .iter()
            .position(|n| self.base.get_node_text(n) == "=")
        {
            if equal_index + 1 < children.len() {
                // Concatenate all nodes after the = (e.g., "suspend" + "(T) -> Unit")
                let type_nodes = &children[equal_index + 1..];
                aliased_type = type_nodes
                    .iter()
                    .map(|n| self.base.get_node_text(n))
                    .collect::<Vec<String>>()
                    .join(" ");
            }
        }

        let mut signature = format!("typealias {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(type_params) = type_params {
            signature.push_str(&type_params);
        }

        if !aliased_type.is_empty() {
            signature.push_str(&format!(" = {}", aliased_type));
        }

        let visibility = self.determine_visibility(&modifiers);

        self.base.create_symbol(
            node,
            name,
            SymbolKind::Type,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(HashMap::from([
                    ("type".to_string(), Value::String("typealias".to_string())),
                    ("modifiers".to_string(), Value::String(modifiers.join(","))),
                    ("aliasedType".to_string(), Value::String(aliased_type)),
                ])),
                doc_comment: None,
            },
        )
    }

    // Helper methods for extraction

    fn extract_modifiers(&self, node: &Node) -> Vec<String> {
        let mut modifiers = Vec::new();
        let modifiers_list = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "modifiers");

        if let Some(modifiers_list) = modifiers_list {
            for child in modifiers_list.children(&mut modifiers_list.walk()) {
                // Handle modifier nodes, annotations, and direct keywords (backward compatibility)
                if matches!(
                    child.kind(),
                    "class_modifier"
                        | "function_modifier"
                        | "property_modifier"
                        | "visibility_modifier"
                        | "inheritance_modifier"
                        | "member_modifier"
                        | "annotation"
                        | "public"
                        | "private"
                        | "protected"
                        | "internal"
                        | "open"
                        | "final"
                        | "abstract"
                        | "sealed"
                        | "data"
                        | "inline"
                        | "suspend"
                        | "operator"
                        | "infix"
                ) {
                    modifiers.push(self.base.get_node_text(&child));
                }
            }
        }

        modifiers
    }

    fn extract_type_parameters(&self, node: &Node) -> Option<String> {
        let type_params = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "type_parameters");
        type_params.map(|tp| self.base.get_node_text(&tp))
    }

    fn extract_super_types(&self, node: &Node) -> Option<String> {
        let mut super_types = Vec::new();

        // Look for delegation_specifiers container first (wrapped case)
        let delegation_container = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "delegation_specifiers");
        if let Some(delegation_container) = delegation_container {
            for child in delegation_container.children(&mut delegation_container.walk()) {
                if child.kind() == "delegation_specifier" {
                    // Check for explicit_delegation (delegation syntax like "Drawable by drawable")
                    let explicit_delegation = child
                        .children(&mut child.walk())
                        .find(|n| n.kind() == "explicit_delegation");
                    if let Some(explicit_delegation) = explicit_delegation {
                        // Get the full delegation text including "by" keyword
                        super_types.push(self.base.get_node_text(&explicit_delegation));
                    } else {
                        // Fallback: simple inheritance without delegation
                        let type_node = child.children(&mut child.walk()).find(|n| {
                            matches!(
                                n.kind(),
                                "type" | "user_type" | "identifier" | "constructor_invocation"
                            )
                        });
                        if let Some(type_node) = type_node {
                            if type_node.kind() == "constructor_invocation" {
                                // For constructor invocations like Result<Nothing>(), include the full call
                                super_types.push(self.base.get_node_text(&type_node));
                            } else {
                                super_types.push(self.base.get_node_text(&type_node));
                            }
                        }
                    }
                } else if matches!(child.kind(), "type" | "user_type" | "identifier") {
                    super_types.push(self.base.get_node_text(&child));
                }
            }
        } else {
            // Look for individual delegation_specifier nodes (multiple at same level)
            let delegation_specifiers: Vec<Node> = node
                .children(&mut node.walk())
                .filter(|n| n.kind() == "delegation_specifier")
                .collect();
            for delegation in delegation_specifiers {
                let explicit_delegation = delegation
                    .children(&mut delegation.walk())
                    .find(|n| n.kind() == "explicit_delegation");
                if let Some(explicit_delegation) = explicit_delegation {
                    super_types.push(self.base.get_node_text(&explicit_delegation));
                } else {
                    // Check for constructor_invocation to include ()
                    let constructor_invocation = delegation
                        .children(&mut delegation.walk())
                        .find(|n| n.kind() == "constructor_invocation");
                    if let Some(constructor_invocation) = constructor_invocation {
                        super_types.push(self.base.get_node_text(&constructor_invocation));
                    } else {
                        super_types.push(self.base.get_node_text(&delegation));
                    }
                }
            }
        }

        if super_types.is_empty() {
            None
        } else {
            Some(super_types.join(", "))
        }
    }

    fn extract_parameters(&self, node: &Node) -> Option<String> {
        let params = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "function_value_parameters");
        params.map(|p| self.base.get_node_text(&p))
    }

    fn extract_return_type(&self, node: &Node) -> Option<String> {
        // Look for return type after the colon in function declarations
        let mut found_colon = false;
        for child in node.children(&mut node.walk()) {
            if child.kind() == ":" {
                found_colon = true;
                continue;
            }
            if found_colon
                && matches!(
                    child.kind(),
                    "type" | "user_type" | "identifier" | "function_type" | "nullable_type"
                )
            {
                return Some(self.base.get_node_text(&child));
            }
        }
        None
    }

    fn extract_property_initializer(&self, node: &Node) -> Option<String> {
        // Look for assignment (=) followed by initializer expression
        let children: Vec<Node> = node.children(&mut node.walk()).collect();
        if let Some(assignment_index) = children
            .iter()
            .position(|n| self.base.get_node_text(n) == "=")
        {
            if assignment_index + 1 < children.len() {
                let initializer_node = &children[assignment_index + 1];
                return Some(self.base.get_node_text(initializer_node).trim().to_string());
            }
        }

        // Also check for property_initializer node type
        let initializer_node = node
            .children(&mut node.walk())
            .find(|n| matches!(n.kind(), "property_initializer" | "expression" | "literal"));
        initializer_node.map(|n| self.base.get_node_text(&n).trim().to_string())
    }

    fn extract_property_delegation(&self, node: &Node) -> Option<String> {
        // Look for property_delegate or 'by' keyword
        let children: Vec<Node> = node.children(&mut node.walk()).collect();
        if let Some(by_index) = children
            .iter()
            .position(|n| self.base.get_node_text(n) == "by")
        {
            if by_index + 1 < children.len() {
                let delegate_node = &children[by_index + 1];
                return Some(format!("by {}", self.base.get_node_text(delegate_node)));
            }
        }

        // Also check for property_delegate node type
        let delegate_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "property_delegate");
        delegate_node.map(|n| self.base.get_node_text(&n))
    }

    fn extract_primary_constructor_signature(&self, node: &Node) -> Option<String> {
        // Look for primary_constructor node
        let primary_constructor = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "primary_constructor");
        let primary_constructor = primary_constructor?;

        // For now, just return the full primary constructor text
        // This ensures we capture the exact signature including val/var modifiers
        Some(self.base.get_node_text(&primary_constructor))
    }

    #[allow(dead_code)]
    fn process_class_parameter(&self, param_node: &Node, params: &mut Vec<String>) {
        let name_node = param_node
            .children(&mut param_node.walk())
            .find(|n| n.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownParam".to_string());

        // Get binding pattern (val/var)
        let binding_node = param_node
            .children(&mut param_node.walk())
            .find(|n| n.kind() == "binding_pattern_kind");
        let binding = binding_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "".to_string());

        // Get type
        let type_node = param_node.children(&mut param_node.walk()).find(|n| {
            matches!(
                n.kind(),
                "user_type" | "type" | "nullable_type" | "type_reference"
            )
        });
        let param_type = type_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "".to_string());

        // Get modifiers (like private)
        let modifiers_node = param_node
            .children(&mut param_node.walk())
            .find(|n| n.kind() == "modifiers");
        let modifiers = modifiers_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "".to_string());

        // Build parameter signature
        let mut param_sig = String::new();
        if !modifiers.is_empty() {
            param_sig.push_str(&format!("{} ", modifiers));
        }
        if !binding.is_empty() {
            param_sig.push_str(&format!("{} ", binding));
        }
        param_sig.push_str(&name);
        if !param_type.is_empty() {
            param_sig.push_str(&format!(": {}", param_type));
        }

        params.push(param_sig);
    }

    fn extract_where_clause(&self, node: &Node) -> Option<String> {
        // Look for type_constraints node as a child of the function declaration
        let type_constraints = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "type_constraints");

        if let Some(type_constraints) = type_constraints {
            // Extract each type_constraint and build the where clause
            let mut constraints = Vec::new();

            for child in type_constraints.children(&mut type_constraints.walk()) {
                if child.kind() == "type_constraint" {
                    constraints.push(self.base.get_node_text(&child));
                }
            }

            if !constraints.is_empty() {
                return Some(format!("where {}", constraints.join(", ")));
            }
        }

        None
    }

    fn extract_receiver_type(&self, node: &Node) -> Option<String> {
        // Look for extension function pattern: user_type + . + identifier
        let children: Vec<_> = node.children(&mut node.walk()).collect();

        // Find the pattern: user_type followed by "."
        for i in 0..children.len().saturating_sub(1) {
            if children[i].kind() == "user_type"
                && i + 1 < children.len()
                && self.base.get_node_text(&children[i + 1]) == "."
            {
                return Some(self.base.get_node_text(&children[i]));
            }
        }

        None
    }

    fn extract_property_type(&self, node: &Node) -> Option<String> {
        // Look for type in variable_declaration (interface properties)
        let var_decl = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "variable_declaration");
        if let Some(var_decl) = var_decl {
            let user_type = var_decl.children(&mut var_decl.walk()).find(|n| {
                matches!(
                    n.kind(),
                    "user_type" | "type" | "nullable_type" | "type_reference"
                )
            });
            if let Some(user_type) = user_type {
                return Some(self.base.get_node_text(&user_type));
            }
        }

        // Look for direct type node (regular properties)
        let property_type = node.children(&mut node.walk()).find(|n| {
            matches!(
                n.kind(),
                "type" | "user_type" | "nullable_type" | "type_reference"
            )
        });
        property_type.map(|n| self.base.get_node_text(&n))
    }

    fn determine_class_kind(&self, modifiers: &[String], node: &Node) -> SymbolKind {
        // Check if this is an enum declaration by node type
        if node.kind() == "enum_declaration" {
            return SymbolKind::Enum;
        }

        // Check for enum class by looking for 'enum' keyword in the node
        let has_enum_keyword = node
            .children(&mut node.walk())
            .any(|n| self.base.get_node_text(&n) == "enum");
        if has_enum_keyword {
            return SymbolKind::Enum;
        }

        // Check modifiers
        if modifiers.contains(&"enum".to_string()) || modifiers.contains(&"enum class".to_string())
        {
            return SymbolKind::Enum;
        }
        if modifiers.contains(&"data".to_string()) {
            return SymbolKind::Class;
        }
        if modifiers.contains(&"sealed".to_string()) {
            return SymbolKind::Class;
        }
        SymbolKind::Class
    }

    fn determine_visibility(&self, modifiers: &[String]) -> Visibility {
        if modifiers.contains(&"private".to_string()) {
            Visibility::Private
        } else if modifiers.contains(&"protected".to_string()) {
            Visibility::Protected
        } else {
            Visibility::Public // Kotlin defaults to public
        }
    }

    fn extract_constructor_parameters(
        &mut self,
        node: &Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        // First find the class_parameters container, then extract class_parameter nodes as properties
        let class_parameters = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "class_parameters");

        if let Some(class_parameters) = class_parameters {
            for child in class_parameters.children(&mut class_parameters.walk()) {
                if child.kind() == "class_parameter" {
                    let name_node = child
                        .children(&mut child.walk())
                        .find(|n| n.kind() == "identifier");
                    let name = name_node
                        .map(|n| self.base.get_node_text(&n))
                        .unwrap_or_else(|| "unknownParam".to_string());

                    // Get binding pattern (val/var)
                    let binding_node = child
                        .children(&mut child.walk())
                        .find(|n| matches!(n.kind(), "val" | "var"));
                    let binding = binding_node
                        .map(|n| self.base.get_node_text(&n))
                        .unwrap_or_else(|| "val".to_string());

                    // Get type (handle various type node structures including nullable)
                    let type_node = child.children(&mut child.walk()).find(|n| {
                        matches!(
                            n.kind(),
                            "user_type" | "type" | "nullable_type" | "type_reference"
                        )
                    });
                    let param_type = type_node
                        .map(|n| self.base.get_node_text(&n))
                        .unwrap_or_else(|| "".to_string());

                    // Get modifiers (like private)
                    let modifiers_node = child
                        .children(&mut child.walk())
                        .find(|n| n.kind() == "modifiers");
                    let modifiers = modifiers_node
                        .map(|n| self.base.get_node_text(&n))
                        .unwrap_or_else(|| "".to_string());

                    // Get default value (handle various literal types and expressions)
                    let default_value = child.children(&mut child.walk()).find(|n| {
                        matches!(
                            n.kind(),
                            "number_literal"
                                | "string_literal"
                                | "boolean_literal"
                                | "expression"
                                | "call_expression"
                        )
                    });
                    let default_val = default_value
                        .map(|n| format!(" = {}", self.base.get_node_text(&n)))
                        .unwrap_or_else(|| "".to_string());

                    // Alternative: look for assignment pattern (= value)
                    let final_signature = if default_val.is_empty() {
                        let children: Vec<Node> = child.children(&mut child.walk()).collect();
                        if let Some(equal_index) = children
                            .iter()
                            .position(|n| self.base.get_node_text(n) == "=")
                        {
                            if equal_index + 1 < children.len() {
                                let value_node = &children[equal_index + 1];
                                let default_assignment =
                                    format!(" = {}", self.base.get_node_text(value_node));
                                let signature2 = format!("{} {}", binding, name);
                                let final_sig = if !param_type.is_empty() {
                                    format!("{}: {}{}", signature2, param_type, default_assignment)
                                } else {
                                    format!("{}{}", signature2, default_assignment)
                                };
                                if !modifiers.is_empty() {
                                    format!("{} {}", modifiers, final_sig)
                                } else {
                                    final_sig
                                }
                            } else {
                                format!("{} {}", binding, name)
                            }
                        } else {
                            format!("{} {}", binding, name)
                        }
                    } else {
                        // Build signature
                        let mut signature = format!("{} {}", binding, name);
                        if !param_type.is_empty() {
                            signature.push_str(&format!(": {}", param_type));
                        }
                        signature.push_str(&default_val);

                        // Add modifiers to signature if present
                        if !modifiers.is_empty() {
                            format!("{} {}", modifiers, signature)
                        } else {
                            signature
                        }
                    };

                    // Determine visibility
                    let visibility = if modifiers.contains("private") {
                        Visibility::Private
                    } else if modifiers.contains("protected") {
                        Visibility::Protected
                    } else {
                        Visibility::Public
                    };

                    let property_symbol = self.base.create_symbol(
                        &child,
                        name,
                        SymbolKind::Property,
                        SymbolOptions {
                            signature: Some(final_signature),
                            visibility: Some(visibility),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: Some(HashMap::from([
                                ("type".to_string(), Value::String("property".to_string())),
                                ("binding".to_string(), Value::String(binding)),
                                ("dataType".to_string(), Value::String(param_type)),
                                (
                                    "hasDefaultValue".to_string(),
                                    Value::String((!default_val.is_empty()).to_string()),
                                ),
                            ])),
                            doc_comment: None,
                        },
                    );

                    symbols.push(property_symbol);
                }
            }
        }
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            if let Some(Value::String(s)) =
                symbol.metadata.as_ref().and_then(|m| m.get("returnType"))
            {
                types.insert(symbol.id.clone(), s.clone());
            } else if let Some(Value::String(s)) =
                symbol.metadata.as_ref().and_then(|m| m.get("propertyType"))
            {
                types.insert(symbol.id.clone(), s.clone());
            } else if let Some(Value::String(s)) =
                symbol.metadata.as_ref().and_then(|m| m.get("dataType"))
            {
                types.insert(symbol.id.clone(), s.clone());
            }
        }
        types
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_node_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn visit_node_for_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_declaration"
            | "enum_declaration"
            | "object_declaration"
            | "interface_declaration" => {
                // Process inheritance relationships for this class/interface/enum
                self.extract_inheritance_relationships(&node, symbols, relationships);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    fn extract_inheritance_relationships(
        &self,
        node: &Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let class_symbol = self.find_class_symbol(node, symbols);
        if class_symbol.is_none() {
            // Could not find class symbol, skipping relationship extraction
            return;
        }
        let class_symbol = class_symbol.unwrap();
        // Found class symbol, extracting relationships

        // Look for delegation_specifiers container first (wrapped case)
        let delegation_container = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "delegation_specifiers");
        let mut base_type_names = Vec::new();

        // Look for delegation_specifiers to find inheritance/interface implementation
        if let Some(delegation_container) = delegation_container {
            // Found delegation_specifiers container
            for child in delegation_container.children(&mut delegation_container.walk()) {
                // Process delegation child node
                if child.kind() == "delegation_specifier" {
                    // Found delegation_specifier, extracting base type
                    let type_node = child.children(&mut child.walk()).find(|n| {
                        matches!(
                            n.kind(),
                            "type" | "user_type" | "identifier" | "constructor_invocation"
                        )
                    });
                    if let Some(type_node) = type_node {
                        let base_type = if type_node.kind() == "constructor_invocation" {
                            // For constructor invocations like Widget(), extract just the type name
                            let user_type_node = type_node
                                .children(&mut type_node.walk())
                                .find(|n| n.kind() == "user_type");
                            if let Some(user_type_node) = user_type_node {
                                self.base.get_node_text(&user_type_node)
                            } else {
                                // Fallback: strip parentheses
                                let full_text = self.base.get_node_text(&type_node);
                                full_text
                                    .split('(')
                                    .next()
                                    .unwrap_or(&full_text)
                                    .to_string()
                            }
                        } else {
                            self.base.get_node_text(&type_node)
                        };
                        // Extracted base type successfully
                        base_type_names.push(base_type);
                    } else {
                        // No type node found in delegation_specifier
                    }
                } else if child.kind() == "delegated_super_type" {
                    let type_node = child
                        .children(&mut child.walk())
                        .find(|n| matches!(n.kind(), "type" | "user_type" | "identifier"));
                    if let Some(type_node) = type_node {
                        base_type_names.push(self.base.get_node_text(&type_node));
                    }
                } else if matches!(child.kind(), "type" | "user_type" | "identifier") {
                    base_type_names.push(self.base.get_node_text(&child));
                }
            }
        } else {
            // Look for individual delegation_specifier nodes (multiple at same level)
            // No delegation_specifiers container, looking for individual delegation_specifier nodes
            let delegation_specifiers: Vec<Node> = node
                .children(&mut node.walk())
                .filter(|n| n.kind() == "delegation_specifier")
                .collect();
            // Found individual delegation_specifier nodes
            for delegation in delegation_specifiers {
                // Extract just the type name from the delegation (remove "by delegate" part)
                let explicit_delegation = delegation
                    .children(&mut delegation.walk())
                    .find(|n| n.kind() == "explicit_delegation");
                if let Some(explicit_delegation) = explicit_delegation {
                    let type_text = self.base.get_node_text(&explicit_delegation);
                    let type_name = type_text.split(" by ").next().unwrap_or(&type_text); // Get part before "by"
                    base_type_names.push(type_name.to_string());
                } else {
                    // Extract type nodes directly - handle both user_type and constructor_invocation
                    let type_node = delegation.children(&mut delegation.walk()).find(|n| {
                        matches!(
                            n.kind(),
                            "type" | "user_type" | "identifier" | "constructor_invocation"
                        )
                    });
                    if let Some(type_node) = type_node {
                        if type_node.kind() == "constructor_invocation" {
                            // For constructor invocations like Widget(), extract just the type name
                            let user_type_node = type_node
                                .children(&mut type_node.walk())
                                .find(|n| n.kind() == "user_type");
                            if let Some(user_type_node) = user_type_node {
                                base_type_names.push(self.base.get_node_text(&user_type_node));
                            }
                        } else {
                            base_type_names.push(self.base.get_node_text(&type_node));
                        }
                    }
                }
            }
        }

        // Extracted base types, creating relationships

        // Create relationships for each base type
        for base_type_name in base_type_names {
            // Find the actual base type symbol
            let base_type_symbol = symbols.iter().find(|s| {
                s.name == base_type_name
                    && matches!(
                        s.kind,
                        SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct
                    )
            });

            if let Some(base_type_symbol) = base_type_symbol {
                // Determine relationship kind: classes extend, interfaces implement
                let relationship_kind = if base_type_symbol.kind == SymbolKind::Interface {
                    RelationshipKind::Implements
                } else {
                    RelationshipKind::Extends
                };

                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol.id,
                        base_type_symbol.id,
                        relationship_kind,
                        node.start_position().row
                    ),
                    from_symbol_id: class_symbol.id.clone(),
                    to_symbol_id: base_type_symbol.id.clone(),
                    kind: relationship_kind,
                    file_path: self.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some(HashMap::from([(
                        "baseType".to_string(),
                        Value::String(base_type_name),
                    )])),
                });
            }
        }
    }

    fn find_class_symbol<'a>(&self, node: &Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
        let name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "identifier");
        let class_name = name_node.map(|n| self.base.get_node_text(&n))?;

        symbols.iter().find(|s| {
            s.name == class_name
                && matches!(s.kind, SymbolKind::Class | SymbolKind::Interface)
                && s.file_path == self.base.file_path
        })
    }

    // ========================================================================
    // Identifier Extraction (for LSP-quality find_references)
    // ========================================================================

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

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
            // Function/method calls: foo(), bar.baz()
            "call_expression" => {
                // Kotlin call_expression structure:
                // - For simple calls: identifier is direct child
                // - For member calls: navigation_expression is child

                // Try to find identifier first (simple function calls)
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" || child.kind() == "simple_identifier" {
                        let name = self.base.get_node_text(&child);
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        return;
                    } else if child.kind() == "navigation_expression" {
                        // For member access calls, extract the rightmost identifier
                        if let Some((name_node, name)) = self.extract_rightmost_identifier(&child) {
                            let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &name_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                        return;
                    }
                }
            }

            // Member access: object.property
            "navigation_expression" => {
                // Only extract if it's NOT part of a call_expression
                // (we handle those in the call_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression" {
                        return; // Skip - handled by call_expression
                    }
                }

                // Extract the rightmost identifier (the member name)
                if let Some((name_node, name)) = self.extract_rightmost_identifier(&node) {
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            _ => {
                // Skip other node types for now
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

    /// Helper to extract the rightmost identifier in a navigation_expression
    /// Returns both the node and the extracted text
    fn extract_rightmost_identifier<'a>(&self, node: &Node<'a>) -> Option<(Node<'a>, String)> {
        // Kotlin navigation_expression structure is similar to Swift
        // For chained access like user.account.balance:
        // - We need to find the rightmost identifier

        // First, try to find identifier children (rightmost in chain)
        let identifiers: Vec<Node> = node
            .children(&mut node.walk())
            .filter(|n| n.kind() == "identifier" || n.kind() == "simple_identifier")
            .collect();

        if let Some(last_identifier) = identifiers.last() {
            let name = self.base.get_node_text(last_identifier);
            return Some((*last_identifier, name));
        }

        None
    }
}
