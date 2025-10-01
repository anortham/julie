use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Python extractor for extracting symbols and relationships from Python source code
/// Port of Miller's Python extractor with comprehensive Python feature support
pub struct PythonExtractor {
    base: BaseExtractor,
}

impl PythonExtractor {
    pub fn new(file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new("python".to_string(), file_path, content),
        }
    }

    /// Extract all symbols from Python source code
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.traverse_tree(tree.root_node(), &mut symbols);
        symbols
    }

    fn traverse_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>) {
        match node.kind() {
            "class_definition" => {
                let symbol = self.extract_class(node);
                symbols.push(symbol);
            }
            "function_definition" => {
                let symbol = self.extract_function(node);
                symbols.push(symbol);
            }
            "async_function_definition" => {
                let symbol = self.extract_async_function(node);
                symbols.push(symbol);
            }
            "assignment" => {
                if let Some(symbol) = self.extract_assignment(node) {
                    symbols.push(symbol);
                }
            }
            "import_statement" | "import_from_statement" => {
                let import_symbols = self.extract_imports(node);
                symbols.extend(import_symbols);
            }
            "lambda" => {
                let symbol = self.extract_lambda(node);
                symbols.push(symbol);
            }
            _ => {}
        }

        // Recursively traverse children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_tree(child, symbols);
        }
    }

    // Port of Miller's extractClass method with full Python support
    fn extract_class(&mut self, node: Node) -> Symbol {
        // For Python, the class name is typically the second child (after "class" keyword)
        let name = if let Some(identifier_node) = node.children(&mut node.walk()).nth(1) {
            if identifier_node.kind() == "identifier" {
                self.base.get_node_text(&identifier_node)
            } else {
                "Anonymous".to_string()
            }
        } else {
            "Anonymous".to_string()
        };

        // Extract base classes and metaclass arguments
        let superclasses_node = node.child_by_field_name("superclasses");
        let mut extends_info = String::new();
        let mut is_enum = false;
        let mut is_protocol = false;
        let all_args = if let Some(superclasses) = superclasses_node {
            let all_args = self.extract_argument_list(&superclasses);

            // Separate regular base classes from keyword arguments
            let bases: Vec<_> = all_args
                .iter()
                .filter(|arg| !arg.contains('='))
                .cloned()
                .collect();
            let keyword_args: Vec<_> = all_args
                .iter()
                .filter(|arg| arg.contains('='))
                .cloned()
                .collect();

            // Check if this is an Enum class
            is_enum = bases
                .iter()
                .any(|base| base == "Enum" || base.contains("Enum"));

            // Check if this is a Protocol class (should be treated as Interface)
            is_protocol = bases
                .iter()
                .any(|base| base == "Protocol" || base.contains("Protocol"));

            // Build extends information
            let mut extends_parts = Vec::new();
            if !bases.is_empty() {
                extends_parts.push(format!("extends {}", bases.join(", ")));
            }

            // Add metaclass info if present
            if let Some(metaclass_arg) = keyword_args
                .iter()
                .find(|arg| arg.starts_with("metaclass="))
            {
                extends_parts.push(metaclass_arg.clone());
            }

            if !extends_parts.is_empty() {
                extends_info = format!(" {}", extends_parts.join(" "));
            }

            all_args
        } else {
            Vec::new()
        };

        // Extract decorators
        let decorators = self.extract_decorators(&node);
        let decorator_info = if decorators.is_empty() {
            String::new()
        } else {
            format!("@{} ", decorators.join(" @"))
        };

        let signature = format!("{}class {}{}", decorator_info, name, extends_info);

        // Determine the symbol kind based on base classes
        let symbol_kind = if is_enum {
            SymbolKind::Enum
        } else if is_protocol {
            SymbolKind::Interface
        } else {
            SymbolKind::Class
        };

        // Extract docstring
        let doc_comment = self.extract_docstring(&node);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("decorators".to_string(), serde_json::json!(decorators));
        metadata.insert("superclasses".to_string(), serde_json::json!(all_args));
        metadata.insert("isEnum".to_string(), serde_json::json!(is_enum));

        use crate::extractors::base::SymbolOptions;
        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: None,
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    fn extract_function(&mut self, node: Node) -> Symbol {
        // Extract function name from 'name' field
        let name = if let Some(name_node) = node.child_by_field_name("name") {
            self.base.get_node_text(&name_node)
        } else {
            "Anonymous".to_string()
        };

        // Check if it's an async function
        let is_async = self.has_async_keyword(&node);

        // Extract parameters from 'parameters' field
        let parameters_node = node.child_by_field_name("parameters");
        let params = if let Some(parameters_node) = parameters_node {
            self.extract_parameters(&parameters_node)
        } else {
            Vec::new()
        };

        // Extract return type annotation from 'return_type' field
        let return_type = if let Some(return_type_node) = node.child_by_field_name("return_type") {
            format!(": {}", self.base.get_node_text(&return_type_node))
        } else {
            String::new()
        };

        // Extract decorators
        let decorators = self.extract_decorators(&node);
        let decorator_info = if decorators.is_empty() {
            String::new()
        } else {
            format!("@{} ", decorators.join(" @"))
        };

        // Build signature
        let async_prefix = if is_async { "async " } else { "" };
        let signature = format!(
            "{}{}def {}({}){}",
            decorator_info,
            async_prefix,
            name,
            params.join(", "),
            return_type
        );

        // Determine if it's a method or function based on context
        let (symbol_kind, parent_id) = self.determine_function_kind(&node, &name);

        // Extract docstring
        let doc_comment = self.extract_docstring(&node);

        // Infer visibility from name
        let visibility = self.infer_visibility(&name);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("decorators".to_string(), serde_json::json!(decorators));
        metadata.insert("isAsync".to_string(), serde_json::json!(is_async));
        metadata.insert("returnType".to_string(), serde_json::json!(return_type));

        use crate::extractors::base::SymbolOptions;
        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id,
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    fn extract_async_function(&mut self, node: Node) -> Symbol {
        // Async functions are handled the same way as regular functions
        // The has_async_keyword check will detect the async keyword
        self.extract_function(node)
    }

    fn extract_assignment(&mut self, node: Node) -> Option<Symbol> {
        // Handle assignments like: x = 5, x: int = 5, self.x = 5
        let left = node.child_by_field_name("left")?;
        let right = node.child_by_field_name("right");

        let (name, mut symbol_kind) = match left.kind() {
            "identifier" => {
                let name = self.base.get_node_text(&left);
                (name, SymbolKind::Variable)
            }
            "attribute" => {
                // Handle self.attribute assignments
                let object_node = left.child_by_field_name("object");
                let attribute_node = left.child_by_field_name("attribute");

                if let (Some(object_node), Some(attribute_node)) = (object_node, attribute_node) {
                    if self.base.get_node_text(&object_node) == "self" {
                        let name = self.base.get_node_text(&attribute_node);
                        (name, SymbolKind::Property)
                    } else {
                        return None; // Skip non-self attributes for now
                    }
                } else {
                    return None;
                }
            }
            "pattern_list" | "tuple_pattern" => {
                // Handle multiple assignment: a, b = 1, 2
                return None; // TODO: Handle multiple assignments
            }
            _ => return None,
        };

        // Check if this is a special class attribute
        if name == "__slots__" {
            symbol_kind = SymbolKind::Property;
        }
        // Check if it's a constant (uppercase name)
        else if symbol_kind == SymbolKind::Variable
            && name == name.to_uppercase()
            && name.len() > 1
        {
            // Check if we're inside an enum class
            if self.is_inside_enum_class(&node) {
                symbol_kind = SymbolKind::EnumMember;
            } else {
                symbol_kind = SymbolKind::Constant;
            }
        }

        // Extract type annotation from assignment node
        let type_annotation = if let Some(type_node) = self.find_type_annotation(&node) {
            format!(": {}", self.base.get_node_text(&type_node))
        } else {
            String::new()
        };

        // Extract value for signature
        let value = if let Some(right) = right {
            self.base.get_node_text(&right)
        } else {
            String::new()
        };

        let signature = format!("{}{} = {}", name, type_annotation, value);

        // Infer visibility from name
        let visibility = self.infer_visibility(&name);

        // TODO: Handle parent_id for nested assignments
        let parent_id = None;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "hasTypeAnnotation".to_string(),
            serde_json::json!(!type_annotation.is_empty()),
        );

        use crate::extractors::base::SymbolOptions;
        Some(self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        ))
    }

    fn extract_imports(&mut self, node: Node) -> Vec<Symbol> {
        let mut imports = Vec::new();

        match node.kind() {
            "import_statement" => {
                // Handle single import: import module [as alias]
                if let Some(import_symbol) = self.extract_single_import(&node) {
                    imports.push(import_symbol);
                }
            }
            "import_from_statement" => {
                // Handle from import: from module import name1, name2, name3
                if let Some(module_node) = node.child_by_field_name("module_name") {
                    let module = self.base.get_node_text(&module_node);

                    // Find all import names after the 'import' keyword
                    let mut found_import_keyword = false;
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "import" {
                            found_import_keyword = true;
                            continue;
                        }

                        if found_import_keyword {
                            match child.kind() {
                                "dotted_name" => {
                                    // Simple import: from module import name
                                    let name = self.base.get_node_text(&child);
                                    let import_text = format!("from {} import {}", module, name);
                                    let symbol =
                                        self.create_import_symbol(&node, name, import_text);
                                    imports.push(symbol);
                                }
                                "aliased_import" => {
                                    // Aliased import: from module import name as alias
                                    if let Some((name, alias)) = self.extract_alias(&child) {
                                        let import_text =
                                            format!("from {} import {} as {}", module, name, alias);
                                        let symbol =
                                            self.create_import_symbol(&node, alias, import_text);
                                        imports.push(symbol);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        imports
    }

    fn extract_lambda(&mut self, node: Node) -> Symbol {
        // Extract lambda parameters
        let parameters_node = node.child_by_field_name("parameters");
        let params = if let Some(parameters_node) = parameters_node {
            self.extract_parameters(&parameters_node)
        } else {
            Vec::new()
        };

        // Extract lambda body (simplified)
        let body_node = node.child_by_field_name("body");
        let body = if let Some(body_node) = body_node {
            self.base.get_node_text(&body_node)
        } else {
            String::new()
        };

        // Create signature: lambda params: body
        let signature = format!("lambda {}: {}", params.join(", "), body);

        // Create name with row number: <lambda:row>
        let start_pos = node.start_position();
        let name = format!("<lambda:{}>", start_pos.row);

        use crate::extractors::base::SymbolOptions;
        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: None, // TODO: Handle parent_id if needed
                metadata: None,
                doc_comment: None,
            },
        )
    }

    /// Extract relationships from Python code
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();

        // Create symbol map for fast lookups by name
        let symbol_map: std::collections::HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.name.clone(), s)).collect();

        // Recursively visit all nodes to extract relationships
        self.visit_node_for_relationships(tree.root_node(), &symbol_map, &mut relationships);

        relationships
    }

    /// Infer types from Python type annotations and assignments
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut type_map = HashMap::new();

        for symbol in symbols {
            // Infer types from Python-specific patterns
            if let Some(ref signature) = symbol.signature {
                if let Some(inferred_type) = self.infer_type_from_signature(signature, &symbol.kind)
                {
                    type_map.insert(symbol.id.clone(), inferred_type);
                }
            }
        }

        type_map
    }

    // Helper methods ported from Miller

    fn extract_argument_list(&self, node: &Node) -> Vec<String> {
        let mut args = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "attribute" => {
                    args.push(self.base.get_node_text(&child));
                }
                "subscript" => {
                    // Handle generic types like Generic[K, V]
                    args.push(self.base.get_node_text(&child));
                }
                "keyword_argument" => {
                    // Handle keyword arguments like metaclass=SingletonMeta
                    let mut child_cursor = child.walk();
                    let children: Vec<_> = child.children(&mut child_cursor).collect();
                    if let (Some(keyword_node), Some(value_node)) =
                        (children.first(), children.last())
                    {
                        if keyword_node.kind() == "identifier"
                            && self.base.get_node_text(keyword_node) == "metaclass"
                        {
                            args.push(format!(
                                "{}={}",
                                self.base.get_node_text(keyword_node),
                                self.base.get_node_text(value_node)
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        args
    }

    fn extract_decorators(&self, node: &Node) -> Vec<String> {
        let mut decorators = Vec::new();
        let mut decorated_node: Option<Node> = None;

        // Check if current node is already a decorated_definition
        if node.kind() == "decorated_definition" {
            decorated_node = Some(*node);
        } else {
            // Walk up to find decorated_definition parent
            let mut current = *node;
            while let Some(parent) = current.parent() {
                if parent.kind() == "decorated_definition" {
                    decorated_node = Some(parent);
                    break;
                }
                current = parent;
            }
        }

        if let Some(decorated_node) = decorated_node {
            let mut cursor = decorated_node.walk();
            for child in decorated_node.children(&mut cursor) {
                if child.kind() == "decorator" {
                    let mut decorator_text = self.base.get_node_text(&child);

                    // Remove @ prefix
                    if decorator_text.starts_with('@') {
                        decorator_text = decorator_text[1..].to_string();
                    }

                    // Extract just the decorator name without parameters
                    // e.g., "lru_cache(maxsize=128)" -> "lru_cache"
                    if let Some(paren_index) = decorator_text.find('(') {
                        decorator_text = decorator_text[..paren_index].to_string();
                    }

                    decorators.push(decorator_text);
                }
            }
        }

        decorators
    }

    fn extract_docstring(&self, node: &Node) -> Option<String> {
        let body_node = node.child_by_field_name("body")?;

        // Look for first string in function/class body (Python docstrings are inside expression_statement nodes)
        let mut cursor = body_node.walk();
        for child in body_node.children(&mut cursor) {
            // Check if this is an expression_statement containing a string (typical for docstrings)
            if child.kind() == "expression_statement" {
                let mut expr_cursor = child.walk();
                for expr_child in child.children(&mut expr_cursor) {
                    if expr_child.kind() == "string" {
                        let mut docstring = self.base.get_node_text(&expr_child);

                        // Remove quotes (single, double, or triple quotes)
                        docstring = Self::strip_string_delimiters(&docstring);
                        return Some(docstring.trim().to_string());
                    }
                }
            }
            // Also handle direct string nodes (just in case)
            else if child.kind() == "string" {
                let mut docstring = self.base.get_node_text(&child);

                // Remove quotes (single, double, or triple quotes)
                docstring = Self::strip_string_delimiters(&docstring);
                return Some(docstring.trim().to_string());
            }
        }

        None
    }

    fn infer_type_from_signature(&self, signature: &str, kind: &SymbolKind) -> Option<String> {
        match kind {
            SymbolKind::Function | SymbolKind::Method => {
                // Extract type hints from function signatures
                if let Some(captures) = regex::Regex::new(r":\s*([^=\s]+)\s*$")
                    .unwrap()
                    .captures(signature)
                {
                    return Some(captures[1].to_string());
                }
            }
            SymbolKind::Variable | SymbolKind::Property => {
                // Extract type from variable annotations
                if let Some(captures) = regex::Regex::new(r":\s*([^=]+)\s*=")
                    .unwrap()
                    .captures(signature)
                {
                    return Some(captures[1].trim().to_string());
                }
            }
            _ => {}
        }

        None
    }

    // Helper methods for function extraction

    fn has_async_keyword(&self, node: &Node) -> bool {
        // Check if any of the node's children is an "async" keyword
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "async" {
                return true;
            }
        }
        false
    }

    fn extract_parameters(&self, parameters_node: &Node) -> Vec<String> {
        let mut params = Vec::new();

        let mut cursor = parameters_node.walk();
        for child in parameters_node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    // Simple parameter name
                    params.push(self.base.get_node_text(&child));
                }
                "parameter" => {
                    // Handle basic parameter - find identifier child
                    let mut param_cursor = child.walk();
                    for param_child in child.children(&mut param_cursor) {
                        if param_child.kind() == "identifier" {
                            params.push(self.base.get_node_text(&param_child));
                            break;
                        }
                    }
                }
                "default_parameter" => {
                    // parameter = default_value
                    let mut parts = Vec::new();
                    let mut param_cursor = child.walk();
                    for param_child in child.children(&mut param_cursor) {
                        if param_child.kind() == "identifier" {
                            parts.push(self.base.get_node_text(&param_child));
                        } else if param_child.kind() == "=" {
                            parts.push("=".to_string());
                        } else if !["(", ")", ","].contains(&param_child.kind()) {
                            parts.push(self.base.get_node_text(&param_child));
                        }
                    }
                    if !parts.is_empty() {
                        params.push(parts.join(""));
                    }
                }
                "typed_parameter" => {
                    // parameter: type
                    let mut name = String::new();
                    let mut type_str = String::new();
                    let mut param_cursor = child.walk();
                    for param_child in child.children(&mut param_cursor) {
                        if param_child.kind() == "identifier" && name.is_empty() {
                            name = self.base.get_node_text(&param_child);
                        } else if param_child.kind() == "type" {
                            type_str = format!(": {}", self.base.get_node_text(&param_child));
                        }
                    }
                    params.push(format!("{}{}", name, type_str));
                }
                "typed_default_parameter" => {
                    // parameter: type = default_value
                    let text = self.base.get_node_text(&child);
                    params.push(text);
                }
                _ => {}
            }
        }

        params
    }

    fn infer_visibility(&self, name: &str) -> crate::extractors::base::Visibility {
        use crate::extractors::base::Visibility;

        if name.starts_with("__") && name.ends_with("__") {
            // Dunder methods are public
            Visibility::Public
        } else if name.starts_with("_") {
            // Single underscore indicates private/protected
            Visibility::Private
        } else {
            Visibility::Public
        }
    }

    fn determine_function_kind(&mut self, node: &Node, name: &str) -> (SymbolKind, Option<String>) {
        // Check if this function is inside a class definition
        let mut current = *node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "class_definition" {
                // This is a method inside a class
                // Extract the class name to create parent_id
                let class_name = if let Some(name_node) = parent.child_by_field_name("name") {
                    self.base.get_node_text(&name_node)
                } else {
                    "Anonymous".to_string()
                };

                // Create parent_id using the same pattern as BaseExtractor
                let start_pos = parent.start_position();
                let parent_id = self.base.generate_id(
                    &class_name,
                    start_pos.row as u32,
                    start_pos.column as u32,
                );

                // Determine method type
                let symbol_kind = if name == "__init__" {
                    SymbolKind::Constructor
                } else {
                    SymbolKind::Method
                };

                return (symbol_kind, Some(parent_id));
            }
            current = parent;
        }

        // Not inside a class, so it's a standalone function
        (SymbolKind::Function, None)
    }

    #[allow(clippy::manual_find)] // Manual loop required for borrow checker
    fn find_type_annotation<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        // Look for type annotation in assignment node children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type" {
                return Some(child);
            }
        }
        None
    }

    // Helper methods for import extraction

    fn extract_single_import(&mut self, node: &Node) -> Option<Symbol> {
        let mut import_text = String::new();
        let mut name = String::new();

        // Check for aliased_import child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "aliased_import" {
                // import module as alias
                if let Some((module_name, alias)) = self.extract_alias(&child) {
                    import_text = format!("import {} as {}", module_name, alias);
                    name = alias; // Use alias as the symbol name
                }
                break;
            } else if child.kind() == "dotted_name" {
                // Simple import: import module
                name = self.base.get_node_text(&child);
                import_text = format!("import {}", name);
                break;
            }
        }

        if !name.is_empty() {
            Some(self.create_import_symbol(node, name, import_text))
        } else {
            None
        }
    }

    fn extract_alias(&self, node: &Node) -> Option<(String, String)> {
        // Extract "name as alias" pattern
        let mut name = String::new();
        let mut alias = String::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "dotted_name" || child.kind() == "identifier" {
                if name.is_empty() {
                    name = self.base.get_node_text(&child);
                } else {
                    // Second name-like node is the alias
                    alias = self.base.get_node_text(&child);
                    break;
                }
            }
        }

        if !name.is_empty() && !alias.is_empty() {
            Some((name, alias))
        } else {
            None
        }
    }

    fn create_import_symbol(&mut self, node: &Node, name: String, signature: String) -> Symbol {
        use crate::extractors::base::SymbolOptions;
        self.base.create_symbol(
            node,
            name,
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: None,
                metadata: None,
                doc_comment: None,
            },
        )
    }

    fn is_inside_enum_class(&self, node: &Node) -> bool {
        // Walk up the parent tree to find a class definition
        let mut current = *node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "class_definition" {
                // Check if this class extends Enum
                if let Some(superclasses_node) = parent.child_by_field_name("superclasses") {
                    let superclasses = self.extract_argument_list(&superclasses_node);
                    // Check if any base class is "Enum"
                    return superclasses
                        .iter()
                        .any(|base| base == "Enum" || base.contains("Enum"));
                }
                // If we found a class but it doesn't extend anything, it's not an enum
                return false;
            }
            current = parent;
        }
        false
    }

    // Relationship extraction methods

    fn visit_node_for_relationships(
        &self,
        node: Node,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_definition" => {
                self.extract_class_relationships(node, symbol_map, relationships);
            }
            "call" => {
                self.extract_call_relationships(node, symbol_map, relationships);
            }
            _ => {}
        }

        // Recursively visit all children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbol_map, relationships);
        }
    }

    fn extract_class_relationships(
        &self,
        node: Node,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // Get class name from the name field
        let name_node = match node.child_by_field_name("name") {
            Some(node) => node,
            None => return,
        };

        let class_name = self.base.get_node_text(&name_node);
        let class_symbol = match symbol_map.get(&class_name) {
            Some(symbol) => symbol,
            None => return,
        };

        // Extract inheritance relationships
        if let Some(superclasses_node) = node.child_by_field_name("superclasses") {
            let bases = self.extract_argument_list(&superclasses_node);

            for base in bases {
                if let Some(base_symbol) = symbol_map.get(&base) {
                    // Determine relationship kind: implements for interfaces/protocols, extends for classes
                    let relationship_kind = if base_symbol.kind == SymbolKind::Interface {
                        RelationshipKind::Implements
                    } else {
                        RelationshipKind::Extends
                    };

                    let relationship = Relationship {
                        id: format!(
                            "{}_{}_{:?}_{}",
                            class_symbol.id,
                            base_symbol.id,
                            relationship_kind,
                            node.start_position().row
                        ),
                        from_symbol_id: class_symbol.id.clone(),
                        to_symbol_id: base_symbol.id.clone(),
                        kind: relationship_kind,
                        file_path: self.base.file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 0.95,
                        metadata: None,
                    };

                    relationships.push(relationship);
                }
            }
        }
    }

    fn extract_call_relationships(
        &self,
        node: Node,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // For a call node, extract the function/method being called
        if let Some(function_node) = node.child_by_field_name("function") {
            let called_method_name = self.extract_method_name_from_call(&function_node);

            if !called_method_name.is_empty() {
                if let Some(called_symbol) = symbol_map.get(&called_method_name) {
                    // Find the enclosing function/method that contains this call
                    if let Some(caller_symbol) = self.find_containing_function(node, symbol_map) {
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
                            confidence: 0.90,
                            metadata: None,
                        };

                        relationships.push(relationship);
                    }
                }
            }
        }
    }

    fn extract_method_name_from_call(&self, function_node: &Node) -> String {
        match function_node.kind() {
            "identifier" => {
                // Simple function call: foo()
                self.base.get_node_text(function_node)
            }
            "attribute" => {
                // Method call: obj.method() or self.db.connect()
                if let Some(attribute_node) = function_node.child_by_field_name("attribute") {
                    self.base.get_node_text(&attribute_node)
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        }
    }

    fn find_containing_function<'a>(
        &self,
        node: Node,
        symbol_map: &std::collections::HashMap<String, &'a Symbol>,
    ) -> Option<&'a Symbol> {
        // Walk up the tree to find the containing function or method
        let mut current = node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "function_definition"
                || parent.kind() == "async_function_definition"
            {
                // Found a function, extract its name
                if let Some(name_node) = parent.child_by_field_name("name") {
                    let function_name = self.base.get_node_text(&name_node);
                    return symbol_map.get(&function_name).copied();
                }
            }
            current = parent;
        }
        None
    }

    /// Helper to strip string delimiters (quotes) from Python strings
    /// Handles triple quotes (""" or '''), double quotes ("), and single quotes (')
    fn strip_string_delimiters(s: &str) -> String {
        // Try delimiters in order: triple quotes first (3 chars), then single quotes (1 char)
        let delimiters = [
            ("\"\"\"", 3),
            ("'''", 3),
            ("\"", 1),
            ("'", 1),
        ];

        for (delimiter, strip_count) in &delimiters {
            if s.starts_with(delimiter) && s.ends_with(delimiter) && s.len() >= strip_count * 2 {
                return s[*strip_count..s.len() - strip_count].to_string();
            }
        }

        // No matching delimiter found, return as-is
        s.to_string()
    }
}
