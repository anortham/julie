// Java Extractor Implementation
//
// Direct port of Miller's Java extractor logic to idiomatic Rust
// Original: /Users/murphy/Source/miller/src/extractors/java-extractor.ts
//
// Handles Java-specific constructs including:
// - Classes, interfaces, and enums
// - Methods, constructors, and fields
// - Packages and imports (regular, static, wildcard)
// - Generics and type parameters
// - Nested classes and annotations
// - Modern Java features (records, sealed classes, pattern matching)
// - Inheritance and implementation relationships

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, SymbolOptions};
use tree_sitter::{Tree, Node};
use std::collections::HashMap;
use serde_json;

pub struct JavaExtractor {
    base: BaseExtractor,
}

impl JavaExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree(tree.root_node(), &mut symbols, None);
        symbols
    }

    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        if let Some(symbol) = self.extract_symbol(node, parent_id) {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);

            // Walk children with this symbol as parent
            for child in node.children(&mut node.walk()) {
                self.walk_tree(child, symbols, Some(&symbol_id));
            }
        } else {
            // Walk children with the same parent
            for child in node.children(&mut node.walk()) {
                self.walk_tree(child, symbols, parent_id);
            }
        }
    }

    fn extract_symbol(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        match node.kind() {
            "package_declaration" => self.extract_package(node, parent_id),
            "import_declaration" => self.extract_import(node, parent_id),
            "class_declaration" => self.extract_class(node, parent_id),
            "interface_declaration" => self.extract_interface(node, parent_id),
            "method_declaration" => self.extract_method(node, parent_id),
            "constructor_declaration" => self.extract_constructor(node, parent_id),
            "field_declaration" => self.extract_field(node, parent_id),
            "enum_declaration" => self.extract_enum(node, parent_id),
            "enum_constant" => self.extract_enum_constant(node, parent_id),
            "annotation_type_declaration" => self.extract_annotation(node, parent_id),
            "record_declaration" => self.extract_record(node, parent_id),
            _ => None,
        }
    }

    fn extract_package(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let scoped_id = node.children(&mut node.walk())
            .find(|c| c.kind() == "scoped_identifier")?;

        let package_name = self.base.get_node_text(&scoped_id);
        let signature = format!("package {}", package_name);

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some("public".to_string()),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, package_name, SymbolKind::Namespace, options))
    }

    fn extract_import(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let scoped_id = node.children(&mut node.walk())
            .find(|c| c.kind() == "scoped_identifier")?;

        let mut full_import_path = self.base.get_node_text(&scoped_id);

        // Check if it's a static import
        let is_static = node.children(&mut node.walk())
            .any(|c| c.kind() == "static");

        // Check for wildcard imports (asterisk node)
        let has_asterisk = node.children(&mut node.walk())
            .any(|c| c.kind() == "asterisk");
        if has_asterisk {
            full_import_path.push_str(".*");
        }

        // Extract the class/member name (last part after the last dot)
        let parts: Vec<&str> = full_import_path.split('.').collect();
        let name = parts.last().unwrap_or(&"");

        // Handle wildcard imports
        let (symbol_name, signature) = if *name == "*" {
            let package_name = parts.get(parts.len().saturating_sub(2)).unwrap_or(&"");
            let sig = if is_static {
                format!("import static {}", full_import_path)
            } else {
                format!("import {}", full_import_path)
            };
            (package_name.to_string(), sig)
        } else {
            let sig = if is_static {
                format!("import static {}", full_import_path)
            } else {
                format!("import {}", full_import_path)
            };
            (name.to_string(), sig)
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some("public".to_string()),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, symbol_name, SymbolKind::Import, options))
    }

    fn extract_class(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(node);
        let visibility = self.determine_visibility(&modifiers);

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("class {}", name)
        } else {
            format!("{} class {}", modifiers.join(" "), name)
        };

        // Handle generic type parameters
        if let Some(type_params) = self.extract_type_parameters(node) {
            signature = signature.replace(&format!("class {}", name), &format!("class {}{}", name, type_params));
        }

        // Check for inheritance and implementations
        if let Some(superclass) = self.extract_superclass(node) {
            signature.push_str(&format!(" extends {}", superclass));
        }

        let interfaces = self.extract_implemented_interfaces(node);
        if !interfaces.is_empty() {
            signature.push_str(&format!(" implements {}", interfaces.join(", ")));
        }

        // Handle sealed class permits clause (Java 17+)
        if let Some(permits_clause) = node.children(&mut node.walk()).find(|c| c.kind() == "permits") {
            signature.push_str(&format!(" {}", self.base.get_node_text(&permits_clause)));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Class, options))
    }

    fn extract_interface(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(node);
        let visibility = self.determine_visibility(&modifiers);

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("interface {}", name)
        } else {
            format!("{} interface {}", modifiers.join(" "), name)
        };

        // Check for interface inheritance (extends)
        let super_interfaces = self.extract_extended_interfaces(node);
        if !super_interfaces.is_empty() {
            signature.push_str(&format!(" extends {}", super_interfaces.join(", ")));
        }

        // Handle generic type parameters
        if let Some(type_params) = self.extract_type_parameters(node) {
            signature = signature.replace(&format!("interface {}", name), &format!("interface {}{}", name, type_params));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Interface, options))
    }

    fn extract_method(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(node);
        let visibility = self.determine_visibility(&modifiers);

        // Get return type (comes before the method name in the AST)
        let children: Vec<Node> = node.children(&mut node.walk()).collect();
        let name_index = children.iter().position(|c| c.id() == name_node.id())?;

        let return_type_node = children[0..name_index].iter().find(|c| {
            matches!(c.kind(),
                "type_identifier" | "generic_type" | "void_type" | "array_type" |
                "primitive_type" | "integral_type" | "floating_point_type" | "boolean_type"
            )
        });
        let return_type = return_type_node
            .map(|n| self.base.get_node_text(n))
            .unwrap_or_else(|| "void".to_string());

        // Get parameters
        let param_list = node.children(&mut node.walk())
            .find(|c| c.kind() == "formal_parameters");
        let params = param_list
            .map(|p| self.base.get_node_text(&p))
            .unwrap_or_else(|| "()".to_string());

        // Handle generic type parameters on the method
        let type_params = self.extract_type_parameters(node);

        // Check for throws clause
        let throws_clause = self.extract_throws_clause(node);

        // Build signature
        let modifier_str = if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        };
        let type_param_str = type_params
            .map(|tp| format!("{} ", tp))
            .unwrap_or_default();
        let throws_str = throws_clause
            .map(|tc| format!(" {}", tc))
            .unwrap_or_default();

        let signature = format!("{}{}{} {}{}{}", modifier_str, type_param_str, return_type, name, params, throws_str);

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Method, options))
    }

    fn extract_constructor(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(node);
        let visibility = self.determine_visibility(&modifiers);

        // Get parameters
        let param_list = node.children(&mut node.walk())
            .find(|c| c.kind() == "formal_parameters");
        let params = param_list
            .map(|p| self.base.get_node_text(&p))
            .unwrap_or_else(|| "()".to_string());

        // Build signature (constructors don't have return types)
        let modifier_str = if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        };
        let signature = format!("{}{}{}", modifier_str, name, params);

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Constructor, options))
    }

    fn extract_field(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let modifiers = self.extract_modifiers(node);
        let visibility = self.determine_visibility(&modifiers);

        // Get type
        let type_node = node.children(&mut node.walk()).find(|c| {
            matches!(c.kind(),
                "type_identifier" | "generic_type" | "array_type" | "primitive_type" |
                "boolean_type" | "integral_type" | "floating_point_type" | "void_type"
            )
        });
        let field_type = type_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknown".to_string());

        // Get variable declarator(s) - there can be multiple fields in one declaration
        let declarators: Vec<Node> = node.children(&mut node.walk())
            .filter(|c| c.kind() == "variable_declarator")
            .collect();

        // For now, handle the first declarator (we could extend to handle multiple)
        let declarator = declarators.first()?;
        let name_node = declarator.children(&mut declarator.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);

        // Check if it's a constant (static final)
        let is_constant = modifiers.contains(&"static".to_string()) &&
                         modifiers.contains(&"final".to_string());
        let symbol_kind = if is_constant {
            SymbolKind::Constant
        } else {
            SymbolKind::Property
        };

        // Get initializer if present
        let children: Vec<Node> = declarator.children(&mut declarator.walk()).collect();
        let assign_index = children.iter().position(|c| c.kind() == "=");
        let initializer = if let Some(idx) = assign_index {
            let init_nodes: Vec<String> = children[(idx + 1)..]
                .iter()
                .map(|n| self.base.get_node_text(n))
                .collect();
            format!(" = {}", init_nodes.join(""))
        } else {
            String::new()
        };

        // Build signature
        let modifier_str = if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        };
        let signature = format!("{}{} {}{}", modifier_str, field_type, name, initializer);

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, symbol_kind, options))
    }

    fn extract_enum(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(node);
        let visibility = self.determine_visibility(&modifiers);

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("enum {}", name)
        } else {
            format!("{} enum {}", modifiers.join(" "), name)
        };

        // Check for interface implementations (enums can implement interfaces)
        let interfaces = self.extract_implemented_interfaces(node);
        if !interfaces.is_empty() {
            signature.push_str(&format!(" implements {}", interfaces.join(", ")));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Enum, options))
    }

    fn extract_enum_constant(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);

        // Build signature - include arguments if present
        let mut signature = name.clone();
        let argument_list = node.children(&mut node.walk())
            .find(|c| c.kind() == "argument_list");
        if let Some(args) = argument_list {
            signature.push_str(&self.base.get_node_text(&args));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some("public".to_string()), // Enum constants are always public in Java
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::EnumMember, options))
    }

    fn extract_annotation(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(node);
        let visibility = self.determine_visibility(&modifiers);

        // Build signature
        let signature = if modifiers.is_empty() {
            format!("@interface {}", name)
        } else {
            format!("{} @interface {}", modifiers.join(" "), name)
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Interface, options))
    }

    fn extract_record(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(node);
        let visibility = self.determine_visibility(&modifiers);

        // Get record parameters (record components)
        let param_list = node.children(&mut node.walk())
            .find(|c| c.kind() == "formal_parameters");
        let params = param_list
            .map(|p| self.base.get_node_text(&p))
            .unwrap_or_else(|| "()".to_string());

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("record {}{}", name, params)
        } else {
            format!("{} record {}{}", modifiers.join(" "), name, params)
        };

        // Handle generic type parameters
        if let Some(type_params) = self.extract_type_parameters(node) {
            signature = signature.replace(&format!("record {}", name), &format!("record {}{}", name, type_params));
        }

        // Check for interface implementations (records can implement interfaces)
        let interfaces = self.extract_implemented_interfaces(node);
        if !interfaces.is_empty() {
            signature.push_str(&format!(" implements {}", interfaces.join(", ")));
        }

        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), serde_json::Value::String("record".to_string()));

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Class, options))
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_node_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn visit_node_for_relationships(&mut self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        match node.kind() {
            "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration" => {
                self.extract_inheritance_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        for child in node.children(&mut node.walk()) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    fn extract_inheritance_relationships(&mut self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        let type_symbol = self.find_type_symbol(node, symbols);
        if type_symbol.is_none() {
            return;
        }
        let type_symbol = type_symbol.unwrap();

        // Handle class inheritance (extends)
        if let Some(superclass) = self.extract_superclass(node) {
            if let Some(base_type_symbol) = symbols.iter().find(|s| {
                s.name == superclass && matches!(s.kind, SymbolKind::Class | SymbolKind::Interface)
            }) {
                relationships.push(Relationship {
                    from_symbol_id: type_symbol.id.clone(),
                    to_symbol_id: base_type_symbol.id.clone(),
                    kind: RelationshipKind::Extends,
                    file_path: self.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: {
                        let mut map = HashMap::new();
                        map.insert("baseType".to_string(), serde_json::Value::String(superclass));
                        Some(map)
                    },
                });
            }
        }

        // Handle interface implementations
        let interfaces = self.extract_implemented_interfaces(node);
        for interface_name in interfaces {
            if let Some(interface_symbol) = symbols.iter().find(|s| {
                s.name == interface_name && s.kind == SymbolKind::Interface
            }) {
                relationships.push(Relationship {
                    from_symbol_id: type_symbol.id.clone(),
                    to_symbol_id: interface_symbol.id.clone(),
                    kind: RelationshipKind::Implements,
                    file_path: self.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: {
                        let mut map = HashMap::new();
                        map.insert("interface".to_string(), serde_json::Value::String(interface_name));
                        Some(map)
                    },
                });
            }
        }
    }

    fn find_type_symbol<'a>(&self, node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier")?;
        let type_name = self.base.get_node_text(&name_node);

        symbols.iter().find(|s| {
            s.name == type_name &&
            matches!(s.kind, SymbolKind::Class | SymbolKind::Interface | SymbolKind::Enum) &&
            s.file_path == self.base.file_path
        })
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            // Extract return type from method signatures
            if symbol.kind == SymbolKind::Method {
                if let Some(signature) = &symbol.signature {
                    // Regex pattern to match return type in method signature
                    if let Some(captures) = regex::Regex::new(r"(\w+[\w<>\[\], ]*)\s+\w+\s*\(")
                        .ok()
                        .and_then(|re| re.captures(signature))
                    {
                        if let Some(return_type_match) = captures.get(1) {
                            let return_type = return_type_match.as_str().trim();
                            // Clean up modifiers from return type
                            let clean_return_type = regex::Regex::new(r"^(public|private|protected|static|final|abstract|synchronized|native|strictfp)\s+")
                                .ok()
                                .map(|re| re.replace(return_type, "").to_string())
                                .unwrap_or_else(|| return_type.to_string());
                            types.insert(symbol.id.clone(), clean_return_type);
                        }
                    }
                }
            }

            // Extract field types from field signatures
            if symbol.kind == SymbolKind::Property {
                if let Some(signature) = &symbol.signature {
                    // Regex pattern to match field type
                    if let Some(captures) = regex::Regex::new(r"(\w+[\w<>\[\], ]*)\s+\w+")
                        .ok()
                        .and_then(|re| re.captures(signature))
                    {
                        if let Some(field_type_match) = captures.get(1) {
                            let field_type = field_type_match.as_str().trim();
                            // Clean up modifiers from field type
                            let clean_field_type = regex::Regex::new(r"^(public|private|protected|static|final|volatile|transient)\s+")
                                .ok()
                                .map(|re| re.replace(field_type, "").to_string())
                                .unwrap_or_else(|| field_type.to_string());
                            types.insert(symbol.id.clone(), clean_field_type);
                        }
                    }
                }
            }
        }

        types
    }

    // Helper methods for Java-specific parsing
    fn extract_modifiers(&self, node: Node) -> Vec<String> {
        node.children(&mut node.walk())
            .find(|c| c.kind() == "modifiers")
            .map(|modifiers_node| {
                modifiers_node.children(&mut modifiers_node.walk())
                    .map(|c| self.base.get_node_text(&c))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn determine_visibility(&self, modifiers: &[String]) -> String {
        if modifiers.contains(&"public".to_string()) {
            "public".to_string()
        } else if modifiers.contains(&"private".to_string()) {
            "private".to_string()
        } else if modifiers.contains(&"protected".to_string()) {
            "protected".to_string()
        } else {
            "package".to_string() // Default visibility in Java
        }
    }

    fn extract_superclass(&self, node: Node) -> Option<String> {
        let superclass_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "superclass")?;

        let type_node = superclass_node.children(&mut superclass_node.walk())
            .find(|c| matches!(c.kind(), "type_identifier" | "generic_type"))?;

        Some(self.base.get_node_text(&type_node))
    }

    fn extract_implemented_interfaces(&self, node: Node) -> Vec<String> {
        let interfaces_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "super_interfaces");

        if let Some(interfaces_node) = interfaces_node {
            if let Some(type_list_node) = interfaces_node.children(&mut interfaces_node.walk())
                .find(|c| c.kind() == "type_list") {
                return type_list_node.children(&mut type_list_node.walk())
                    .filter(|c| matches!(c.kind(), "type_identifier" | "generic_type"))
                    .map(|c| self.base.get_node_text(&c))
                    .collect();
            }
        }

        Vec::new()
    }

    fn extract_extended_interfaces(&self, node: Node) -> Vec<String> {
        let extends_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "extends_interfaces");

        if let Some(extends_node) = extends_node {
            if let Some(type_list_node) = extends_node.children(&mut extends_node.walk())
                .find(|c| c.kind() == "type_list") {
                return type_list_node.children(&mut type_list_node.walk())
                    .filter(|c| matches!(c.kind(), "type_identifier" | "generic_type"))
                    .map(|c| self.base.get_node_text(&c))
                    .collect();
            }
        }

        Vec::new()
    }

    fn extract_type_parameters(&self, node: Node) -> Option<String> {
        let type_params_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_parameters")?;

        Some(self.base.get_node_text(&type_params_node))
    }

    fn extract_throws_clause(&self, node: Node) -> Option<String> {
        let throws_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "throws")?;

        // The throws node contains the entire clause including the 'throws' keyword
        Some(self.base.get_node_text(&throws_node))
    }
}