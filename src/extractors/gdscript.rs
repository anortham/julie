use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, SymbolOptions, Visibility};
use tree_sitter::{Tree, Node};
use std::collections::{HashMap, HashSet};
use serde_json::Value;

pub struct GDScriptExtractor {
    base: BaseExtractor,
    pending_inheritance: HashMap<String, String>, // className -> baseClassName
    processed_positions: HashSet<String>, // Track processed node positions
}

impl GDScriptExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
            pending_inheritance: HashMap::new(),
            processed_positions: HashSet::new(),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.pending_inheritance.clear();
        self.processed_positions.clear();

        let root_node = tree.root_node();
        // First pass: collect inheritance information
        self.collect_inheritance_info(root_node);

        // Check for top-level extends statement (creates implicit class)
        let mut implicit_class_id: Option<String> = None;
        for i in 0..root_node.child_count() {
            if let Some(child) = root_node.child(i) {
                if child.kind() == "extends_statement" {
                    if let Some(type_node) = self.find_child_by_type(child, "type") {
                        let base_class_name = self.base.get_node_text(&type_node);

                        // Create implicit class based on file name
                        let file_name = self.base.file_path
                            .split('/')
                            .last()
                            .unwrap_or("ImplicitClass")
                            .replace(".gd", "");

                        let mut metadata = HashMap::new();
                        metadata.insert("baseClass".to_string(), Value::String(base_class_name.clone()));

                        let implicit_class = self.base.create_symbol(
                            &child,
                            file_name,
                            SymbolKind::Class,
                            SymbolOptions {
                                signature: Some(format!("extends {}", base_class_name)),
                                visibility: Some(Visibility::Public),
                                parent_id: None,
                                metadata: Some(metadata),
                                doc_comment: None,
                            },
                        );

                        implicit_class_id = Some(implicit_class.id.clone());
                        symbols.push(implicit_class);
                        break;
                    }
                }
            }
        }

        // Second pass: extract symbols with implicit class context
        self.traverse_node(root_node, implicit_class_id.as_ref(), &mut symbols);

        symbols
    }

    pub fn extract_relationships(&mut self, _tree: &Tree, _symbols: &[Symbol]) -> Vec<Relationship> {
        // For now, return empty relationships - this can be extended later
        Vec::new()
    }

    fn collect_inheritance_info(&mut self, node: Node) {
        // Look for adjacent class_name_statement and extends_statement pairs
        for i in 0..node.child_count() {
            if let (Some(current_child), Some(next_child)) = (node.child(i), node.child(i + 1)) {
                // Check for class_name followed by extends
                if current_child.kind() == "class_name_statement" && next_child.kind() == "extends_statement" {
                    if let (Some(name_node), Some(type_node)) = (
                        self.find_child_by_type(current_child, "name"),
                        self.find_child_by_type(next_child, "type")
                    ) {
                        let class_name = self.base.get_node_text(&name_node);
                        if let Some(identifier_node) = self.find_child_by_type(type_node, "identifier") {
                            let base_class_name = self.base.get_node_text(&identifier_node);
                            self.pending_inheritance.insert(class_name, base_class_name);
                        }
                    }
                }

                // Check for extends followed by class_name (reverse order)
                if current_child.kind() == "extends_statement" && next_child.kind() == "class_name_statement" {
                    if let (Some(type_node), Some(name_node)) = (
                        self.find_child_by_type(current_child, "type"),
                        self.find_child_by_type(next_child, "name")
                    ) {
                        let class_name = self.base.get_node_text(&name_node);
                        if let Some(identifier_node) = self.find_child_by_type(type_node, "identifier") {
                            let base_class_name = self.base.get_node_text(&identifier_node);
                            self.pending_inheritance.insert(class_name, base_class_name);
                        }
                    }
                }
            }
        }

        // Recursively collect from children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.collect_inheritance_info(child);
            }
        }
    }

    fn traverse_node(&mut self, node: Node, parent_id: Option<&String>, symbols: &mut Vec<Symbol>) {
        // Create position-based key to prevent double processing
        let position_key = format!("{}:{}:{}", node.start_position().row, node.start_position().column, node.kind());

        if self.processed_positions.contains(&position_key) {
            return;
        }
        self.processed_positions.insert(position_key);

        let mut extracted_symbol: Option<Symbol> = None;

        match node.kind() {
            "class_name_statement" => {
                if let Some(symbol) = self.extract_class_name_statement(node, parent_id) {
                    extracted_symbol = Some(symbol);
                }
            }
            "class" => {
                if let Some(symbol) = self.extract_class_definition(node, parent_id) {
                    extracted_symbol = Some(symbol);
                }
            }
            "function_definition" => {
                if let Some(symbol) = self.extract_function_definition(node, parent_id, symbols) {
                    extracted_symbol = Some(symbol);
                }
            }
            "func" => {
                // Skip if this func node is part of a function_definition
                if let Some(parent) = node.parent() {
                    if parent.kind() != "function_definition" {
                        if let Some(symbol) = self.extract_function_definition(node, parent_id, symbols) {
                            extracted_symbol = Some(symbol);
                        }
                    }
                }
            }
            "constructor_definition" => {
                if let Some(symbol) = self.extract_constructor_definition(node, parent_id) {
                    extracted_symbol = Some(symbol);
                }
            }
            "var" => {
                // Skip if this var node is part of a variable_statement
                if let Some(parent) = node.parent() {
                    if parent.kind() != "variable_statement" {
                        if let Some(symbol) = self.extract_variable_statement(node, parent_id) {
                            extracted_symbol = Some(symbol);
                        }
                    }
                }
            }
            "variable_statement" => {
                if let Some(symbol) = self.extract_variable_from_statement(node, parent_id, symbols) {
                    extracted_symbol = Some(symbol);
                }
            }
            "const" => {
                if let Some(symbol) = self.extract_constant_statement(node, parent_id) {
                    extracted_symbol = Some(symbol);
                }
            }
            "enum_definition" => {
                if let Some(symbol) = self.extract_enum_definition(node, parent_id) {
                    extracted_symbol = Some(symbol);
                }
            }
            "signal_statement" | "signal" => {
                if let Some(symbol) = self.extract_signal_statement(node, parent_id) {
                    extracted_symbol = Some(symbol);
                }
            }
            _ => {}
        }

        if let Some(symbol) = extracted_symbol {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);

            // Traverse children with current symbol as parent
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    self.traverse_node(child, Some(&symbol_id), symbols);
                }
            }
        } else {
            // Traverse children with current parent
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    self.traverse_node(child, parent_id, symbols);
                }
            }
        }
    }

    fn extract_class_name_statement(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.find_child_by_type(node, "name")?;
        let name = self.base.get_node_text(&name_node);

        // Include preceding annotations in signature
        let mut signature = self.base.get_node_text(&node);
        if let Some(parent) = node.parent() {
            // Look for annotations before this class_name_statement
            for i in 0..parent.child_count() {
                if let Some(child) = parent.child(i) {
                    if child.kind() == "class_name_statement" &&
                       self.base.get_node_text(&child) == self.base.get_node_text(&node) {
                        // Found our node, now look backwards for annotations
                        if i > 0 {
                            for j in (0..i).rev() {
                                if let Some(prev_child) = parent.child(j) {
                                    if prev_child.kind() == "annotation" {
                                        let annotation_text = self.base.get_node_text(&prev_child);
                                        signature = format!("{}\n{}", annotation_text, signature);
                                        break;
                                    }
                                    if prev_child.kind() == "class_name_statement" {
                                        break;
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }

        let mut metadata = HashMap::new();
        if let Some(base_class_name) = self.pending_inheritance.get(&name) {
            metadata.insert("baseClass".to_string(), Value::String(base_class_name.clone()));
        }

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata: if metadata.is_empty() { None } else { Some(metadata) },
                doc_comment: None,
            },
        ))
    }

    fn extract_class_definition(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        // For `class` nodes, look for the name node in the parent's children
        let parent_node = node.parent()?;
        let mut name_node: Option<Node> = None;

        // Find the index of the current class node
        let mut class_index = None;
        for i in 0..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.id() == node.id() {
                    class_index = Some(i);
                    break;
                }
            }
        }

        // Look for 'name' node after the 'class' node
        if let Some(idx) = class_index {
            for i in (idx + 1)..parent_node.child_count() {
                if let Some(child) = parent_node.child(i) {
                    if child.kind() == "name" {
                        name_node = Some(child);
                        break;
                    }
                }
            }
        }

        let name_node = name_node?;
        let name = self.base.get_node_text(&name_node);
        let signature = format!("class {}:", name);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_function_definition(&mut self, node: Node, parent_id: Option<&String>, symbols: &[Symbol]) -> Option<Symbol> {
        let (name_node, func_node, parent_node) = if node.kind() == "function_definition" {
            // Processing function_definition node - find child nodes
            let children = node.children(&mut node.walk()).collect::<Vec<_>>();
            let func_node = children.iter().find(|c| c.kind() == "func").cloned();
            let name_node = children.iter().find(|c| c.kind() == "name").cloned();
            (name_node, func_node, Some(node))
        } else if node.kind() == "func" {
            // Processing func node - look for sibling name node
            let parent_node = node.parent()?;
            let mut name_node = None;

            // Find func index and look for name after it
            for i in 0..parent_node.child_count() {
                if let Some(child) = parent_node.child(i) {
                    if child.id() == node.id() {
                        // Found func node, look for name after it
                        for j in (i + 1)..parent_node.child_count() {
                            if let Some(sibling) = parent_node.child(j) {
                                if sibling.kind() == "name" {
                                    name_node = Some(sibling);
                                    break;
                                }
                            }
                        }
                        break;
                    }
                }
            }
            (name_node, Some(node), Some(parent_node))
        } else {
            return None;
        };

        let name_node = name_node?;
        let parent_node = parent_node?;
        let name = self.base.get_node_text(&name_node);
        let signature = self.base.get_node_text(&parent_node);

        // Determine visibility based on naming convention
        let visibility = if name.starts_with('_') {
            Visibility::Private
        } else {
            Visibility::Public
        };

        // Determine symbol kind based on context and name
        let kind = if name == "_init" {
            SymbolKind::Constructor
        } else if let Some(parent_id) = parent_id {
            // Find the parent symbol to determine context
            if let Some(parent_symbol) = symbols.iter().find(|s| &s.id == parent_id) {
                if parent_symbol.kind == SymbolKind::Class {
                    let is_implicit_class = parent_symbol.signature.as_ref()
                        .map(|s| s.contains("extends") && !s.contains("class_name"))
                        .unwrap_or(false);

                    let is_explicit_class = parent_symbol.signature.as_ref()
                        .map(|s| s.contains("class_name"))
                        .unwrap_or(false);

                    if is_implicit_class {
                        // In implicit classes, lifecycle callbacks are methods, custom functions remain functions
                        let lifecycle_prefixes = ["_ready", "_enter_tree", "_exit_tree", "_process", "_physics_process",
                                                "_input", "_unhandled_input", "_unhandled_key_input", "_notification",
                                                "_draw", "_on_", "_handle_"];

                        let is_lifecycle_callback = name.starts_with('_') &&
                            lifecycle_prefixes.iter().any(|prefix| name.starts_with(prefix));

                        if is_lifecycle_callback {
                            SymbolKind::Method
                        } else {
                            SymbolKind::Function
                        }
                    } else if is_explicit_class {
                        // In explicit classes, all functions are methods
                        SymbolKind::Method
                    } else {
                        SymbolKind::Method
                    }
                } else {
                    SymbolKind::Function
                }
            } else {
                SymbolKind::Function
            }
        } else {
            SymbolKind::Function
        };

        Some(self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_constructor_definition(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let signature = self.base.get_node_text(&node);

        Some(self.base.create_symbol(
            &node,
            "_init".to_string(),
            SymbolKind::Constructor,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_variable_statement(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let parent_node = node.parent()?;
        let mut name_node = None;

        // Find var index and look for name after it
        for i in 0..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.id() == node.id() {
                    // Found var node, look for name after it
                    for j in (i + 1)..parent_node.child_count() {
                        if let Some(sibling) = parent_node.child(j) {
                            if sibling.kind() == "name" {
                                name_node = Some(sibling);
                                break;
                            }
                        }
                    }
                    break;
                }
            }
        }

        let name_node = name_node?;
        let name = self.base.get_node_text(&name_node);
        let signature = self.base.get_node_text(&parent_node);

        // Extract annotations and determine properties
        let (annotations, full_signature) = self.extract_variable_annotations(parent_node, &signature);
        let is_exported = annotations.iter().any(|a| a.starts_with("@export"));
        let is_onready = annotations.iter().any(|a| a.starts_with("@onready"));

        // Determine data type
        let data_type = self.extract_variable_type(parent_node, &name_node).unwrap_or_else(|| "unknown".to_string());

        // Determine visibility
        let visibility = if is_exported {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let mut metadata = HashMap::new();
        metadata.insert("dataType".to_string(), Value::String(data_type));
        if !annotations.is_empty() {
            let annotations_json = annotations.iter().map(|a| Value::String(a.clone())).collect::<Vec<_>>();
            metadata.insert("annotations".to_string(), Value::Array(annotations_json));
        }
        metadata.insert("isExported".to_string(), Value::Bool(is_exported));
        metadata.insert("isOnReady".to_string(), Value::Bool(is_onready));

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Field,
            SymbolOptions {
                signature: Some(full_signature),
                visibility: Some(visibility),
                parent_id: parent_id.cloned(),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_constant_statement(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let parent_node = node.parent()?;
        let mut name_node = None;

        // Find const index and look for name after it
        for i in 0..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.kind() == "const" {
                    // Found const node, look for name after it
                    for j in (i + 1)..parent_node.child_count() {
                        if let Some(sibling) = parent_node.child(j) {
                            if sibling.kind() == "name" {
                                name_node = Some(sibling);
                                break;
                            }
                        }
                    }
                    break;
                }
            }
        }

        let name_node = name_node?;
        let name = self.base.get_node_text(&name_node);
        let signature = self.base.get_node_text(&parent_node);

        // Get type annotation
        let data_type = self.extract_variable_type(parent_node, &name_node).unwrap_or_else(|| "unknown".to_string());

        let mut metadata = HashMap::new();
        metadata.insert("dataType".to_string(), Value::String(data_type));

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_signal_statement(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.find_child_by_type(node, "name")?;
        let name = self.base.get_node_text(&name_node);
        let signature = self.base.get_node_text(&node);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Event,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_enum_definition(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        // For enum_definition nodes, find the identifier child directly
        let name = if let Some(name_node) = self.find_child_by_type(node, "identifier") {
            self.base.get_node_text(&name_node)
        } else {
            // Try to extract name from the text pattern: "enum Name { ... }"
            let text = self.base.get_node_text(&node);
            if let Some(captures) = regex::Regex::new(r"enum\s+(\w+)\s*\{").unwrap().captures(&text) {
                captures.get(1)?.as_str().to_string()
            } else {
                return None;
            }
        };

        let signature = self.base.get_node_text(&node);

        let enum_symbol = self.base.create_symbol(
            &node,
            name,
            SymbolKind::Enum,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: None,
            },
        );

        // Note: Enum members would be extracted in the traversal as children
        Some(enum_symbol)
    }

    fn extract_variable_from_statement(&mut self, node: Node, parent_id: Option<&String>, symbols: &[Symbol]) -> Option<Symbol> {
        // For variable_statement nodes, find the var child and extract from there
        let var_node = self.find_child_by_type(node, "var")?;

        // Check if we should use class_name class as parent instead of implicit class
        let actual_parent_id = if let Some(node_parent) = node.parent() {
            if node_parent.kind() == "source" {
                // Find the closest preceding class_name statement
                self.find_closest_class_name_parent(node, parent_id, symbols)
                    .unwrap_or_else(|| parent_id.cloned().unwrap_or_default())
            } else {
                parent_id.cloned().unwrap_or_default()
            }
        } else {
            parent_id.cloned().unwrap_or_default()
        };

        self.extract_variable_statement(var_node, Some(&actual_parent_id))
    }

    fn find_closest_class_name_parent(&self, node: Node, default_parent: Option<&String>, symbols: &[Symbol]) -> Option<String> {
        let source_parent = node.parent()?;
        let class_name_classes: Vec<_> = symbols.iter()
            .filter(|s| s.kind == SymbolKind::Class &&
                        s.signature.as_ref().map(|sig| sig.contains("class_name")).unwrap_or(false) &&
                        s.parent_id == default_parent.map(|s| s.clone()))
            .collect();

        if class_name_classes.is_empty() {
            return None;
        }

        // Find variable's position in source children
        let mut var_index = None;
        for i in 0..source_parent.child_count() {
            if let Some(child) = source_parent.child(i) {
                if child.kind() == "variable_statement" &&
                   child.start_position().row == node.start_position().row &&
                   child.start_position().column == node.start_position().column {
                    var_index = Some(i);
                    break;
                }
            }
        }

        let var_index = var_index?;

        // Find the last class_name_statement before this variable
        for i in (0..var_index).rev() {
            if let Some(child) = source_parent.child(i) {
                if child.kind() == "class_name_statement" {
                    if let Some(name_node) = self.find_child_by_type(child, "name") {
                        let class_name = self.base.get_node_text(&name_node);
                        if let Some(matching_class) = class_name_classes.iter().find(|c| c.name == class_name) {
                            return Some(matching_class.id.clone());
                        }
                    }
                }
            }
        }

        None
    }

    fn extract_variable_annotations(&self, parent_node: Node, signature: &str) -> (Vec<String>, String) {
        let mut annotations = Vec::new();
        let mut full_signature = signature.to_string();

        // Check for annotations as children
        for i in 0..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.kind() == "annotations" {
                    for j in 0..child.child_count() {
                        if let Some(annotation_child) = child.child(j) {
                            if annotation_child.kind() == "annotation" {
                                let annotation_text = self.base.get_node_text(&annotation_child);
                                annotations.push(annotation_text);
                            }
                        }
                    }
                }
            }
        }

        // Also look for sibling annotations at source level
        if let Some(grandparent) = parent_node.parent() {
            // Find parent node index
            let mut node_index = None;
            for i in 0..grandparent.child_count() {
                if let Some(child) = grandparent.child(i) {
                    if child.id() == parent_node.id() {
                        node_index = Some(i);
                        break;
                    }
                }
            }

            if let Some(idx) = node_index {
                let mut annotation_texts = Vec::new();

                // Look backwards for annotations
                for i in (0..idx).rev() {
                    if let Some(child) = grandparent.child(i) {
                        if child.kind() == "annotations" {
                            for j in 0..child.child_count() {
                                if let Some(annotation_child) = child.child(j) {
                                    if annotation_child.kind() == "annotation" {
                                        let annotation_text = self.base.get_node_text(&annotation_child);
                                        annotations.push(annotation_text.clone());
                                        annotation_texts.insert(0, annotation_text);
                                    }
                                }
                            }
                        } else if child.kind() == "annotation" {
                            let annotation_text = self.base.get_node_text(&child);
                            annotations.push(annotation_text.clone());
                            annotation_texts.insert(0, annotation_text);
                        } else if child.kind() == "variable_statement" || child.kind() == "var" {
                            break;
                        }
                    }
                }

                // Build full signature with annotations
                if !annotation_texts.is_empty() {
                    full_signature = format!("{}\n{}", annotation_texts.join("\n"), signature);
                }
            }
        }

        (annotations, full_signature)
    }

    fn extract_variable_type(&self, parent_node: Node, name_node: &Node) -> Option<String> {
        // Look for type annotation as sibling after the name
        let mut name_index = None;
        for i in 0..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.id() == name_node.id() {
                    name_index = Some(i);
                    break;
                }
            }
        }

        let name_index = name_index?;

        // Look for type annotation after name
        for i in (name_index + 1)..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.kind() == "type" {
                    if let Some(identifier_node) = self.find_child_by_type(child, "identifier") {
                        return Some(self.base.get_node_text(&identifier_node));
                    } else {
                        // Handle complex types (e.g., Array[String])
                        return Some(self.base.get_node_text(&child).trim().to_string());
                    }
                }
            }
        }

        // If no explicit type, try to infer from assignment
        for i in (name_index + 1)..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.kind() == "=" {
                    if let Some(value_node) = parent_node.child(i + 1) {
                        return Some(self.infer_type_from_expression(value_node));
                    }
                }
            }
        }

        None
    }

    fn infer_type_from_expression(&self, node: Node) -> String {
        match node.kind() {
            "string" => "String".to_string(),
            "integer" => "int".to_string(),
            "float" => "float".to_string(),
            "true" | "false" => "bool".to_string(),
            "null" => "null".to_string(),
            "identifier" => {
                let text = self.base.get_node_text(&node);
                if text.starts_with('$') || text.contains("Node") {
                    text.replace('$', "")
                } else {
                    "unknown".to_string()
                }
            }
            "call_expression" => {
                if let Some(callee_node) = self.find_child_by_type(node, "identifier") {
                    let callee_text = self.base.get_node_text(&callee_node);
                    // Common Godot constructors
                    if ["Vector2", "Vector3", "Color", "Rect2", "Transform2D"].contains(&callee_text.as_str()) {
                        return callee_text;
                    }
                }
                "unknown".to_string()
            }
            _ => "unknown".to_string(),
        }
    }

    fn find_child_by_type<'a>(&self, node: Node<'a>, child_type: &str) -> Option<Node<'a>> {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == child_type {
                    return Some(child);
                }
            }
        }
        None
    }
}