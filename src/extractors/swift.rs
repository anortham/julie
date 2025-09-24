use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, Visibility, SymbolOptions};
use tree_sitter::{Tree, Node};
use std::collections::HashMap;
use serde_json;

/// Swift extractor for extracting symbols and relationships from Swift source code
/// Port of Miller's comprehensive Swift extractor with full Swift language support
pub struct SwiftExtractor {
    base: BaseExtractor,
}

impl SwiftExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    /// Extract all symbols from Swift source code
    /// Port of Miller's extractSymbols method with comprehensive Swift support
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if !node.is_named() {
            return;
        }

        let mut symbol: Option<Symbol> = None;
        let mut current_parent_id = parent_id.clone();

        match node.kind() {
            "class_declaration" => {
                symbol = Some(self.extract_class(node, parent_id.as_deref()));
            }
            "struct_declaration" => {
                symbol = Some(self.extract_struct(node, parent_id.as_deref()));
            }
            "protocol_declaration" => {
                symbol = Some(self.extract_protocol(node, parent_id.as_deref()));
            }
            "enum_declaration" => {
                symbol = Some(self.extract_enum(node, parent_id.as_deref()));
            }
            "enum_case_declaration" => {
                self.extract_enum_cases(node, symbols, parent_id.as_deref());
            }
            "enum_entry" => {
                symbol = Some(self.extract_enum_case(node, parent_id.as_deref()));
            }
            "function_declaration" => {
                symbol = Some(self.extract_function(node, parent_id.as_deref()));
            }
            "protocol_function_declaration" => {
                symbol = Some(self.extract_protocol_function(node, parent_id.as_deref()));
            }
            "protocol_property_declaration" => {
                symbol = Some(self.extract_protocol_property(node, parent_id.as_deref()));
            }
            "associatedtype_declaration" => {
                symbol = Some(self.extract_associated_type(node, parent_id.as_deref()));
            }
            "subscript_declaration" => {
                symbol = Some(self.extract_subscript(node, parent_id.as_deref()));
            }
            "init_declaration" => {
                symbol = Some(self.extract_initializer(node, parent_id.as_deref()));
            }
            "deinit_declaration" => {
                symbol = Some(self.extract_deinitializer(node, parent_id.as_deref()));
            }
            "variable_declaration" => {
                if let Some(var_symbol) = self.extract_variable(node, parent_id.as_deref()) {
                    symbol = Some(var_symbol);
                }
            }
            "property_declaration" => {
                symbol = Some(self.extract_property(node, parent_id.as_deref()));
            }
            "extension_declaration" => {
                symbol = Some(self.extract_extension(node, parent_id.as_deref()));
            }
            "import_declaration" => {
                symbol = Some(self.extract_import(node, parent_id.as_deref()));
            }
            "typealias_declaration" => {
                symbol = Some(self.extract_type_alias(node, parent_id.as_deref()));
            }
            _ => {}
        }

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            current_parent_id = Some(sym.id.clone());
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    // Port of Miller's extractClass method with full Swift class support
    fn extract_class(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        // Swift parser uses class_declaration for classes
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier" || c.kind() == "user_type");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownClass".to_string());

        // Check what type this actually is
        let is_enum = node.children(&mut node.walk()).any(|c| c.kind() == "enum");
        let is_struct = node.children(&mut node.walk()).any(|c| c.kind() == "struct");
        let is_extension = node.children(&mut node.walk()).any(|c| c.kind() == "extension");

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);
        let inheritance = self.extract_inheritance(node);

        // Determine the correct keyword and symbol kind
        let (keyword, symbol_kind) = if is_enum {
            // Check for indirect modifier for enums
            let is_indirect = node.children(&mut node.walk()).any(|c| c.kind() == "indirect");
            if is_indirect {
                ("indirect enum", SymbolKind::Enum)
            } else {
                ("enum", SymbolKind::Enum)
            }
        } else if is_struct {
            ("struct", SymbolKind::Struct)
        } else if is_extension {
            ("extension", SymbolKind::Class)
        } else {
            ("class", SymbolKind::Class)
        };

        let mut signature = format!("{} {}", keyword, name);

        // For enums with indirect modifier, don't add modifiers again
        let is_enum_with_indirect = is_enum && keyword.contains("indirect");
        if !modifiers.is_empty() && !is_enum_with_indirect {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        if let Some(ref inheritance) = inheritance {
            signature.push_str(&format!(": {}", inheritance));
        }

        // Add where clause if present
        if let Some(where_clause) = self.extract_where_clause(node) {
            signature.push_str(&format!(" {}", where_clause));
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("class".to_string())),
            ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
        ]);

        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractStruct method
    fn extract_struct(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownStruct".to_string());

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);
        let conformance = self.extract_inheritance(node);

        let mut signature = format!("struct {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        if let Some(ref conformance) = conformance {
            signature.push_str(&format!(": {}", conformance));
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("struct".to_string())),
            ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Struct,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractProtocol method
    fn extract_protocol(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownProtocol".to_string());

        let modifiers = self.extract_modifiers(node);
        let inheritance = self.extract_inheritance(node);

        let mut signature = format!("protocol {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref inheritance) = inheritance {
            signature.push_str(&format!(": {}", inheritance));
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("protocol".to_string())),
            ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Interface,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractEnum method
    fn extract_enum(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownEnum".to_string());

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);
        let inheritance = self.extract_inheritance(node);

        let mut signature = format!("enum {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        if let Some(ref inheritance) = inheritance {
            signature.push_str(&format!(": {}", inheritance));
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("enum".to_string())),
            ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Enum,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractEnumCases method
    fn extract_enum_cases(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "enum_case_element" {
                let name_node = child.children(&mut child.walk())
                    .find(|c| c.kind() == "pattern" || c.kind() == "type_identifier");
                if let Some(name_node) = name_node {
                    let name = self.base.get_node_text(&name_node);
                    let associated_values = child.children(&mut child.walk())
                        .find(|c| c.kind() == "enum_case_parameters");

                    let mut signature = name.clone();
                    if let Some(associated_values) = associated_values {
                        signature.push_str(&self.base.get_node_text(&associated_values));
                    }

                    let metadata = HashMap::from([
                        ("type".to_string(), serde_json::Value::String("enum-case".to_string())),
                    ]);

                    let symbol = self.base.create_symbol(
                        &child,
                        name,
                        SymbolKind::EnumMember,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: Some(metadata),
                            doc_comment: None,
                        },
                    );
                    symbols.push(symbol);
                }
            }
        }
    }

    // Port of Miller's extractEnumCase method
    fn extract_enum_case(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "simple_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownCase".to_string());

        let mut signature = name.clone();

        // Check for associated values
        if let Some(associated_values) = node.children(&mut node.walk())
            .find(|c| c.kind() == "enum_type_parameters") {
            signature.push_str(&self.base.get_node_text(&associated_values));
        }

        // Check for raw values
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        if let Some(equal_index) = children.iter().position(|c| c.kind() == "=") {
            if let Some(raw_value_node) = children.get(equal_index + 1) {
                signature.push_str(&format!(" = {}", self.base.get_node_text(raw_value_node)));
            }
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("enum-case".to_string())),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::EnumMember,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractFunction method
    fn extract_function(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "simple_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownFunction".to_string());

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);
        let parameters = self.extract_parameters(node);
        let return_type = self.extract_return_type(node);

        let mut signature = format!("func {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        let params_str = parameters.unwrap_or_else(|| "()".to_string());
        let return_str = return_type.unwrap_or_else(|| "Void".to_string());

        signature.push_str(&params_str);

        if !return_str.is_empty() && return_str != "Void" {
            signature.push_str(&format!(" -> {}", return_str));
        }

        // Functions inside classes/structs are methods
        let symbol_kind = if parent_id.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("function".to_string())),
            ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
            ("parameters".to_string(), serde_json::Value::String(params_str)),
            ("returnType".to_string(), serde_json::Value::String(return_str)),
        ]);

        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractInitializer method
    fn extract_initializer(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = "init".to_string();
        let modifiers = self.extract_modifiers(node);
        let parameters = self.extract_initializer_parameters(node);

        let params_str = parameters.unwrap_or_else(|| "()".to_string());
        let mut signature = format!("init{}", params_str);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("initializer".to_string())),
            ("parameters".to_string(), serde_json::Value::String(params_str)),
            ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Constructor,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractDeinitializer method
    fn extract_deinitializer(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = "deinit".to_string();
        let signature = "deinit".to_string();

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("deinitializer".to_string())),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Destructor,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractVariable method
    fn extract_variable(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let binding_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "property_binding_pattern" || c.kind() == "pattern_binding");

        if let Some(binding_node) = binding_node {
            let name_node = binding_node.children(&mut binding_node.walk())
                .find(|c| c.kind() == "simple_identifier" || c.kind() == "pattern");
            let name = name_node
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "unknownVariable".to_string());

            let modifiers = self.extract_modifiers(node);
            let var_type = self.extract_variable_type(node);
            let is_let = node.children(&mut node.walk()).any(|c| c.kind() == "let");
            let is_var = node.children(&mut node.walk()).any(|c| c.kind() == "var");

            let mut signature = if is_let {
                format!("let {}", name)
            } else if is_var {
                format!("var {}", name)
            } else {
                format!("var {}", name)
            };

            if !modifiers.is_empty() {
                signature = format!("{} {}", modifiers.join(" "), signature);
            }

            if let Some(ref var_type) = var_type {
                signature.push_str(&format!(": {}", var_type));
            }

            let metadata = HashMap::from([
                ("type".to_string(), serde_json::Value::String("variable".to_string())),
                ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
                ("variableType".to_string(), serde_json::Value::String(var_type.unwrap_or_else(|| "Any".to_string()))),
                ("isLet".to_string(), serde_json::Value::String(is_let.to_string())),
                ("isVar".to_string(), serde_json::Value::String(is_var.to_string())),
            ]);

            Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(self.determine_visibility(&modifiers)),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some(metadata),
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    // Port of Miller's extractProperty method
    fn extract_property(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "pattern");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownProperty".to_string());

        let modifiers = self.extract_modifiers(node);
        let property_type = self.extract_property_type(node);

        // Extract the property keyword (var or let)
        let binding_pattern = node.children(&mut node.walk())
            .find(|c| c.kind() == "value_binding_pattern");
        let keyword = if let Some(binding_pattern) = binding_pattern {
            binding_pattern.children(&mut binding_pattern.walk())
                .find(|c| c.kind() == "var" || c.kind() == "let")
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "var".to_string())
        } else {
            "var".to_string()
        };

        // Build signature with non-visibility modifiers
        let non_visibility_modifiers: Vec<_> = modifiers.iter()
            .filter(|m| !["public", "private", "internal", "fileprivate", "open"].contains(&m.as_str()))
            .cloned()
            .collect();

        let mut signature = if !non_visibility_modifiers.is_empty() {
            format!("{} {} {}", non_visibility_modifiers.join(" "), keyword, name)
        } else {
            format!("{} {}", keyword, name)
        };

        if let Some(ref property_type) = property_type {
            signature.push_str(&format!(": {}", property_type));
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("property".to_string())),
            ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
            ("propertyType".to_string(), serde_json::Value::String(property_type.unwrap_or_else(|| "Any".to_string()))),
            ("keyword".to_string(), serde_json::Value::String(keyword)),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractProtocolFunction method
    fn extract_protocol_function(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "simple_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownFunction".to_string());

        let parameters = self.extract_parameters(node);
        let return_type = self.extract_return_type(node);

        let params_str = parameters.unwrap_or_else(|| "()".to_string());
        let return_str = return_type.unwrap_or_else(|| "Void".to_string());

        let mut signature = format!("func {}", name);
        signature.push_str(&params_str);

        if !return_str.is_empty() && return_str != "Void" {
            signature.push_str(&format!(" -> {}", return_str));
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("protocol-requirement".to_string())),
            ("parameters".to_string(), serde_json::Value::String(params_str)),
            ("returnType".to_string(), serde_json::Value::String(return_str)),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractProtocolProperty method
    fn extract_protocol_property(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let pattern_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "pattern");
        let name = if let Some(pattern_node) = pattern_node {
            pattern_node.children(&mut pattern_node.walk())
                .find(|c| c.kind() == "simple_identifier")
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "unknownProperty".to_string())
        } else {
            "unknownProperty".to_string()
        };

        // Check for static modifier
        let modifiers_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "modifiers");
        let is_static = modifiers_node
            .map(|modifiers_node| {
                modifiers_node.children(&mut modifiers_node.walk())
                    .any(|c| c.kind() == "property_modifier" && self.base.get_node_text(&c) == "static")
            })
            .unwrap_or(false);

        let property_type = self.extract_property_type(node);

        // Extract getter/setter requirements
        let protocol_requirements = node.children(&mut node.walk())
            .find(|c| c.kind() == "protocol_property_requirements");
        let accessors = protocol_requirements
            .map(|req| format!(" {}", self.base.get_node_text(&req)))
            .unwrap_or_else(|| String::new());

        let mut signature = if is_static {
            format!("static var {}", name)
        } else {
            format!("var {}", name)
        };

        if let Some(ref property_type) = property_type {
            signature.push_str(&format!(": {}", property_type));
        }

        if !accessors.is_empty() {
            signature.push_str(&accessors);
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("protocol-requirement".to_string())),
            ("propertyType".to_string(), serde_json::Value::String(property_type.unwrap_or_else(|| "Any".to_string()))),
            ("accessors".to_string(), serde_json::Value::String(accessors)),
            ("isStatic".to_string(), serde_json::Value::String(is_static.to_string())),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractAssociatedType method
    fn extract_associated_type(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier" || c.kind() == "simple_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownType".to_string());

        let mut signature = format!("associatedtype {}", name);

        // Check for type constraints
        if let Some(inheritance) = self.extract_inheritance(node) {
            signature.push_str(&format!(": {}", inheritance));
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("associatedtype".to_string())),
        ]);

        self.base.create_symbol(
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
        )
    }

    // Port of Miller's extractSubscript method
    fn extract_subscript(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = "subscript".to_string();
        let parameters = self.extract_parameters(node).unwrap_or_else(|| "()".to_string());
        let return_type = self.extract_return_type(node);
        let modifiers = self.extract_modifiers(node);

        let mut signature = "subscript".to_string();

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        signature.push_str(&parameters);

        if let Some(ref return_type) = return_type {
            signature.push_str(&format!(" -> {}", return_type));
        }

        // Check for accessor requirements
        if let Some(accessor_reqs) = node.children(&mut node.walk())
            .find(|c| c.kind() == "getter_setter_block" || c.kind() == "protocol_property_requirements") {
            signature.push_str(&format!(" {}", self.base.get_node_text(&accessor_reqs)));
        }

        let metadata = HashMap::from([
            ("type".to_string(), serde_json::Value::String("subscript".to_string())),
            ("parameters".to_string(), serde_json::Value::String(parameters)),
            ("returnType".to_string(), serde_json::Value::String(return_type.unwrap_or_else(|| "Any".to_string()))),
            ("modifiers".to_string(), serde_json::Value::String(modifiers.join(", "))),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractExtension method
    fn extract_extension(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let type_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = type_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownExtension".to_string());

        let modifiers = self.extract_modifiers(node);
        let conformance = self.extract_inheritance(node);

        let mut signature = format!("extension {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref conformance) = conformance {
            signature.push_str(&format!(": {}", conformance));
        }

        let metadata = HashMap::from([
            ("type".to_string(), "extension".to_string()),
            ("modifiers".to_string(), modifiers.join(", ")),
            ("extendedType".to_string(), name.clone()),
        ]);

        let options = self.create_symbol_options(
            Some(signature),
            Some(Visibility::Public),
            parent_id.map(|s| s.to_string()),
            metadata,
            None,
        );

        self.base.create_symbol(&node, name, SymbolKind::Class, options)
    }

    // Port of Miller's extractImport method
    fn extract_import(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownImport".to_string());

        let metadata = HashMap::from([
            ("type".to_string(), "import".to_string()),
        ]);

        let options = self.create_symbol_options(
            Some(format!("import {}", name)),
            Some(Visibility::Public),
            parent_id.map(|s| s.to_string()),
            metadata,
            None,
        );

        self.base.create_symbol(&node, name, SymbolKind::Import, options)
    }

    // Port of Miller's extractTypeAlias method
    fn extract_type_alias(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownTypeAlias".to_string());

        // Find the type that the alias refers to
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        let aliased_type = if let Some(equal_index) = children.iter().position(|c| self.base.get_node_text(c) == "=") {
            children.get(equal_index + 1)
                .map(|type_node| self.base.get_node_text(type_node))
                .unwrap_or_else(|| String::new())
        } else {
            String::new()
        };

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);

        let mut signature = format!("typealias {}", name);

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if !aliased_type.is_empty() {
            signature.push_str(&format!(" = {}", aliased_type));
        }

        let metadata = HashMap::from([
            ("type".to_string(), "typealias".to_string()),
            ("aliasedType".to_string(), aliased_type),
            ("modifiers".to_string(), modifiers.join(", ")),
        ]);

        let options = self.create_symbol_options(
            Some(signature),
            Some(self.determine_visibility(&modifiers)),
            parent_id.map(|s| s.to_string()),
            metadata,
            None,
        );

        self.base.create_symbol(&node, name, SymbolKind::Type, options)
    }

    // Helper method to create SymbolOptions with proper serde_json::Value metadata
    fn create_symbol_options(
        &self,
        signature: Option<String>,
        visibility: Option<Visibility>,
        parent_id: Option<String>,
        metadata: HashMap<String, String>,
        doc_comment: Option<String>
    ) -> SymbolOptions {
        let json_metadata: HashMap<String, serde_json::Value> = metadata
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();

        SymbolOptions {
            signature,
            visibility,
            parent_id,
            metadata: Some(json_metadata),
            doc_comment,
        }
    }

    // Port of Miller's extractModifiers method
    fn extract_modifiers(&self, node: Node) -> Vec<String> {
        let mut modifiers = Vec::new();

        if let Some(modifiers_list) = node.children(&mut node.walk())
            .find(|c| c.kind() == "modifiers") {
            for child in modifiers_list.children(&mut modifiers_list.walk()) {
                if matches!(child.kind(),
                    "visibility_modifier" | "mutation_modifier" | "declaration_modifier" |
                    "access_level_modifier" | "property_modifier" | "member_modifier"
                ) {
                    modifiers.push(self.base.get_node_text(&child));
                } else if matches!(child.kind(),
                    "public" | "private" | "internal" | "fileprivate" | "open" | "final" |
                    "static" | "class" | "override" | "lazy" | "weak" | "unowned" |
                    "required" | "convenience" | "dynamic"
                ) {
                    modifiers.push(self.base.get_node_text(&child));
                } else if child.kind() == "attribute" {
                    modifiers.push(self.base.get_node_text(&child));
                }
            }
        }

        // Check for direct modifier nodes
        for child in node.children(&mut node.walk()) {
            if child.kind() == "lazy" || self.base.get_node_text(&child) == "lazy" {
                modifiers.push("lazy".to_string());
            } else if child.kind() == "attribute" {
                modifiers.push(self.base.get_node_text(&child));
            }
        }

        modifiers
    }

    // Port of Miller's extractGenericParameters method
    fn extract_generic_parameters(&self, node: Node) -> Option<String> {
        node.children(&mut node.walk())
            .find(|c| c.kind() == "type_parameters")
            .map(|generic_params| self.base.get_node_text(&generic_params))
    }

    // Port of Miller's extractInheritance method
    fn extract_inheritance(&self, node: Node) -> Option<String> {
        // First try the standard type_inheritance_clause
        if let Some(inheritance) = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_inheritance_clause") {
            let types: Vec<_> = inheritance.children(&mut inheritance.walk())
                .filter(|c| c.kind() == "type_identifier" || c.kind() == "type")
                .map(|t| self.base.get_node_text(&t))
                .collect();
            if !types.is_empty() {
                return Some(types.join(", "));
            }
        }

        // For Swift enums, inheritance is represented as direct inheritance_specifier nodes
        let inheritance_specifiers: Vec<_> = node.children(&mut node.walk())
            .filter(|c| c.kind() == "inheritance_specifier")
            .collect();
        if !inheritance_specifiers.is_empty() {
            let types: Vec<_> = inheritance_specifiers.iter()
                .filter_map(|spec| {
                    spec.children(&mut spec.walk())
                        .find(|c| matches!(c.kind(), "user_type" | "type_identifier" | "type"))
                        .map(|type_node| self.base.get_node_text(&type_node))
                })
                .collect();
            if !types.is_empty() {
                return Some(types.join(", "));
            }
        }

        None
    }

    // Port of Miller's extractWhereClause method
    fn extract_where_clause(&self, node: Node) -> Option<String> {
        // Look for where clause in class/function declarations
        if let Some(where_clause) = node.children(&mut node.walk())
            .find(|c| matches!(c.kind(),
                "where_clause" | "generic_where_clause" | "type_constraints"
            ) || self.base.get_node_text(c).starts_with("where")) {
            return Some(self.base.get_node_text(&where_clause));
        }

        // Fallback: scan for any child containing "where"
        for child in node.children(&mut node.walk()) {
            let text = self.base.get_node_text(&child);
            if text.contains("where ") {
                if let Some(captures) = text.find("where ") {
                    let where_part = &text[captures..];
                    if let Some(end) = where_part.find('{') {
                        return Some(where_part[..end].trim().to_string());
                    } else {
                        return Some(where_part.trim().to_string());
                    }
                }
            }
        }

        None
    }

    // Port of Miller's extractParameters method
    fn extract_parameters(&self, node: Node) -> Option<String> {
        // First try parameter_clause
        if let Some(param_clause) = node.children(&mut node.walk())
            .find(|c| c.kind() == "parameter_clause") {
            return Some(self.base.get_node_text(&param_clause));
        }

        // For Swift functions, parameters are individual nodes between ( and )
        let parameters: Vec<_> = node.children(&mut node.walk())
            .filter(|c| c.kind() == "parameter")
            .map(|p| self.base.get_node_text(&p))
            .collect();

        if !parameters.is_empty() {
            return Some(format!("({})", parameters.join(", ")));
        }

        // Check if there are parentheses (indicating a function with no parameters)
        if node.children(&mut node.walk()).any(|c| c.kind() == "(") {
            Some("()".to_string())
        } else {
            None
        }
    }

    // Port of Miller's extractInitializerParameters method
    fn extract_initializer_parameters(&self, node: Node) -> Option<String> {
        // Look for parameter nodes
        if let Some(parameter_node) = node.children(&mut node.walk())
            .find(|c| c.kind() == "parameter") {
            return Some(format!("({})", self.base.get_node_text(&parameter_node)));
        }

        // Check if there are parentheses but no parameters
        if node.children(&mut node.walk()).any(|c| c.kind() == "(") {
            Some("()".to_string())
        } else {
            None
        }
    }

    // Port of Miller's extractReturnType method
    fn extract_return_type(&self, node: Node) -> Option<String> {
        // Try function_type first
        if let Some(return_clause) = node.children(&mut node.walk())
            .find(|c| c.kind() == "function_type") {
            if let Some(type_node) = return_clause.children(&mut return_clause.walk())
                .find(|c| c.kind() == "type") {
                return Some(self.base.get_node_text(&type_node));
            }
        }

        // Try type_annotation
        if let Some(type_annotation) = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_annotation") {
            if let Some(type_node) = type_annotation.children(&mut type_annotation.walk())
                .find(|c| matches!(c.kind(), "type" | "type_identifier" | "user_type")) {
                return Some(self.base.get_node_text(&type_node));
            }
        }

        // Try direct type nodes (for simple cases)
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        if let Some((node_index, direct_type)) = children.iter().enumerate()
            .find(|(_, c)| matches!(c.kind(), "type" | "type_identifier" | "user_type")) {
            let has_arrow = children.iter().take(node_index)
                .any(|child| self.base.get_node_text(child).contains("->"));
            if has_arrow {
                return Some(self.base.get_node_text(direct_type));
            }
        }

        None
    }

    // Port of Miller's extractVariableType method
    fn extract_variable_type(&self, node: Node) -> Option<String> {
        if let Some(type_annotation) = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_annotation") {
            if let Some(type_node) = type_annotation.children(&mut type_annotation.walk())
                .find(|c| matches!(c.kind(),
                    "type" | "user_type" | "primitive_type" | "optional_type" |
                    "function_type" | "tuple_type" | "dictionary_type" | "array_type"
                )) {
                return Some(self.base.get_node_text(&type_node));
            }
        }
        None
    }

    // Port of Miller's extractPropertyType method
    fn extract_property_type(&self, node: Node) -> Option<String> {
        self.extract_variable_type(node)
    }

    // Port of Miller's determineVisibility method
    fn determine_visibility(&self, modifiers: &[String]) -> Visibility {
        if modifiers.iter().any(|m| m == "private" || m == "fileprivate") {
            Visibility::Private
        } else if modifiers.iter().any(|m| m == "internal") {
            Visibility::Protected
        } else {
            Visibility::Public
        }
    }

    /// Extract relationships between Swift types (inheritance and protocol conformance)
    /// Port of Miller's extractRelationships method
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_node_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn visit_node_for_relationships(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        match node.kind() {
            "class_declaration" | "struct_declaration" | "extension_declaration" => {
                self.extract_inheritance_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    // Port of Miller's extractInheritanceRelationships method
    fn extract_inheritance_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        if let Some(type_symbol) = self.find_type_symbol(node, symbols) {
            // Try type_inheritance_clause first
            if let Some(inheritance) = node.children(&mut node.walk())
                .find(|c| c.kind() == "type_inheritance_clause") {
                for child in inheritance.children(&mut inheritance.walk()) {
                    if matches!(child.kind(), "type_identifier" | "type") {
                        let base_type_name = self.base.get_node_text(&child);
                        self.add_inheritance_relationship(&type_symbol, &base_type_name, symbols, relationships, node);
                    }
                }
            }

            // Also handle direct inheritance_specifier nodes
            for spec in node.children(&mut node.walk())
                .filter(|c| c.kind() == "inheritance_specifier") {
                if let Some(type_node) = spec.children(&mut spec.walk())
                    .find(|c| matches!(c.kind(), "user_type" | "type_identifier" | "type")) {
                    let base_type_name = if type_node.kind() == "user_type" {
                        if let Some(inner_type_node) = type_node.children(&mut type_node.walk())
                            .find(|c| c.kind() == "type_identifier") {
                            self.base.get_node_text(&inner_type_node)
                        } else {
                            self.base.get_node_text(&type_node)
                        }
                    } else {
                        self.base.get_node_text(&type_node)
                    };
                    self.add_inheritance_relationship(&type_symbol, &base_type_name, symbols, relationships, node);
                }
            }
        }
    }

    // Port of Miller's addInheritanceRelationship method
    fn add_inheritance_relationship(
        &self,
        type_symbol: &Symbol,
        base_type_name: &str,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
        node: Node,
    ) {
        // Find the actual base type symbol
        if let Some(base_type_symbol) = symbols.iter().find(|s| {
            s.name == base_type_name &&
            matches!(s.kind, SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct)
        }) {
            // Determine relationship kind: classes extend, protocols implement
            let relationship_kind = if base_type_symbol.kind == SymbolKind::Interface {
                RelationshipKind::Implements
            } else {
                RelationshipKind::Extends
            };

            let metadata = HashMap::from([
                ("baseType".to_string(), serde_json::Value::String(base_type_name.to_string())),
            ]);

            relationships.push(Relationship {
                from_symbol_id: type_symbol.id.clone(),
                to_symbol_id: base_type_symbol.id.clone(),
                kind: relationship_kind,
                file_path: self.base.file_path.clone(),
                line_number: (node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: Some(metadata),
            });
        }
    }

    // Port of Miller's inferTypes method
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            // For functions/methods, prefer returnType over generic type
            if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                if let Some(return_type) = symbol.metadata.get("returnType") {
                    if let Some(return_type_str) = return_type.as_str() {
                        types.insert(symbol.id.clone(), return_type_str.to_string());
                        continue;
                    }
                }
            }
            // For properties/variables, prefer propertyType or variableType
            else if matches!(symbol.kind, SymbolKind::Property | SymbolKind::Variable) {
                if let Some(property_type) = symbol.metadata.get("propertyType") {
                    if let Some(property_type_str) = property_type.as_str() {
                        types.insert(symbol.id.clone(), property_type_str.to_string());
                        continue;
                    }
                }
                if let Some(variable_type) = symbol.metadata.get("variableType") {
                    if let Some(variable_type_str) = variable_type.as_str() {
                        types.insert(symbol.id.clone(), variable_type_str.to_string());
                        continue;
                    }
                }
            }

            // Fallback to generic type from metadata
            if let Some(symbol_type) = symbol.metadata.get("type") {
                if let Some(symbol_type_str) = symbol_type.as_str() {
                    types.insert(symbol.id.clone(), symbol_type_str.to_string());
                }
            } else if let Some(return_type) = symbol.metadata.get("returnType") {
                if let Some(return_type_str) = return_type.as_str() {
                    types.insert(symbol.id.clone(), return_type_str.to_string());
                }
            }
        }
        types
    }

    // Port of Miller's findTypeSymbol method
    fn find_type_symbol(&self, node: Node, symbols: &[Symbol]) -> Option<Symbol> {
        if let Some(name_node) = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier") {
            let type_name = self.base.get_node_text(&name_node);
            symbols.iter()
                .find(|s| {
                    s.name == type_name &&
                    matches!(s.kind, SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface) &&
                    s.file_path == self.base.file_path
                })
                .cloned()
        } else {
            None
        }
    }
}