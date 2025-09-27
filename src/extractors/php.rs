// PHP Extractor for Julie - Direct port from Miller's php-extractor.ts
//
// This is the RED phase - minimal implementation to make tests compile but fail
// Will be fully implemented in GREEN phase following Miller's exact logic

use crate::extractors::base::{
    BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct PhpExtractor {
    base: BaseExtractor,
}

impl PhpExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    /// Extract symbols from PHP code - Miller's main extraction method
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Extract relationships from PHP code - Miller's relationship extraction
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    /// Infer types from PHP type declarations - Miller's type inference
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            let metadata = &symbol.metadata;
            if let Some(return_type) = metadata.as_ref().and_then(|m| m.get("returnType")) {
                if let Some(type_str) = return_type.as_str() {
                    types.insert(symbol.id.clone(), type_str.to_string());
                }
            } else if let Some(property_type) =
                metadata.as_ref().and_then(|m| m.get("propertyType"))
            {
                if let Some(type_str) = property_type.as_str() {
                    types.insert(symbol.id.clone(), type_str.to_string());
                }
            } else if let Some(type_val) = metadata.as_ref().and_then(|m| m.get("type")) {
                if let Some(type_str) = type_val.as_str() {
                    if !matches!(type_str, "function" | "property") {
                        types.insert(symbol.id.clone(), type_str.to_string());
                    }
                }
            }
        }
        types
    }

    /// Recursive node visitor following Miller's visitNode pattern
    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if node.kind().is_empty() {
            return; // Skip invalid nodes
        }

        let mut current_parent_id = parent_id.clone();
        let symbol = match node.kind() {
            "class_declaration" => Some(self.extract_class(node, parent_id.as_deref())),
            "interface_declaration" => Some(self.extract_interface(node, parent_id.as_deref())),
            "trait_declaration" => Some(self.extract_trait(node, parent_id.as_deref())),
            "enum_declaration" => Some(self.extract_enum(node, parent_id.as_deref())),
            "function_definition" | "method_declaration" => {
                Some(self.extract_function(node, parent_id.as_deref()))
            }
            "property_declaration" => self.extract_property(node, parent_id.as_deref()),
            "const_declaration" => self.extract_constant(node, parent_id.as_deref()),
            "namespace_definition" => Some(self.extract_namespace(node, parent_id.as_deref())),
            "use_declaration" | "namespace_use_declaration" => {
                Some(self.extract_use(node, parent_id.as_deref()))
            }
            "enum_case" => self.extract_enum_case(node, parent_id.as_deref()),
            "assignment_expression" => self.extract_variable_assignment(node, parent_id.as_deref()),
            _ => None,
        };

        if let Some(sym) = symbol {
            current_parent_id = Some(sym.id.clone());
            symbols.push(sym);
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract PHP class declarations following Miller's logic
    fn extract_class(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = self
            .find_child_text(&node, "name")
            .unwrap_or_else(|| "UnknownClass".to_string());

        let modifiers = self.extract_modifiers(&node);
        let extends_node = self.find_child(&node, "base_clause");
        let implements_node = self.find_child(&node, "class_interface_clause");
        let attribute_list = self.find_child(&node, "attribute_list");

        let mut signature = String::new();

        // Add attributes if present
        if let Some(attr_node) = attribute_list {
            signature.push_str(&self.base.get_node_text(&attr_node));
            signature.push('\n');
        }

        signature.push_str(&format!("class {}", name));

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(extends_node) = extends_node {
            let base_class = self
                .base
                .get_node_text(&extends_node)
                .replace("extends", "")
                .trim()
                .to_string();
            signature.push_str(&format!(" extends {}", base_class));
        }

        if let Some(implements_node) = implements_node {
            let interfaces = self
                .base
                .get_node_text(&implements_node)
                .replace("implements", "")
                .trim()
                .to_string();
            signature.push_str(&format!(" implements {}", interfaces));
        }

        // Add trait usages
        if let Some(declaration_list) = self.find_child(&node, "declaration_list") {
            let mut cursor = declaration_list.walk();
            for child in declaration_list.children(&mut cursor) {
                if child.kind() == "use_declaration" {
                    let trait_usage = self.base.get_node_text(&child);
                    signature.push_str(&format!(" {}", trait_usage));
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("class".to_string()),
        );
        metadata.insert(
            "modifiers".to_string(),
            serde_json::Value::Array(
                modifiers
                    .iter()
                    .map(|m| serde_json::Value::String(m.clone()))
                    .collect(),
            ),
        );

        if let Some(extends_node) = extends_node {
            metadata.insert(
                "extends".to_string(),
                serde_json::Value::String(self.base.get_node_text(&extends_node)),
            );
        }

        if let Some(implements_node) = implements_node {
            metadata.insert(
                "implements".to_string(),
                serde_json::Value::String(self.base.get_node_text(&implements_node)),
            );
        }

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract PHP interface declarations
    fn extract_interface(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = self
            .find_child_text(&node, "name")
            .unwrap_or_else(|| "UnknownInterface".to_string());

        let extends_node = self.find_child(&node, "base_clause");
        let mut signature = format!("interface {}", name);

        if let Some(extends_node) = extends_node {
            let base_interfaces = self
                .base
                .get_node_text(&extends_node)
                .replace("extends", "")
                .trim()
                .to_string();
            signature.push_str(&format!(" extends {}", base_interfaces));
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("interface".to_string()),
        );
        if let Some(extends_node) = extends_node {
            metadata.insert(
                "extends".to_string(),
                serde_json::Value::String(self.base.get_node_text(&extends_node)),
            );
        }

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Interface,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract PHP trait declarations
    fn extract_trait(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = self
            .find_child_text(&node, "name")
            .unwrap_or_else(|| "UnknownTrait".to_string());

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("trait".to_string()),
        );

        self.base.create_symbol(
            &node,
            name.clone(),
            SymbolKind::Trait,
            SymbolOptions {
                signature: Some(format!("trait {}", name)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Helper method to find child node by type
    fn find_child<'a>(&self, node: &Node<'a>, child_type: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == child_type {
                return Some(child);
            }
        }
        None
    }

    /// Helper method to find child node text by type
    fn find_child_text(&self, node: &Node, child_type: &str) -> Option<String> {
        self.find_child(node, child_type)
            .map(|child| self.base.get_node_text(&child))
    }

    /// Extract modifiers from PHP nodes
    fn extract_modifiers(&self, node: &Node) -> Vec<String> {
        let mut modifiers = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "visibility_modifier" => modifiers.push(self.base.get_node_text(&child)),
                "abstract_modifier" => modifiers.push("abstract".to_string()),
                "static_modifier" => modifiers.push("static".to_string()),
                "final_modifier" => modifiers.push("final".to_string()),
                "readonly_modifier" => modifiers.push("readonly".to_string()),
                "public" | "private" | "protected" | "static" | "abstract" | "final"
                | "readonly" => {
                    modifiers.push(self.base.get_node_text(&child));
                }
                _ => {}
            }
        }

        modifiers
    }

    /// Determine visibility from modifiers
    fn determine_visibility(&self, modifiers: &[String]) -> Visibility {
        for modifier in modifiers {
            match modifier.as_str() {
                "private" => return Visibility::Private,
                "protected" => return Visibility::Protected,
                _ => {}
            }
        }
        Visibility::Public // PHP defaults to public
    }

    /// Extract PHP function/method declarations
    fn extract_function(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = self
            .find_child_text(&node, "name")
            .unwrap_or_else(|| "unknownFunction".to_string());

        let modifiers = self.extract_modifiers(&node);
        let parameters_node = self.find_child(&node, "formal_parameters");
        let attribute_list = self.find_child(&node, "attribute_list");

        // PHP return type comes after : as primitive_type, named_type, union_type, or optional_type
        let return_type_node = self.find_return_type(&node);

        // Check for reference modifier (&)
        let reference_modifier = self.find_child(&node, "reference_modifier");
        let ref_prefix = if reference_modifier.is_some() {
            "&"
        } else {
            ""
        };

        // Determine symbol kind
        let symbol_kind = match name.as_str() {
            "__construct" => SymbolKind::Constructor,
            "__destruct" => SymbolKind::Destructor,
            _ => SymbolKind::Function,
        };

        let mut signature = String::new();

        // Add attributes if present
        if let Some(attr_node) = attribute_list {
            signature.push_str(&self.base.get_node_text(&attr_node));
            signature.push('\n');
        }

        signature.push_str(&format!("function {}{}", ref_prefix, name));

        if !modifiers.is_empty() {
            signature = signature.replace(
                &format!("function {}{}", ref_prefix, name),
                &format!("{} function {}{}", modifiers.join(" "), ref_prefix, name),
            );
        }

        if let Some(params_node) = parameters_node {
            signature.push_str(&self.base.get_node_text(&params_node));
        } else {
            signature.push_str("()");
        }

        if let Some(return_node) = return_type_node {
            signature.push_str(&format!(": {}", self.base.get_node_text(&return_node)));
        }

        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "function".to_string());
        metadata.insert("modifiers".to_string(), modifiers.join(","));

        if let Some(params_node) = parameters_node {
            metadata.insert(
                "parameters".to_string(),
                self.base.get_node_text(&params_node),
            );
        } else {
            metadata.insert("parameters".to_string(), "()".to_string());
        }

        if let Some(return_node) = return_type_node {
            metadata.insert(
                "returnType".to_string(),
                self.base.get_node_text(&return_node),
            );
        }

        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(
                    metadata
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                        .collect(),
                ),
                doc_comment: None,
            },
        )
    }

    /// Find return type node after colon
    fn find_return_type<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let mut found_colon = false;

        for child in node.children(&mut cursor) {
            if found_colon {
                match child.kind() {
                    "primitive_type" | "named_type" | "union_type" | "optional_type" => {
                        return Some(child)
                    }
                    _ => {}
                }
            }
            if child.kind() == ":" {
                found_colon = true;
            }
        }
        None
    }

    /// Extract PHP property declarations
    fn extract_property(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract property name from property_element
        let property_element = self.find_child(&node, "property_element")?;
        let name_node = self.find_child(&property_element, "variable_name")?;
        let name = self.base.get_node_text(&name_node);

        let modifiers = self.extract_modifiers(&node);
        let type_node = self.find_type_node(&node);
        let attribute_list = self.find_child(&node, "attribute_list");

        // Check for default value assignment
        let property_value = self.extract_property_value(&property_element);

        // Build signature in correct order: attributes + modifiers + type + name + value
        let mut signature = String::new();

        // Add attributes if present
        if let Some(attr_node) = attribute_list {
            signature.push_str(&self.base.get_node_text(&attr_node));
            signature.push('\n');
        }

        if !modifiers.is_empty() {
            signature.push_str(&format!("{} ", modifiers.join(" ")));
        }

        if let Some(type_node) = type_node {
            signature.push_str(&format!("{} ", self.base.get_node_text(&type_node)));
        }

        signature.push_str(&name);

        if let Some(value) = property_value {
            signature.push_str(&format!(" = {}", value));
        }

        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "property".to_string());
        metadata.insert("modifiers".to_string(), modifiers.join(","));

        if let Some(type_node) = type_node {
            metadata.insert(
                "propertyType".to_string(),
                self.base.get_node_text(&type_node),
            );
        }

        Some(
            self.base.create_symbol(
                &node,
                name.replace('$', ""), // Remove $ from property name
                SymbolKind::Property,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(self.determine_visibility(&modifiers)),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some(
                        metadata
                            .into_iter()
                            .map(|(k, v)| (k, serde_json::Value::String(v)))
                            .collect(),
                    ),
                    doc_comment: None,
                },
            ),
        )
    }

    /// Find type node in property declaration
    fn find_type_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "type" | "primitive_type" | "optional_type" | "named_type" => return Some(child),
                _ => {}
            }
        }
        None
    }

    /// Extract property default value
    fn extract_property_value(&self, property_element: &Node) -> Option<String> {
        let mut cursor = property_element.walk();
        let mut found_assignment = false;

        for child in property_element.children(&mut cursor) {
            if found_assignment {
                return Some(self.base.get_node_text(&child));
            }
            if child.kind() == "=" {
                found_assignment = true;
            }
        }
        None
    }

    /// Extract PHP constant declarations
    fn extract_constant(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // First pass: extract all text values we need before any borrowing operations
        let const_element = self.find_child(&node, "const_element")?;
        let name_node = self.find_child(&const_element, "name")?;

        // Extract all text values immediately
        let name = {
            let text = self.base.get_node_text(&name_node);
            text
        };

        // Extract value text immediately
        let value = {
            let mut cursor = const_element.walk();
            let mut found_assignment = false;
            let mut val = None;

            for child in const_element.children(&mut cursor) {
                if found_assignment {
                    val = Some(self.base.get_node_text(&child));
                    break;
                }
                if child.kind() == "=" {
                    found_assignment = true;
                }
            }
            val
        };

        // Extract modifiers and visibility immediately
        let visibility = {
            let modifiers = self.extract_modifiers(&node);
            self.determine_visibility(&modifiers)
        };

        // Now all borrows are complete - build the symbol
        let mut signature = format!(
            "{} const {}",
            match visibility {
                Visibility::Public => "public",
                Visibility::Private => "private",
                Visibility::Protected => "protected",
            },
            name
        );

        if let Some(val) = &value {
            signature.push_str(&format!(" = {}", val));
        }

        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "constant".to_string());
        if let Some(val) = value {
            metadata.insert("value".to_string(), val);
        }

        Some(
            self.base.create_symbol(
                &node,
                name,
                SymbolKind::Constant,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some(
                        metadata
                            .into_iter()
                            .map(|(k, v)| (k, serde_json::Value::String(v)))
                            .collect(),
                    ),
                    doc_comment: None,
                },
            ),
        )
    }

    /// Extract PHP namespace declarations
    fn extract_namespace(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = self
            .find_child_text(&node, "namespace_name")
            .unwrap_or_else(|| "UnknownNamespace".to_string());

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("namespace".to_string()),
        );

        self.base.create_symbol(
            &node,
            name.clone(),
            SymbolKind::Namespace,
            SymbolOptions {
                signature: Some(format!("namespace {}", name)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract PHP use/import declarations
    fn extract_use(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let (name, alias) = match node.kind() {
            "namespace_use_declaration" => {
                // Handle new namespace_use_declaration format
                if let Some(use_clause) = self.find_child(&node, "namespace_use_clause") {
                    if let Some(qualified_name) = self.find_child(&use_clause, "qualified_name") {
                        let name = self.base.get_node_text(&qualified_name);
                        let alias = self
                            .find_child(&node, "namespace_aliasing_clause")
                            .map(|alias_node| self.base.get_node_text(&alias_node));
                        (name, alias)
                    } else {
                        ("UnknownImport".to_string(), None)
                    }
                } else {
                    ("UnknownImport".to_string(), None)
                }
            }
            _ => {
                // Handle legacy use_declaration format
                let name = self
                    .find_child_text(&node, "namespace_name")
                    .or_else(|| self.find_child_text(&node, "qualified_name"))
                    .unwrap_or_else(|| "UnknownImport".to_string());
                let alias = self
                    .find_child(&node, "namespace_aliasing_clause")
                    .map(|alias_node| self.base.get_node_text(&alias_node));
                (name, alias)
            }
        };

        let mut signature = format!("use {}", name);
        if let Some(alias_text) = &alias {
            signature.push_str(&format!(" {}", alias_text));
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("use".to_string()),
        );
        if let Some(alias_text) = alias {
            metadata.insert("alias".to_string(), serde_json::Value::String(alias_text));
        }

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract PHP enum declarations
    fn extract_enum(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = self
            .find_child_text(&node, "name")
            .unwrap_or_else(|| "UnknownEnum".to_string());

        // Check for backing type (e.g., enum Status: string)
        let backing_type = self.find_backing_type(&node);

        // Check for implements clause (e.g., implements JsonSerializable)
        let implements_node = self.find_child(&node, "class_interface_clause");

        let mut signature = format!("enum {}", name);
        if let Some(backing_type) = &backing_type {
            signature.push_str(&format!(": {}", backing_type));
        }
        if let Some(implements_node) = implements_node {
            let implements_clause = self
                .base
                .get_node_text(&implements_node)
                .replace("implements", "")
                .trim()
                .to_string();
            signature.push_str(&format!(" implements {}", implements_clause));
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("enum".to_string()),
        );
        if let Some(backing_type) = backing_type {
            metadata.insert(
                "backingType".to_string(),
                serde_json::Value::String(backing_type),
            );
        }

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Enum,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Find backing type after colon in enum declaration
    fn find_backing_type(&self, node: &Node) -> Option<String> {
        let mut cursor = node.walk();
        let mut found_colon = false;

        for child in node.children(&mut cursor) {
            if found_colon && child.kind() == "primitive_type" {
                return Some(self.base.get_node_text(&child));
            }
            if child.kind() == ":" {
                found_colon = true;
            }
        }
        None
    }

    /// Extract PHP enum cases
    fn extract_enum_case(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name_node = self.find_child(&node, "name")?;
        let case_name = self.base.get_node_text(&name_node);

        // Check for value assignment (e.g., case PENDING = 'pending')
        let mut value = None;
        let mut cursor = node.walk();
        let mut found_assignment = false;

        for child in node.children(&mut cursor) {
            if found_assignment {
                match child.kind() {
                    "string" | "integer" => {
                        value = Some(self.base.get_node_text(&child));
                        break;
                    }
                    _ => {}
                }
            }
            if child.kind() == "=" {
                found_assignment = true;
            }
        }

        let mut signature = format!("case {}", case_name);
        if let Some(val) = &value {
            signature.push_str(&format!(" = {}", val));
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("enum_case".to_string()),
        );
        if let Some(val) = value {
            metadata.insert("value".to_string(), serde_json::Value::String(val));
        }

        Some(self.base.create_symbol(
            &node,
            case_name,
            SymbolKind::EnumMember,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    /// Extract variable assignments
    fn extract_variable_assignment(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Find variable name (left side of assignment)
        let variable_name_node = self.find_child(&node, "variable_name")?;
        let name_node = self.find_child(&variable_name_node, "name")?;
        let var_name = self.base.get_node_text(&name_node);

        // Find assignment value (right side of assignment)
        let mut value_text = String::new();
        let mut cursor = node.walk();
        let mut found_assignment = false;

        for child in node.children(&mut cursor) {
            if found_assignment {
                value_text = self.base.get_node_text(&child);
                break;
            }
            if child.kind() == "=" {
                found_assignment = true;
            }
        }

        let signature = format!(
            "{} = {}",
            self.base.get_node_text(&variable_name_node),
            value_text
        );

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("variable_assignment".to_string()),
        );
        metadata.insert("value".to_string(), serde_json::Value::String(value_text));

        Some(self.base.create_symbol(
            &node,
            var_name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    /// Visit nodes for relationship extraction
    fn visit_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_declaration" => {
                self.extract_class_relationships(node, symbols, relationships);
            }
            "interface_declaration" => {
                self.extract_interface_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_relationships(child, symbols, relationships);
        }
    }

    /// Extract class inheritance and implementation relationships
    fn extract_class_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let class_symbol = self.find_class_symbol(node, symbols);
        if class_symbol.is_none() {
            return;
        }
        let class_symbol = class_symbol.unwrap();

        // Inheritance relationships
        if let Some(extends_node) = self.find_child(&node, "base_clause") {
            let base_class_name = self
                .base
                .get_node_text(&extends_node)
                .replace("extends", "")
                .trim()
                .to_string();
            // Find the actual symbol for the base class
            if let Some(base_class_symbol) = symbols
                .iter()
                .find(|s| s.name == base_class_name && s.kind == SymbolKind::Class)
            {
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol.id,
                        base_class_symbol.id,
                        RelationshipKind::Extends,
                        node.start_position().row
                    ),
                    from_symbol_id: class_symbol.id.clone(),
                    to_symbol_id: base_class_symbol.id.clone(),
                    kind: RelationshipKind::Extends,
                    file_path: self.base.file_path.clone(),
                    line_number: node.start_position().row as u32 + 1,
                    confidence: 1.0,
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "baseClass".to_string(),
                            serde_json::Value::String(base_class_name),
                        );
                        metadata
                    }),
                });
            }
        }

        // Implementation relationships
        if let Some(implements_node) = self.find_child(&node, "class_interface_clause") {
            let interface_names: Vec<String> = self
                .base
                .get_node_text(&implements_node)
                .replace("implements", "")
                .split(',')
                .map(|name| name.trim().to_string())
                .collect();

            for interface_name in interface_names {
                // Find the actual interface symbol
                let interface_symbol = symbols.iter().find(|s| {
                    s.name == interface_name
                        && s.kind == SymbolKind::Interface
                        && s.file_path == self.base.file_path
                });

                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol.id,
                        interface_symbol
                            .map(|s| s.id.clone())
                            .unwrap_or_else(|| format!("php-interface:{}", interface_name)),
                        RelationshipKind::Implements,
                        node.start_position().row
                    ),
                    from_symbol_id: class_symbol.id.clone(),
                    to_symbol_id: interface_symbol
                        .map(|s| s.id.clone())
                        .unwrap_or_else(|| format!("php-interface:{}", interface_name)),
                    kind: RelationshipKind::Implements,
                    file_path: self.base.file_path.clone(),
                    line_number: node.start_position().row as u32 + 1,
                    confidence: if interface_symbol.is_some() { 1.0 } else { 0.8 },
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "interface".to_string(),
                            serde_json::Value::String(interface_name),
                        );
                        metadata
                    }),
                });
            }
        }
    }

    /// Extract interface inheritance relationships
    fn extract_interface_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let interface_symbol = self.find_interface_symbol(node, symbols);
        if interface_symbol.is_none() {
            return;
        }
        let interface_symbol = interface_symbol.unwrap();

        // Interface inheritance
        if let Some(extends_node) = self.find_child(&node, "base_clause") {
            let base_interface_names: Vec<String> = self
                .base
                .get_node_text(&extends_node)
                .replace("extends", "")
                .split(',')
                .map(|name| name.trim().to_string())
                .collect();

            for base_interface_name in base_interface_names {
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        interface_symbol.id,
                        format!("php-interface:{}", base_interface_name),
                        RelationshipKind::Extends,
                        node.start_position().row
                    ),
                    from_symbol_id: interface_symbol.id.clone(),
                    to_symbol_id: format!("php-interface:{}", base_interface_name),
                    kind: RelationshipKind::Extends,
                    file_path: self.base.file_path.clone(),
                    line_number: node.start_position().row as u32 + 1,
                    confidence: 1.0,
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "baseInterface".to_string(),
                            serde_json::Value::String(base_interface_name),
                        );
                        metadata
                    }),
                });
            }
        }
    }

    /// Find class symbol by node
    fn find_class_symbol<'a>(&self, node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
        let name = self.find_child_text(&node, "name")?;
        symbols.iter().find(|s| {
            s.name == name && s.kind == SymbolKind::Class && s.file_path == self.base.file_path
        })
    }

    /// Find interface symbol by node
    fn find_interface_symbol<'a>(&self, node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
        let name = self.find_child_text(&node, "name")?;
        symbols.iter().find(|s| {
            s.name == name && s.kind == SymbolKind::Interface && s.file_path == self.base.file_path
        })
    }
}
