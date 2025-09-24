// C Extractor (TDD GREEN phase implementation)
//
// Port of Miller's C extractor using proven extraction logic with idiomatic Rust
// Original: /Users/murphy/Source/miller/src/extractors/c-extractor.ts

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, SymbolOptions, Visibility};
use tree_sitter::Tree;
use std::collections::HashMap;
use serde_json::Value;

pub struct CExtractor {
    base: BaseExtractor,
}

impl CExtractor {
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
        self.extract_relationships_from_node(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn visit_node(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if !node.is_named() {
            return;
        }

        let mut symbol: Option<Symbol> = None;

        // Port Miller's switch statement logic for C constructs
        match node.kind() {
            "preproc_include" => {
                symbol = Some(self.extract_include(node, parent_id.as_deref()));
            }
            "preproc_def" | "preproc_function_def" => {
                symbol = Some(self.extract_macro(node, parent_id.as_deref()));
            }
            "declaration" => {
                let declaration_symbols = self.extract_declaration(node, parent_id.as_deref());
                symbols.extend(declaration_symbols);
            }
            "function_definition" => {
                symbol = Some(self.extract_function_definition(node, parent_id.as_deref()));
            }
            "struct_specifier" => {
                symbol = Some(self.extract_struct(node, parent_id.as_deref()));
            }
            "enum_specifier" => {
                symbol = Some(self.extract_enum(node, parent_id.as_deref()));
                // Also extract enum values as separate constants
                if let Some(ref enum_symbol) = symbol {
                    let enum_values = self.extract_enum_value_symbols(node, &enum_symbol.id);
                    symbols.extend(enum_values);
                }
            }
            "type_definition" => {
                symbol = Some(self.extract_type_definition(node, parent_id.as_deref()));
            }
            "linkage_specification" => {
                symbol = self.extract_linkage_specification(node, parent_id.as_deref());
            }
            "expression_statement" => {
                // Handle cases like "} PACKED NetworkHeader;" where NetworkHeader is in expression_statement
                symbol = self.extract_from_expression_statement(node, parent_id.as_deref());
            }
            _ => {}
        }

        let current_parent_id = if let Some(sym) = symbol {
            let symbol_id = sym.id.clone();
            symbols.push(sym);
            Some(symbol_id)
        } else {
            parent_id
        };

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    // Include extraction - Miller's extractInclude
    fn extract_include(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Symbol {
        let signature = self.base.get_node_text(&node);
        let include_path = self.extract_include_path(&signature);

        let metadata = self.create_metadata_map(HashMap::from([
            ("type".to_string(), "include".to_string()),
            ("path".to_string(), include_path.clone()),
            ("isSystemHeader".to_string(), self.is_system_header(&signature).to_string()),
        ]));

        self.base.create_symbol(
            &node,
            include_path.clone(),
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(signature.clone()),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Macro extraction - Miller's extractMacro
    fn extract_macro(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Symbol {
        let signature = self.base.get_node_text(&node);
        let macro_name = self.extract_macro_name(node);

        let metadata = self.create_metadata_map(HashMap::from([
            ("type".to_string(), "macro".to_string()),
            ("name".to_string(), macro_name.clone()),
            ("isFunctionLike".to_string(), (node.kind() == "preproc_function_def").to_string()),
            ("definition".to_string(), signature.clone()),
        ]));

        self.base.create_symbol(
            &node,
            macro_name.clone(),
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature.clone()),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    // Declaration extraction - Miller's extractDeclaration
    fn extract_declaration(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Check if this is a function declaration
        if let Some(function_declarator) = self.find_function_declarator(node) {
            if let Some(function_symbol) = self.extract_function_declaration(node, parent_id) {
                symbols.push(function_symbol);
                return symbols;
            }
        }

        // Extract variable declarations
        let declarators = self.find_variable_declarators(node);
        for declarator in declarators {
            if let Some(variable_symbol) = self.extract_variable_declaration(node, declarator, parent_id) {
                symbols.push(variable_symbol);
            }
        }

        symbols
    }

    // Function definition extraction - Miller's extractFunctionDefinition
    fn extract_function_definition(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Symbol {
        let function_name = self.extract_function_name(node);
        let signature = self.build_function_signature(node);
        let visibility = if self.is_static_function(node) { "private" } else { "public" };

        self.create_symbol(
            &node,
            function_name.clone(),
            SymbolKind::Function,
            Some(signature),
            visibility,
            parent_id,
            Some(HashMap::from([
                ("type".to_string(), "function".to_string()),
                ("name".to_string(), function_name),
                ("returnType".to_string(), self.extract_return_type(node)),
                ("parameters".to_string(), self.extract_function_parameters(node).join(", ")),
                ("isDefinition".to_string(), "true".to_string()),
                ("isStatic".to_string(), self.is_static_function(node).to_string()),
            ])),
        )
    }

    // Function declaration extraction - Miller's extractFunctionDeclaration
    fn extract_function_declaration(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        let function_name = self.extract_function_name_from_declaration(node);
        let signature = self.build_function_declaration_signature(node);
        let visibility = if self.is_static_function(node) { "private" } else { "public" };

        Some(self.create_symbol(
            &node,
            function_name.clone(),
            SymbolKind::Function,
            Some(signature),
            visibility,
            parent_id,
            Some(HashMap::from([
                ("type".to_string(), "function".to_string()),
                ("name".to_string(), function_name),
                ("returnType".to_string(), self.extract_return_type_from_declaration(node)),
                ("parameters".to_string(), self.extract_function_parameters_from_declaration(node).join(", ")),
                ("isDefinition".to_string(), "false".to_string()),
                ("isStatic".to_string(), self.is_static_function(node).to_string()),
            ])),
        ))
    }

    // Variable declaration extraction - Miller's extractVariableDeclaration
    fn extract_variable_declaration(&mut self, node: tree_sitter::Node, declarator: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        let variable_name = self.extract_variable_name(declarator);
        let signature = self.build_variable_signature(node, declarator);
        let visibility = if self.is_static_variable(node) { "private" } else { "public" };

        Some(self.create_symbol(
            &node,
            variable_name.clone(),
            SymbolKind::Variable,
            Some(signature),
            visibility,
            parent_id,
            Some(HashMap::from([
                ("type".to_string(), "variable".to_string()),
                ("name".to_string(), variable_name),
                ("dataType".to_string(), self.extract_variable_type(node)),
                ("isStatic".to_string(), self.is_static_variable(node).to_string()),
                ("isExtern".to_string(), self.is_extern_variable(node).to_string()),
                ("isConst".to_string(), self.is_const_variable(node).to_string()),
                ("isVolatile".to_string(), self.is_volatile_variable(node).to_string()),
                ("isArray".to_string(), self.is_array_variable(declarator).to_string()),
                ("initializer".to_string(), self.extract_initializer(declarator).unwrap_or_default()),
            ])),
        ))
    }

    // Struct extraction - Miller's extractStruct
    fn extract_struct(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Symbol {
        let struct_name = self.extract_struct_name(node);
        let signature = self.build_struct_signature(node);

        self.create_symbol(
            &node,
            struct_name.clone(),
            SymbolKind::Class,
            Some(signature),
            "public",
            parent_id,
            Some(HashMap::from([
                ("type".to_string(), "struct".to_string()),
                ("name".to_string(), struct_name),
                ("fields".to_string(), format!("{} fields", self.extract_struct_fields(node).len())),
            ])),
        )
    }

    // Enum extraction - Miller's extractEnum
    fn extract_enum(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Symbol {
        let enum_name = self.extract_enum_name(node);
        let signature = self.build_enum_signature(node);

        self.create_symbol(
            &node,
            enum_name.clone(),
            SymbolKind::Enum,
            Some(signature),
            "public",
            parent_id,
            Some(HashMap::from([
                ("type".to_string(), "enum".to_string()),
                ("name".to_string(), enum_name),
                ("values".to_string(), format!("{} values", self.extract_enum_values(node).len())),
            ])),
        )
    }

    // Type definition extraction - Miller's extractTypeDefinition
    fn extract_type_definition(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Symbol {
        let typedef_name = self.extract_typedef_name_from_type_definition(node);
        let signature = self.base.get_node_text(&node);
        let underlying_type = self.extract_underlying_type_from_type_definition(node);

        // If the typedef contains any struct, treat it as a Class
        let symbol_kind = if self.contains_struct(node) { SymbolKind::Class } else { SymbolKind::Type };
        let struct_type = if symbol_kind == SymbolKind::Class { "struct" } else { "typedef" };
        let is_struct = symbol_kind == SymbolKind::Class;

        self.create_symbol(
            &node,
            typedef_name.clone(),
            symbol_kind,
            Some(signature),
            "public",
            parent_id,
            Some(HashMap::from([
                ("type".to_string(), struct_type.to_string()),
                ("name".to_string(), typedef_name),
                ("underlyingType".to_string(), underlying_type),
                ("isStruct".to_string(), is_struct.to_string()),
            ])),
        )
    }

    // Linkage specification extraction - Miller's extractLinkageSpecification
    fn extract_linkage_specification(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Find string literal for linkage type
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "string_literal" {
                let linkage_text = self.base.get_node_text(&child);
                if linkage_text.contains("\"C\"") {
                    let signature = format!("extern {}", linkage_text);
                    return Some(self.create_symbol(
                        &node,
                        "extern_c_block".to_string(),
                        SymbolKind::Namespace,
                        Some(signature),
                        "public",
                        parent_id,
                        Some(HashMap::from([
                            ("type".to_string(), "linkage_specification".to_string()),
                            ("linkage".to_string(), "C".to_string()),
                        ])),
                    ));
                }
            }
        }
        None
    }

    // Expression statement extraction - Miller's extractFromExpressionStatement
    fn extract_from_expression_statement(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Find identifier in expression statement
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let identifier_name = self.base.get_node_text(&child);

                // Check if this looks like a typedef name by looking at siblings
                if self.looks_like_typedef_name(&node, &identifier_name) {
                    let signature = self.build_typedef_signature(&node, &identifier_name);
                    return Some(self.create_symbol(
                        &node,
                        identifier_name.clone(),
                        SymbolKind::Class,
                        Some(signature),
                        "public",
                        parent_id,
                        Some(HashMap::from([
                            ("type".to_string(), "struct".to_string()),
                            ("name".to_string(), identifier_name),
                            ("fromExpressionStatement".to_string(), "true".to_string()),
                        ])),
                    ));
                }
            }
        }
        None
    }

    // Enum value symbols extraction - Miller's extractEnumValueSymbols
    fn extract_enum_value_symbols(&mut self, node: tree_sitter::Node, parent_enum_id: &str) -> Vec<Symbol> {
        let mut enum_value_symbols = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "enumerator_list" {
                let mut enum_cursor = child.walk();
                for enum_child in child.children(&mut enum_cursor) {
                    if enum_child.kind() == "enumerator" {
                        if let Some(name_node) = enum_child.child_by_field_name("name") {
                            let name = self.base.get_node_text(&name_node);
                            let value = enum_child.child_by_field_name("value")
                                .map(|v| self.base.get_node_text(&v));

                            let mut signature = name.clone();
                            if let Some(ref val) = value {
                                signature = format!("{} = {}", signature, val);
                            }

                            let enum_value_symbol = self.create_symbol(
                                &enum_child,
                                name.clone(),
                                SymbolKind::Constant,
                                Some(signature),
                                "public",
                                Some(parent_enum_id),
                                Some(HashMap::from([
                                    ("type".to_string(), "enum_value".to_string()),
                                    ("name".to_string(), name),
                                    ("value".to_string(), value.unwrap_or_default()),
                                    ("enumParent".to_string(), parent_enum_id.to_string()),
                                ])),
                            );

                            enum_value_symbols.push(enum_value_symbol);
                        }
                    }
                }
            }
        }

        enum_value_symbols
    }

    // Helper methods - Port of Miller's helper methods

    fn extract_include_path(&self, signature: &str) -> String {
        // Extract include path from #include statement
        if let Some(start) = signature.find('"') {
            if let Some(end) = signature.rfind('"') {
                if start < end {
                    return signature[start+1..end].to_string();
                }
            }
        }
        if let Some(start) = signature.find('<') {
            if let Some(end) = signature.rfind('>') {
                if start < end {
                    return signature[start+1..end].to_string();
                }
            }
        }
        "unknown".to_string()
    }

    fn is_system_header(&self, signature: &str) -> bool {
        signature.contains('<') && signature.contains('>')
    }

    fn extract_macro_name(&self, node: tree_sitter::Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                return self.base.get_node_text(&child);
            }
        }
        "unknown".to_string()
    }

    fn find_function_declarator<'a>(&self, node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "function_declarator" {
                return Some(child);
            }
            if child.kind() == "init_declarator" {
                let mut init_cursor = child.walk();
                for init_child in child.children(&mut init_cursor) {
                    if init_child.kind() == "function_declarator" {
                        return Some(init_child);
                    }
                }
            }
        }
        None
    }

    fn find_variable_declarators<'a>(&self, node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
        let mut declarators = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "init_declarator" | "declarator" | "identifier" | "array_declarator" => {
                    declarators.push(child);
                }
                _ => {}
            }
        }

        declarators
    }

    fn extract_function_name(&self, node: tree_sitter::Node) -> String {
        // Look for function declarator
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "function_declarator" {
                if let Some(identifier) = child.child_by_field_name("declarator") {
                    return self.base.get_node_text(&identifier);
                }
            }
            // For pointer return types, check pointer_declarator
            if child.kind() == "pointer_declarator" {
                let mut pointer_cursor = child.walk();
                for pointer_child in child.children(&mut pointer_cursor) {
                    if pointer_child.kind() == "function_declarator" {
                        if let Some(identifier) = pointer_child.child_by_field_name("declarator") {
                            return self.base.get_node_text(&identifier);
                        }
                    }
                }
            }
        }
        "unknown".to_string()
    }

    fn extract_function_name_from_declaration(&self, node: tree_sitter::Node) -> String {
        if let Some(function_declarator) = self.find_function_declarator(node) {
            if let Some(identifier) = function_declarator.child_by_field_name("declarator") {
                return self.base.get_node_text(&identifier);
            }
        }
        "unknown".to_string()
    }

    fn extract_variable_name(&self, declarator: tree_sitter::Node) -> String {
        if declarator.kind() == "identifier" {
            return self.base.get_node_text(&declarator);
        }

        // Find deepest identifier in declarator tree
        self.find_deepest_identifier(declarator)
            .map(|node| self.base.get_node_text(&node))
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn find_deepest_identifier<'a>(&self, node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        if node.kind() == "identifier" {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(result) = self.find_deepest_identifier(child) {
                return Some(result);
            }
        }

        None
    }

    // Signature building methods - Port of Miller's signature builders
    fn build_function_signature(&self, node: tree_sitter::Node) -> String {
        let storage_class = self.extract_storage_class(node);
        let return_type = self.extract_return_type(node);
        let function_name = self.extract_function_name(node);
        let parameters = self.extract_function_parameters(node);

        let storage_prefix = if let Some(sc) = storage_class {
            format!("{} ", sc)
        } else {
            String::new()
        };

        format!("{}{}{}({})", storage_prefix, return_type, function_name, parameters.join(", "))
    }

    fn build_function_declaration_signature(&self, node: tree_sitter::Node) -> String {
        let return_type = self.extract_return_type_from_declaration(node);
        let function_name = self.extract_function_name_from_declaration(node);
        let parameters = self.extract_function_parameters_from_declaration(node);

        format!("{}{}({})", return_type, function_name, parameters.join(", "))
    }

    fn build_variable_signature(&self, node: tree_sitter::Node, declarator: tree_sitter::Node) -> String {
        let storage_class = self.extract_storage_class(node);
        let type_qualifiers = self.extract_type_qualifiers(node);
        let data_type = self.extract_variable_type(node);
        let variable_name = self.extract_variable_name(declarator);
        let array_spec = self.extract_array_specifier(declarator);
        let initializer = self.extract_initializer(declarator);

        let mut signature = String::new();
        if let Some(sc) = storage_class {
            signature.push_str(&format!("{} ", sc));
        }
        if let Some(tq) = type_qualifiers {
            signature.push_str(&format!("{} ", tq));
        }
        signature.push_str(&format!("{} {}", data_type, variable_name));
        if let Some(arr) = array_spec {
            signature.push_str(&arr);
        }
        if let Some(init) = initializer {
            signature.push_str(&format!(" = {}", init));
        }

        signature
    }

    fn build_struct_signature(&self, node: tree_sitter::Node) -> String {
        let struct_name = self.extract_struct_name(node);
        let fields = self.extract_struct_fields(node);
        let attributes = self.extract_struct_attributes(node);

        let mut signature = format!("struct {}", struct_name);
        if !fields.is_empty() {
            let field_signatures: Vec<String> = fields.iter()
                .take(3)
                .map(|f| format!("{} {}", f.field_type, f.name))
                .collect();
            signature.push_str(&format!(" {{ {} }}", field_signatures.join("; ")));
        }

        if !attributes.is_empty() {
            signature.push_str(&format!(" {}", attributes.join(" ")));
        }

        signature
    }

    fn build_enum_signature(&self, node: tree_sitter::Node) -> String {
        let enum_name = self.extract_enum_name(node);
        let values = self.extract_enum_values(node);

        let mut signature = format!("enum {}", enum_name);
        if !values.is_empty() {
            let value_names: Vec<String> = values.iter()
                .take(3)
                .map(|v| v.name.clone())
                .collect();
            signature.push_str(&format!(" {{ {} }}", value_names.join(", ")));
        }

        signature
    }

    fn build_typedef_signature(&self, node: &tree_sitter::Node, identifier_name: &str) -> String {
        let node_text = self.base.get_node_text(node);

        // Look for typedef in previous siblings or parent context
        let attributes = if node_text.contains("PACKED") {
            vec!["PACKED"]
        } else {
            vec![]
        };

        let mut signature = format!("typedef struct {}", identifier_name);
        if !attributes.is_empty() {
            signature.push_str(&format!(" {}", attributes.join(" ")));
        }

        signature
    }

    // Type and attribute extraction methods
    fn extract_return_type(&self, node: tree_sitter::Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "primitive_type" | "type_identifier" | "sized_type_specifier" => {
                    return self.base.get_node_text(&child);
                }
                _ => {}
            }
        }
        "void".to_string()
    }

    fn extract_return_type_from_declaration(&self, node: tree_sitter::Node) -> String {
        self.extract_return_type(node)
    }

    fn extract_function_parameters(&self, node: tree_sitter::Node) -> Vec<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "function_declarator" {
                return self.extract_parameters_from_declarator(child);
            }
            if child.kind() == "pointer_declarator" {
                let mut pointer_cursor = child.walk();
                for pointer_child in child.children(&mut pointer_cursor) {
                    if pointer_child.kind() == "function_declarator" {
                        return self.extract_parameters_from_declarator(pointer_child);
                    }
                }
            }
        }
        Vec::new()
    }

    fn extract_function_parameters_from_declaration(&self, node: tree_sitter::Node) -> Vec<String> {
        if let Some(function_declarator) = self.find_function_declarator(node) {
            return self.extract_parameters_from_declarator(function_declarator);
        }
        Vec::new()
    }

    fn extract_parameters_from_declarator(&self, declarator: tree_sitter::Node) -> Vec<String> {
        let mut parameters = Vec::new();

        if let Some(param_list) = declarator.child_by_field_name("parameters") {
            let mut cursor = param_list.walk();
            for child in param_list.children(&mut cursor) {
                match child.kind() {
                    "parameter_declaration" => {
                        parameters.push(self.base.get_node_text(&child));
                    }
                    "variadic_parameter" => {
                        parameters.push("...".to_string());
                    }
                    _ => {}
                }
            }
        }

        parameters
    }

    fn extract_storage_class(&self, node: tree_sitter::Node) -> Option<String> {
        let mut storage_classes = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "storage_class_specifier" {
                storage_classes.push(self.base.get_node_text(&child));
            }
        }

        if storage_classes.is_empty() {
            None
        } else {
            Some(storage_classes.join(" "))
        }
    }

    fn extract_type_qualifiers(&self, node: tree_sitter::Node) -> Option<String> {
        let mut qualifiers = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "type_qualifier" {
                qualifiers.push(self.base.get_node_text(&child));
            }
        }

        if qualifiers.is_empty() {
            None
        } else {
            Some(qualifiers.join(" "))
        }
    }

    fn extract_variable_type(&self, node: tree_sitter::Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "primitive_type" | "type_identifier" | "sized_type_specifier" | "struct_specifier" | "enum_specifier" => {
                    return self.base.get_node_text(&child);
                }
                _ => {}
            }
        }
        "unknown".to_string()
    }

    fn extract_array_specifier(&self, declarator: tree_sitter::Node) -> Option<String> {
        if let Some(array_decl) = self.find_node_by_type(declarator, "array_declarator") {
            // Extract array size information
            let mut sizes = Vec::new();
            let mut cursor = array_decl.walk();
            let mut found_identifier = false;

            for child in array_decl.children(&mut cursor) {
                if child.kind() == "identifier" && !found_identifier {
                    found_identifier = true;
                    continue; // Skip the variable name
                }
                if child.kind() != "[" && child.kind() != "]" && found_identifier {
                    sizes.push(self.base.get_node_text(&child));
                }
            }

            if sizes.is_empty() {
                Some("[]".to_string())
            } else {
                Some(format!("[{}]", sizes.join(", ")))
            }
        } else {
            None
        }
    }

    fn extract_initializer(&self, declarator: tree_sitter::Node) -> Option<String> {
        if declarator.kind() == "init_declarator" {
            // Look for initializer after '='
            let mut found_equals = false;
            let mut cursor = declarator.walk();

            for child in declarator.children(&mut cursor) {
                if self.base.get_node_text(&child) == "=" {
                    found_equals = true;
                } else if found_equals {
                    return Some(self.base.get_node_text(&child));
                }
            }
        }
        None
    }

    fn extract_struct_name(&self, node: tree_sitter::Node) -> String {
        if let Some(name_node) = node.child_by_field_name("name") {
            self.base.get_node_text(&name_node)
        } else {
            "anonymous".to_string()
        }
    }

    fn extract_struct_fields(&self, node: tree_sitter::Node) -> Vec<StructField> {
        let mut fields = Vec::new();

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "field_declaration" {
                    let field_type = self.extract_variable_type(child);
                    let declarators = self.find_variable_declarators(child);

                    for declarator in declarators {
                        let field_name = self.extract_variable_name(declarator);
                        fields.push(StructField {
                            name: field_name,
                            field_type: field_type.clone(),
                        });
                    }
                }
            }
        }

        fields
    }

    fn extract_struct_attributes(&self, node: tree_sitter::Node) -> Vec<String> {
        let mut attributes = Vec::new();
        let node_text = self.base.get_node_text(&node);

        if node_text.contains("PACKED") {
            attributes.push("PACKED".to_string());
        }
        if node_text.contains("ALIGNED") {
            attributes.push("ALIGNED".to_string());
        }

        attributes
    }

    fn extract_enum_name(&self, node: tree_sitter::Node) -> String {
        if let Some(name_node) = node.child_by_field_name("name") {
            self.base.get_node_text(&name_node)
        } else {
            "anonymous".to_string()
        }
    }

    fn extract_enum_values(&self, node: tree_sitter::Node) -> Vec<EnumValue> {
        let mut values = Vec::new();

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "enumerator" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = self.base.get_node_text(&name_node);
                        let value = child.child_by_field_name("value")
                            .map(|v| self.base.get_node_text(&v));

                        values.push(EnumValue { name, value });
                    }
                }
            }
        }

        values
    }

    fn extract_typedef_name_from_type_definition(&self, node: tree_sitter::Node) -> String {
        // Look for type identifiers, returning the last one found (usually the typedef name)
        let mut type_identifiers = Vec::new();
        self.find_type_identifiers(node, &mut type_identifiers);

        if let Some(last_identifier) = type_identifiers.last() {
            last_identifier.clone()
        } else {
            "unknown".to_string()
        }
    }

    fn find_type_identifiers(&self, node: tree_sitter::Node, identifiers: &mut Vec<String>) {
        if node.kind() == "type_identifier" {
            let text = self.base.get_node_text(&node);
            // Skip known attributes
            if !["PACKED", "ALIGNED", "__packed__", "__aligned__"].contains(&text.as_str()) {
                identifiers.push(text);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.find_type_identifiers(child, identifiers);
        }
    }

    fn extract_underlying_type_from_type_definition(&self, node: tree_sitter::Node) -> String {
        // Find the underlying type (first non-typedef, non-semicolon, non-type_identifier child)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "typedef" | ";" | "type_identifier" => continue,
                _ => return self.base.get_node_text(&child),
            }
        }
        "unknown".to_string()
    }

    fn contains_struct(&self, node: tree_sitter::Node) -> bool {
        if node.kind() == "struct_specifier" {
            return true;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if self.contains_struct(child) {
                return true;
            }
        }

        false
    }

    fn looks_like_typedef_name(&self, node: &tree_sitter::Node, _identifier_name: &str) -> bool {
        // Simple heuristic: check if previous siblings contain "typedef"
        if let Some(parent) = node.parent() {
            let mut cursor = parent.walk();
            for child in parent.children(&mut cursor) {
                let child_text = self.base.get_node_text(&child);
                if child_text.contains("typedef") {
                    return true;
                }
            }
        }
        false
    }

    // Boolean property methods - Port of Miller's utility methods
    fn is_static_function(&self, node: tree_sitter::Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "storage_class_specifier" && self.base.get_node_text(&child) == "static" {
                return true;
            }
        }
        false
    }

    fn is_static_variable(&self, node: tree_sitter::Node) -> bool {
        self.is_static_function(node) // Same logic
    }

    fn is_extern_variable(&self, node: tree_sitter::Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "storage_class_specifier" && self.base.get_node_text(&child) == "extern" {
                return true;
            }
        }
        false
    }

    fn is_const_variable(&self, node: tree_sitter::Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_qualifier" && self.base.get_node_text(&child) == "const" {
                return true;
            }
        }
        false
    }

    fn is_volatile_variable(&self, node: tree_sitter::Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_qualifier" && self.base.get_node_text(&child) == "volatile" {
                return true;
            }
        }
        false
    }

    fn is_array_variable(&self, declarator: tree_sitter::Node) -> bool {
        self.find_node_by_type(declarator, "array_declarator").is_some()
    }

    fn find_node_by_type<'a>(&self, node: tree_sitter::Node<'a>, node_type: &str) -> Option<tree_sitter::Node<'a>> {
        if node.kind() == node_type {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(result) = self.find_node_by_type(child, node_type) {
                return Some(result);
            }
        }

        None
    }

    // Relationship extraction methods
    fn extract_relationships_from_node(&mut self, node: tree_sitter::Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        match node.kind() {
            "call_expression" => {
                self.extract_function_call_relationships(node, symbols, relationships);
            }
            "preproc_include" => {
                self.extract_include_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_relationships_from_node(child, symbols, relationships);
        }
    }

    fn extract_function_call_relationships(&mut self, node: tree_sitter::Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        if let Some(function_node) = node.child_by_field_name("function") {
            if function_node.kind() == "identifier" {
                let function_name = self.base.get_node_text(&function_node);
                if let Some(called_symbol) = symbols.iter().find(|s| s.name == function_name && s.kind == SymbolKind::Function) {
                    if let Some(containing_symbol) = self.find_containing_symbol(node, symbols) {
                        relationships.push(self.base.create_relationship(
                            &containing_symbol.id,
                            &called_symbol.id,
                            crate::extractors::base::RelationshipKind::Calls,
                            &node,
                        ));
                    }
                }
            }
        }
    }

    fn extract_include_relationships(&mut self, node: tree_sitter::Node, _symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        let include_path = self.extract_include_path(&self.base.get_node_text(&node));
        relationships.push(Relationship {
            from_symbol_id: format!("file:{}", self.base.file_path),
            to_symbol_id: format!("header:{}", include_path),
            kind: crate::extractors::base::RelationshipKind::Imports,
            file_path: self.base.file_path.clone(),
            line_number: (node.start_position().row + 1) as u32,
            confidence: 1.0,
            metadata: Some(HashMap::from([("includePath".to_string(), serde_json::Value::String(include_path))])),
        });
    }

    fn find_containing_symbol(&self, _node: tree_sitter::Node, _symbols: &[Symbol]) -> Option<&Symbol> {
        // Simplified implementation - would need more sophisticated logic to find the containing function
        None
    }

    // Helper for converting string metadata to serde_json::Value metadata
    fn create_metadata_map(&self, metadata: HashMap<String, String>) -> HashMap<String, Value> {
        metadata.into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect()
    }
}

// Helper structs for complex data
#[derive(Debug, Clone)]
struct StructField {
    name: String,
    field_type: String,
}

#[derive(Debug, Clone)]
struct EnumValue {
    name: String,
    value: Option<String>,
}