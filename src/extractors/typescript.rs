// Minimal TypeScript Extractor (TDD GREEN phase implementation)
//
// This is the minimal implementation to make tests compile but fail initially (RED phase)

use crate::extractors::base::{
    BaseExtractor, Identifier, IdentifierKind, Relationship, Symbol, SymbolKind,
};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct TypeScriptExtractor {
    base: BaseExtractor,
}

impl TypeScriptExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        // Minimal implementation - just extract function declarations to pass first test
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols);
        symbols
    }

    fn visit_node(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>) {
        let mut symbol: Option<Symbol> = None;

        // Port Miller's switch statement logic
        match node.kind() {
            "class_declaration" => {
                symbol = Some(self.extract_class(node));
            }
            "interface_declaration" => {
                symbol = Some(self.extract_interface(node));
            }
            "function_declaration" | "function" => {
                symbol = Some(self.extract_function(node));
            }
            "method_definition" | "method_signature" => {
                symbol = Some(self.extract_method(node));
            }
            "variable_declarator" => {
                symbol = Some(self.extract_variable(node));
            }
            "type_alias_declaration" => {
                symbol = Some(self.extract_type_alias(node));
            }
            "enum_declaration" => {
                symbol = Some(self.extract_enum(node));
            }
            "import_statement" | "import_declaration" => {
                symbol = Some(self.extract_import(node));
            }
            "export_statement" => {
                symbol = Some(self.extract_export(node));
            }
            "namespace_declaration" | "module_declaration" => {
                symbol = Some(self.extract_namespace(node));
            }
            "property_signature" | "public_field_definition" | "property_definition" => {
                symbol = Some(self.extract_property(node));
            }
            _ => {}
        }

        if let Some(sym) = symbol {
            symbols.push(sym);
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols);
        }
    }

    #[allow(dead_code)]
    fn create_minimal_symbol(
        &mut self,
        node: &tree_sitter::Node,
        name: String,
        kind: SymbolKind,
        signature: Option<String>,
    ) -> Symbol {
        use crate::extractors::base::SymbolOptions;

        self.base.create_symbol(
            node,
            name,
            kind,
            SymbolOptions {
                signature,
                visibility: None,
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    // Port of Miller's extractFunction method
    fn extract_function(&mut self, node: tree_sitter::Node) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let mut name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        // Handle arrow functions assigned to variables (Miller logic)
        if node.kind() == "arrow_function" {
            if let Some(parent) = node.parent() {
                if parent.kind() == "variable_declarator" {
                    if let Some(var_name_node) = parent.child_by_field_name("name") {
                        name = self.base.get_node_text(&var_name_node);
                    }
                }
            }
        }

        let signature = self.build_function_signature(&node, &name);
        let visibility = self.base.extract_visibility(&node);

        // Check for modifiers (Miller logic)
        let is_async = node
            .children(&mut node.walk())
            .any(|child| child.kind() == "async");
        let is_generator = node
            .children(&mut node.walk())
            .any(|child| child.kind() == "*");

        let parameters = self.extract_parameters(&node);
        let return_type = self.base.get_field_text(&node, "return_type");
        let type_parameters = self.extract_type_parameters(&node);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("isAsync".to_string(), serde_json::json!(is_async));
        metadata.insert("isGenerator".to_string(), serde_json::json!(is_generator));
        metadata.insert("parameters".to_string(), serde_json::json!(parameters));
        if let Some(return_type) = return_type {
            metadata.insert("returnType".to_string(), serde_json::json!(return_type));
        }
        metadata.insert(
            "typeParameters".to_string(),
            serde_json::json!(type_parameters),
        );

        use crate::extractors::base::SymbolOptions;

        // CRITICAL FIX: Symbol must span entire function body for containment logic,
        // but ID should be generated from function name position (not body start).
        // Previous bug: adjust_symbol_span() overrode span to just the name, breaking containment.

        // First create symbol with full function body span
        let mut symbol = self.base.create_symbol(
            &node,
            name.clone(),
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility,
                parent_id: None,
                metadata: Some(metadata),
                doc_comment: None,
            },
        );

        // Then regenerate ID using function name position (not body start)
        // This keeps the full body span but uses name position for ID consistency
        if let Some(name_node) = node.child_by_field_name("name") {
            let start_pos = name_node.start_position();
            let new_id = self.base.generate_id(&name, start_pos.row as u32, start_pos.column as u32);

            // Update ID in symbol and symbol_map
            let old_id = symbol.id.clone();
            symbol.id = new_id.clone();
            self.base.symbol_map.remove(&old_id);
            self.base.symbol_map.insert(new_id, symbol.clone());
        } else if node.kind() == "arrow_function" {
            if let Some(parent) = node.parent() {
                if parent.kind() == "variable_declarator" {
                    if let Some(var_name_node) = parent.child_by_field_name("name") {
                        let start_pos = var_name_node.start_position();
                        let new_id = self.base.generate_id(&name, start_pos.row as u32, start_pos.column as u32);

                        let old_id = symbol.id.clone();
                        symbol.id = new_id.clone();
                        self.base.symbol_map.remove(&old_id);
                        self.base.symbol_map.insert(new_id, symbol.clone());
                    }
                }
            }
        }

        symbol
    }

    // Port of Miller's extractClass method
    fn extract_class(&mut self, node: tree_sitter::Node) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        let visibility = self.base.extract_visibility(&node);
        let mut metadata = std::collections::HashMap::new();

        // Check for inheritance (extends clause)
        if let Some(heritage) = node.child_by_field_name("superclass") {
            let superclass_name = self.base.get_node_text(&heritage);
            metadata.insert("extends".to_string(), serde_json::json!(superclass_name));
        }

        // Check for abstract modifier
        let is_abstract = node
            .children(&mut node.walk())
            .any(|child| child.kind() == "abstract");
        metadata.insert("isAbstract".to_string(), serde_json::json!(is_abstract));

        use crate::extractors::base::SymbolOptions;
        self.base.create_symbol(
            &node,  // Use the whole class node for correct start/end positions
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: None,
                visibility,
                parent_id: None,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    fn extract_interface(&mut self, node: tree_sitter::Node) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        use crate::extractors::base::SymbolOptions;
        self.base
            .create_symbol(&node, name, SymbolKind::Interface, SymbolOptions::default())
    }

    // Port of Miller's extractMethod method
    fn extract_method(&mut self, node: tree_sitter::Node) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        // Determine if this is a constructor
        let symbol_kind = if name == "constructor" {
            SymbolKind::Constructor
        } else {
            SymbolKind::Method
        };

        let signature = self.build_function_signature(&node, &name);
        let visibility = self.base.extract_visibility(&node);

        // Check for modifiers
        let is_async = node
            .children(&mut node.walk())
            .any(|child| child.kind() == "async");
        let is_static = node
            .children(&mut node.walk())
            .any(|child| child.kind() == "static");
        let is_generator = node
            .children(&mut node.walk())
            .any(|child| child.kind() == "*");

        let parameters = self.extract_parameters(&node);
        let return_type = self.base.get_field_text(&node, "return_type");
        let type_parameters = self.extract_type_parameters(&node);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("isAsync".to_string(), serde_json::json!(is_async));
        metadata.insert("isStatic".to_string(), serde_json::json!(is_static));
        metadata.insert("isGenerator".to_string(), serde_json::json!(is_generator));
        metadata.insert("parameters".to_string(), serde_json::json!(parameters));
        if let Some(return_type) = return_type {
            metadata.insert("returnType".to_string(), serde_json::json!(return_type));
        }
        metadata.insert(
            "typeParameters".to_string(),
            serde_json::json!(type_parameters),
        );

        // Find parent class if this method is inside a class
        let parent_id = self.find_parent_class_id(&node);

        use crate::extractors::base::SymbolOptions;

        // CRITICAL FIX: Same as extract_function - keep full body span for containment
        let mut symbol = self.base.create_symbol(
            &node,
            name.clone(),
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility,
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        );

        // Regenerate ID using method name position (not body start)
        if let Some(name_node) = node.child_by_field_name("name") {
            let start_pos = name_node.start_position();
            let new_id = self.base.generate_id(&name, start_pos.row as u32, start_pos.column as u32);

            let old_id = symbol.id.clone();
            symbol.id = new_id.clone();
            self.base.symbol_map.remove(&old_id);
            self.base.symbol_map.insert(new_id, symbol.clone());
        }

        symbol
    }

    fn extract_variable(&mut self, node: tree_sitter::Node) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        // Check if this variable contains an arrow function (Miller logic)
        if let Some(value_node) = node.child_by_field_name("value") {
            if value_node.kind() == "arrow_function" {
                // Extract as a function instead of a variable
                return self.extract_function(value_node);
            }
        }

        use crate::extractors::base::SymbolOptions;
        self.base
            .create_symbol(&node, name, SymbolKind::Variable, SymbolOptions::default())
    }

    fn extract_type_alias(&mut self, node: tree_sitter::Node) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        use crate::extractors::base::SymbolOptions;
        self.base
            .create_symbol(&node, name, SymbolKind::Type, SymbolOptions::default())
    }

    fn extract_enum(&mut self, node: tree_sitter::Node) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        use crate::extractors::base::SymbolOptions;
        self.base
            .create_symbol(&node, name, SymbolKind::Enum, SymbolOptions::default())
    }

    fn extract_import(&mut self, node: tree_sitter::Node) -> Symbol {
        // For imports, extract the source (what's being imported from)
        // Example: import { foo } from './bar' -> name should be './bar' or 'foo'
        let name = if let Some(source_node) = node.child_by_field_name("source") {
            self.base
                .get_node_text(&source_node)
                .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                .to_string()
        } else {
            // Try to get import clause for named imports
            node.children(&mut node.walk())
                .find(|c| c.kind() == "import_clause")
                .and_then(|clause| clause.child_by_field_name("name"))
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "import".to_string())
        };

        use crate::extractors::base::SymbolOptions;
        self.base
            .create_symbol(&node, name, SymbolKind::Import, SymbolOptions::default())
    }

    fn extract_export(&mut self, node: tree_sitter::Node) -> Symbol {
        // For exports, extract what's being exported
        // Example: export class Foo -> name should be 'Foo'
        // Example: export { bar } from './baz' -> name should be 'bar'
        let name = if let Some(declaration_node) = node.child_by_field_name("declaration") {
            // export class/function/const/etc
            declaration_node
                .child_by_field_name("name")
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "export".to_string())
        } else if let Some(source_node) = node.child_by_field_name("source") {
            // export { ... } from '...'
            self.base
                .get_node_text(&source_node)
                .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                .to_string()
        } else {
            // export { ... }
            node.children(&mut node.walk())
                .find(|c| c.kind() == "export_clause")
                .and_then(|clause| clause.named_child(0))
                .and_then(|spec| spec.child_by_field_name("name"))
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "export".to_string())
        };

        use crate::extractors::base::SymbolOptions;
        self.base
            .create_symbol(&node, name, SymbolKind::Export, SymbolOptions::default())
    }

    fn extract_namespace(&mut self, node: tree_sitter::Node) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        use crate::extractors::base::SymbolOptions;
        self.base
            .create_symbol(&node, name, SymbolKind::Namespace, SymbolOptions::default())
    }

    fn extract_property(&mut self, node: tree_sitter::Node) -> Symbol {
        // Properties can have their name in different field names depending on context
        let name_node = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("key"));
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        use crate::extractors::base::SymbolOptions;
        self.base
            .create_symbol(&node, name, SymbolKind::Property, SymbolOptions::default())
    }

    // Helper method to find parent class for method symbols
    fn find_parent_class_id(&self, node: &tree_sitter::Node) -> Option<String> {
        let mut current = node.parent();
        while let Some(parent_node) = current {
            if parent_node.kind() == "class_declaration" {
                // Extract the class name and generate its ID
                if let Some(class_name_node) = parent_node.child_by_field_name("name") {
                    let class_name = self.base.get_node_text(&class_name_node);
                    let class_start = parent_node.start_position();
                    let candidates = [
                        self.base.generate_id(
                            &class_name,
                            class_start.row as u32,
                            class_start.column as u32,
                        ),
                        self.base.generate_id(
                            &class_name,
                            class_name_node.start_position().row as u32,
                            class_name_node.start_position().column as u32,
                        ),
                    ];

                    for candidate in candidates { 
                        if self.base.symbol_map.contains_key(&candidate) {
                            return Some(candidate);
                        }
                    }

                    if let Some((id, _symbol)) = self
                        .base
                        .symbol_map
                        .iter()
                        .find(|(_, symbol)| symbol.name == class_name && symbol.kind == SymbolKind::Class)
                    {
                        return Some(id.clone());
                    }
                }
            }
            current = parent_node.parent();
        }
        None
    }

    fn adjust_symbol_span(&mut self, mut symbol: Symbol, span_node: tree_sitter::Node) -> Symbol {
        let original_id = symbol.id.clone();
        let start_pos = span_node.start_position();
        let end_pos = span_node.end_position();

        symbol.start_line = (start_pos.row + 1) as u32;
        symbol.start_column = start_pos.column as u32;
        symbol.end_line = (end_pos.row + 1) as u32;
        symbol.end_column = end_pos.column as u32;
        symbol.start_byte = span_node.start_byte() as u32;
        symbol.end_byte = span_node.end_byte() as u32;

        let new_id = self
            .base
            .generate_id(&symbol.name, start_pos.row as u32, start_pos.column as u32);
        symbol.id = new_id.clone();

        self.base.symbol_map.remove(&original_id);
        self.base.symbol_map.insert(new_id, symbol.clone());

        symbol
    }

    // Helper methods (port from Miller)
    fn build_function_signature(&self, node: &tree_sitter::Node, name: &str) -> String {
        let params = self
            .base
            .get_field_text(node, "parameters")
            .or_else(|| self.base.get_field_text(node, "formal_parameters"))
            .unwrap_or_else(|| "()".to_string());
        let return_type = self.base.get_field_text(node, "return_type");

        let mut signature = format!("{}{}", name, params);
        if let Some(return_type) = return_type {
            signature.push_str(&format!(": {}", return_type));
        }

        signature
    }

    fn extract_type_parameters(&self, node: &tree_sitter::Node) -> Vec<String> {
        if let Some(type_params) = node.child_by_field_name("type_parameters") {
            let mut params = Vec::new();
            let mut cursor = type_params.walk();
            for child in type_params.children(&mut cursor) {
                if child.kind() == "type_parameter" {
                    params.push(self.base.get_node_text(&child));
                }
            }
            params
        } else {
            Vec::new()
        }
    }

    fn extract_parameters(&self, node: &tree_sitter::Node) -> Vec<String> {
        if let Some(params) = node.child_by_field_name("parameters") {
            let mut parameters = Vec::new();
            let mut cursor = params.walk();
            for child in params.children(&mut cursor) {
                if child.kind() == "parameter" || child.kind() == "identifier" {
                    parameters.push(self.base.get_node_text(&child));
                }
            }
            parameters
        } else {
            Vec::new()
        }
    }

    // Port of Miller's extractRelationships method
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.extract_call_relationships(tree.root_node(), symbols, &mut relationships);
        self.extract_inheritance_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    // Extract function call relationships
    fn extract_call_relationships(
        &self,
        node: tree_sitter::Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // Look for call expressions
        if node.kind() == "call_expression" {
            if let Some(function_node) = node.child_by_field_name("function") {
                let function_name = self.base.get_node_text(&function_node);

                // Find the calling function (containing function)
                if let Some(caller_symbol) = self.find_containing_function(node, symbols) {
                    // Find the called function symbol
                    if let Some(called_symbol) = symbols
                        .iter()
                        .find(|s| s.name == function_name && matches!(s.kind, SymbolKind::Function))
                    {
                        use crate::extractors::base::RelationshipKind;
                        let relationship = Relationship {
                            id: format!(
                                "{}_{}_{:?}_{}",
                                caller_symbol.id,
                                called_symbol.id,
                                RelationshipKind::Calls,
                                node.start_position().row
                            ),
                            from_symbol_id: caller_symbol.id.clone(),
                            to_symbol_id: called_symbol.id.clone(),
                            kind: RelationshipKind::Calls,
                            file_path: self.base.file_path.clone(),
                            line_number: (node.start_position().row + 1) as u32,
                            confidence: 1.0,
                            metadata: None,
                        };
                        relationships.push(relationship);
                    }
                }
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_call_relationships(child, symbols, relationships);
        }
    }

    // Extract inheritance relationships (extends, implements)
    fn extract_inheritance_relationships(
        &self,
        node: tree_sitter::Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // Look for extends_clause or class_heritage nodes (Miller's approach)
        match node.kind() {
            "extends_clause" | "class_heritage" => {
                if let Some(parent) = node.parent() {
                    if parent.kind() == "class_declaration" {
                        // Get the class name from parent
                        if let Some(class_name_node) = parent.child_by_field_name("name") {
                            let class_name = self.base.get_node_text(&class_name_node);

                            // Find the class symbol
                            if let Some(class_symbol) = symbols
                                .iter()
                                .find(|s| s.name == class_name && s.kind == SymbolKind::Class)
                            {
                                // Look for identifier or type_identifier children to get superclass name
                                let mut cursor = node.walk();
                                for child in node.children(&mut cursor) {
                                    if child.kind() == "identifier"
                                        || child.kind() == "type_identifier"
                                    {
                                        let superclass_name = self.base.get_node_text(&child);

                                        // Find the superclass symbol
                                        if let Some(superclass_symbol) = symbols.iter().find(|s| {
                                            s.name == superclass_name && s.kind == SymbolKind::Class
                                        }) {
                                            use crate::extractors::base::RelationshipKind;
                                            let relationship = Relationship {
                                                id: format!(
                                                    "{}_{}_{:?}_{}",
                                                    class_symbol.id,
                                                    superclass_symbol.id,
                                                    RelationshipKind::Extends,
                                                    child.start_position().row
                                                ),
                                                from_symbol_id: class_symbol.id.clone(),
                                                to_symbol_id: superclass_symbol.id.clone(),
                                                kind: RelationshipKind::Extends,
                                                file_path: self.base.file_path.clone(),
                                                line_number: (child.start_position().row + 1)
                                                    as u32,
                                                confidence: 1.0,
                                                metadata: None,
                                            };
                                            relationships.push(relationship);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_inheritance_relationships(child, symbols, relationships);
        }
    }

    // Helper to find the function that contains a given node
    fn find_containing_function<'a>(
        &self,
        node: tree_sitter::Node,
        symbols: &'a [Symbol],
    ) -> Option<&'a Symbol> {
        let mut current = Some(node);

        while let Some(current_node) = current {
            // Check if this node corresponds to a function symbol
            let position = current_node.start_position();
            let pos_line = (position.row + 1) as u32;

            // Find function symbols that contain this position
            for symbol in symbols {
                if matches!(symbol.kind, SymbolKind::Function)
                    && symbol.start_line <= pos_line
                    && symbol.end_line >= pos_line
                {
                    return Some(symbol);
                }
            }

            current = current_node.parent();
        }

        None
    }

    // Port of Miller's inferTypes method - basic type inference from variable assignments
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        // Parse the content to find variable assignments and infer types
        if let Ok(tree) = self.parse_content() {
            self.infer_types_from_tree(tree.root_node(), symbols, &mut types);
        }

        types
    }

    fn parse_content(&self) -> Result<tree_sitter::Tree, Box<dyn std::error::Error>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into())?;
        let tree = parser
            .parse(&self.base.content, None)
            .ok_or("Failed to parse content")?;
        Ok(tree)
    }

    fn infer_types_from_tree(
        &self,
        node: tree_sitter::Node,
        symbols: &[Symbol],
        types: &mut HashMap<String, String>,
    ) {
        // Look for variable declarations and assignments
        if node.kind() == "variable_declarator" {
            if let Some(name_node) = node.child_by_field_name("name") {
                let var_name = self.base.get_node_text(&name_node);

                // Find the symbol for this variable
                if let Some(symbol) = symbols.iter().find(|s| s.name == var_name) {
                    // Look at the value to infer the type
                    if let Some(value_node) = node.child_by_field_name("value") {
                        let inferred_type = self.infer_type_from_value(&value_node);
                        types.insert(symbol.id.clone(), inferred_type);
                    }
                }
            }
        }
        // Look for function declarations
        else if node.kind() == "function_declaration"
            || node.kind() == "arrow_function"
            || node.kind() == "function_expression"
        {
            if let Some(name_node) = node.child_by_field_name("name") {
                let func_name = self.base.get_node_text(&name_node);

                // Find the function symbol
                if let Some(symbol) = symbols
                    .iter()
                    .find(|s| s.name == func_name && s.kind == SymbolKind::Function)
                {
                    // For functions, try to infer return type or just use "function"
                    let return_type = self.infer_function_return_type(&node);
                    types.insert(symbol.id.clone(), return_type);
                }
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.infer_types_from_tree(child, symbols, types);
        }
    }

    fn infer_type_from_value(&self, value_node: &tree_sitter::Node) -> String {
        match value_node.kind() {
            "string" => "string".to_string(),
            "number" => "number".to_string(),
            "true" | "false" => "boolean".to_string(),
            "array" => "array".to_string(),
            "object" => "object".to_string(),
            "null" => "null".to_string(),
            "undefined" => "undefined".to_string(),
            "arrow_function" | "function" | "function_expression" => "function".to_string(),
            "call_expression" => {
                // Try to infer based on common function calls
                if let Some(function_node) = value_node.child_by_field_name("function") {
                    let function_name = self.base.get_node_text(&function_node);
                    match function_name.as_str() {
                        "fetch" => "Promise<Response>".to_string(),
                        "Promise.resolve" => "Promise<any>".to_string(),
                        "JSON.parse" => "any".to_string(),
                        "JSON.stringify" => "string".to_string(),
                        _ => "any".to_string(),
                    }
                } else {
                    "any".to_string()
                }
            }
            _ => "any".to_string(),
        }
    }

    fn infer_function_return_type(&self, func_node: &tree_sitter::Node) -> String {
        // Check for async functions
        let is_async = func_node
            .children(&mut func_node.walk())
            .any(|child| child.kind() == "async");

        if is_async {
            return "Promise<any>".to_string();
        }

        // Look for return statements in the function body
        if let Some(body_node) = func_node.child_by_field_name("body") {
            let mut return_types = Vec::new();
            self.collect_return_types(&body_node, &mut return_types);

            if !return_types.is_empty() {
                // If we found return statements, try to unify types
                if return_types.iter().all(|t| t == "string") {
                    return "string".to_string();
                } else if return_types.iter().all(|t| t == "number") {
                    return "number".to_string();
                } else if return_types.iter().all(|t| t == "boolean") {
                    return "boolean".to_string();
                }
                // Mixed types or complex types
                return "any".to_string();
            }
        }

        // Default to function type
        "function".to_string()
    }

    fn collect_return_types(&self, node: &tree_sitter::Node, return_types: &mut Vec<String>) {
        if node.kind() == "return_statement" {
            if let Some(value_node) = node.child_by_field_name("argument") {
                let return_type = self.infer_type_from_value(&value_node);
                return_types.push(return_type);
            }
        }

        // Recursively search children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_return_types(&child, return_types);
        }
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
    fn walk_tree_for_identifiers(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        match node.kind() {
            // Function/method calls: foo(), object.method()
            "call_expression" => {
                // The function being called is in the "function" field
                if let Some(function_node) = node.child_by_field_name("function") {
                    match function_node.kind() {
                        "identifier" => {
                            // Simple function call: foo()
                            let name = self.base.get_node_text(&function_node);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &function_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                        "member_expression" => {
                            // Member call: object.method()
                            // Extract the rightmost identifier (the method name)
                            if let Some(property_node) =
                                function_node.child_by_field_name("property")
                            {
                                let name = self.base.get_node_text(&property_node);
                                let containing_symbol_id =
                                    self.find_containing_symbol_id(node, symbol_map);

                                self.base.create_identifier(
                                    &property_node,
                                    name,
                                    IdentifierKind::Call,
                                    containing_symbol_id,
                                );
                            }
                        }
                        _ => {
                            // Other cases like computed member expressions
                            // Skip for now
                        }
                    }
                }
            }

            // Member access: object.property
            "member_expression" => {
                // Only extract if it's NOT part of a call_expression
                // (we handle those in the call_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression" {
                        // Check if this member_expression is the function being called
                        if let Some(function_node) = parent.child_by_field_name("function") {
                            if function_node.id() == node.id() {
                                return; // Skip - handled by call_expression
                            }
                        }
                    }
                }

                // Extract the rightmost identifier (the property name)
                if let Some(property_node) = node.child_by_field_name("property") {
                    let name = self.base.get_node_text(&property_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &property_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            _ => {
                // Skip other node types for now
                // Future: type usage, constructor calls, etc.
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
