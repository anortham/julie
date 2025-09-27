// C++ extractor - Port of Miller's comprehensive C++ extraction logic
// Following Miller's proven patterns while making it idiomatic Rust

use crate::extractors::base::{
    BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

/// C++ extractor for extracting symbols and relationships from C++ source code
/// Direct port of Miller's CppExtractor with all advanced C++ features
pub struct CppExtractor {
    base: BaseExtractor,
    processed_nodes: HashSet<String>,
    additional_symbols: Vec<Symbol>,
}

impl CppExtractor {
    pub fn new(file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new("cpp".to_string(), file_path, content),
            processed_nodes: HashSet::new(),
            additional_symbols: Vec::new(),
        }
    }

    /// Extract all symbols from C++ source code - port of Miller's extractSymbols
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.processed_nodes.clear(); // Reset for each extraction
        self.additional_symbols.clear(); // Reset additional symbols

        self.walk_tree(tree.root_node(), &mut symbols, None);

        // Add any additional symbols collected from ERROR nodes
        symbols.extend(self.additional_symbols.clone());

        symbols
    }

    /// Walk the tree recursively - port of Miller's walkTree
    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        // Extract symbol from current node
        if let Some(symbol) = self.extract_symbol(node, parent_id.as_deref()) {
            let current_parent_id = Some(symbol.id.clone());
            symbols.push(symbol);

            // Continue with children using this symbol as parent
            self.walk_children(node, symbols, current_parent_id);
        } else {
            // No symbol extracted, continue with same parent
            self.walk_children(node, symbols, parent_id);
        }
    }

    fn walk_children(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, symbols, parent_id.clone());
        }
    }

    /// Generate unique key for node - port of Miller's getNodeKey
    fn get_node_key(&self, node: Node) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            node.start_position().row,
            node.start_position().column,
            node.end_position().row,
            node.end_position().column,
            node.kind()
        )
    }

    /// Extract symbol from a single node - port of Miller's extractSymbol
    fn extract_symbol(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let node_key = self.get_node_key(node);

        // Track specific node types to prevent duplicates (Miller's logic)
        let should_track = matches!(
            node.kind(),
            "function_declarator" | "function_definition" | "declaration" | "class_specifier"
        );

        if should_track && self.processed_nodes.contains(&node_key) {
            return None;
        }

        let symbol = match node.kind() {
            "namespace_definition" => self.extract_namespace(node, parent_id),
            "using_declaration" | "namespace_alias_definition" => {
                self.extract_using(node, parent_id)
            }
            "class_specifier" => self.extract_class(node, parent_id),
            "struct_specifier" => self.extract_struct(node, parent_id),
            "union_specifier" => self.extract_union(node, parent_id),
            "enum_specifier" => self.extract_enum(node, parent_id),
            "enumerator" => self.extract_enum_member(node, parent_id),
            "function_definition" => self.extract_function(node, parent_id),
            "function_declarator" => {
                // Only extract standalone function declarators (Miller's logic)
                if node.parent().map(|p| p.kind()) != Some("function_definition") {
                    self.extract_function(node, parent_id)
                } else {
                    None
                }
            }
            "declaration" => self.extract_declaration(node, parent_id),
            "friend_declaration" => self.extract_friend_declaration(node, parent_id),
            "template_declaration" => self.extract_template(node, parent_id),
            "field_declaration" => self.extract_field(node, parent_id),
            "ERROR" => self.extract_from_error_node(node, parent_id),
            _ => None,
        };

        // Mark node as processed if we successfully extracted a symbol and should track
        if symbol.is_some() && should_track {
            self.processed_nodes.insert(node_key);
        }

        symbol
    }

    /// Extract namespace declaration - port of Miller's extractNamespace
    fn extract_namespace(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "namespace_identifier")?;

        let name = self.base.get_node_text(&name_node);
        let signature = format!("namespace {}", name);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Namespace,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    /// Extract using declarations - port of Miller's extractUsing
    fn extract_using(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut name = String::new();
        let mut signature = String::new();

        if node.kind() == "using_declaration" {
            let mut cursor = node.walk();
            let qualified_id_node = node
                .children(&mut cursor)
                .find(|c| c.kind() == "qualified_identifier" || c.kind() == "identifier")?;

            let full_path = self.base.get_node_text(&qualified_id_node);

            // Check if it's "using namespace"
            let is_namespace = node
                .children(&mut node.walk())
                .any(|c| c.kind() == "namespace");

            if is_namespace {
                name = full_path.clone();
                signature = format!("using namespace {}", full_path);
            } else {
                // Extract the last part for the symbol name
                let parts: Vec<&str> = full_path.split("::").collect();
                name = (*parts.last().unwrap_or(&full_path.as_str())).to_string();
                signature = format!("using {}", full_path);
            }
        } else if node.kind() == "namespace_alias_definition" {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();

            let alias_node = children
                .iter()
                .find(|c| c.kind() == "namespace_identifier")?;
            let target_node = children.iter().find(|c| {
                c.kind() == "nested_namespace_specifier" || c.kind() == "qualified_identifier"
            })?;

            name = self.base.get_node_text(alias_node);
            let target = self.base.get_node_text(target_node);
            signature = format!("namespace {} = {}", name, target);
        }

        if name.is_empty() {
            return None;
        }

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    /// Extract class declaration - port of Miller's extractClass
    fn extract_class(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "type_identifier" || c.kind() == "template_type")?;

        let (name, is_specialization) = if name_node.kind() == "template_type" {
            // For template specializations like Vector<bool>, extract just the base name
            // The template_type node contains type_identifier + template_argument_list
            let type_id = name_node
                .children(&mut name_node.walk())
                .find(|c| c.kind() == "type_identifier")
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| self.base.get_node_text(&name_node));
            (type_id, true)
        } else {
            (self.base.get_node_text(&name_node), false)
        };

        let mut signature = if is_specialization {
            // For template specializations, include the full template type in signature
            let full_name = self.base.get_node_text(&name_node);
            format!("class {}", full_name)
        } else {
            format!("class {}", name)
        };

        // Handle template parameters (Miller's logic)
        if let Some(template_params) = self.extract_template_parameters(node.parent()) {
            signature = format!("{}\n{}", template_params, signature);
        }

        // Handle inheritance
        if let Some(base_clause) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "base_class_clause")
        {
            let bases = self.extract_base_classes(base_clause);
            if !bases.is_empty() {
                signature.push_str(&format!(" : {}", bases.join(", ")));
            }
        }

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    /// Extract struct declaration - port of Miller's extractStruct
    fn extract_struct(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "type_identifier")?;

        let name = self.base.get_node_text(&name_node);
        let mut signature = format!("struct {}", name);

        // Handle template parameters
        if let Some(template_params) = self.extract_template_parameters(node.parent()) {
            signature = format!("{}\n{}", template_params, signature);
        }

        // Handle inheritance (structs can inherit too)
        if let Some(base_clause) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "base_class_clause")
        {
            let bases = self.extract_base_classes(base_clause);
            if !bases.is_empty() {
                signature.push_str(&format!(" : {}", bases.join(", ")));
            }
        }

        // Check for alignas qualifier
        let mut children_cursor = node.walk();
        let alignas_node = node
            .children(&mut children_cursor)
            .find(|c| c.kind() == "alignas_qualifier");
        if let Some(alignas) = alignas_node {
            let alignas_text = self.base.get_node_text(&alignas);
            signature = format!("{} {}", alignas_text, signature);
        }

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Struct,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    /// Extract union declaration - port of Miller's extractUnion
    fn extract_union(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "type_identifier");

        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            // Handle anonymous unions
            format!("<anonymous_union_{}>", node.start_position().row)
        };

        let signature = if name_node.is_some() {
            format!("union {}", name)
        } else {
            "union".to_string()
        };

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Union,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    /// Extract enum declaration - port of Miller's extractEnum
    fn extract_enum(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "type_identifier")?;

        let name = self.base.get_node_text(&name_node);

        // Check if it's a scoped enum (enum class)
        let is_scoped = node.children(&mut node.walk()).any(|c| c.kind() == "class");

        let mut signature = if is_scoped {
            format!("enum class {}", name)
        } else {
            format!("enum {}", name)
        };

        // Check for underlying type
        let children: Vec<Node> = node.children(&mut node.walk()).collect();
        if let Some(colon_pos) = children.iter().position(|c| c.kind() == ":") {
            if colon_pos + 1 < children.len() {
                let type_node = &children[colon_pos + 1];
                if type_node.kind() == "primitive_type" || type_node.kind() == "type_identifier" {
                    signature.push_str(&format!(" : {}", self.base.get_node_text(type_node)));
                }
            }
        }

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Enum,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    /// Extract enum member - port of Miller's extractEnumMember
    fn extract_enum_member(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let mut signature = name.clone();

        // Check for initializer
        let children: Vec<Node> = node.children(&mut node.walk()).collect();
        if let Some(equals_pos) = children.iter().position(|c| c.kind() == "=") {
            if equals_pos + 1 < children.len() {
                let value_nodes = &children[equals_pos + 1..];
                let value: String = value_nodes
                    .iter()
                    .map(|n| self.base.get_node_text(n))
                    .collect::<Vec<_>>()
                    .join("")
                    .trim()
                    .to_string();
                if !value.is_empty() {
                    signature.push_str(&format!(" = {}", value));
                }
            }
        }

        // Determine if this is from an anonymous enum
        let enum_parent = self.find_parent_enum(node);
        let is_anonymous_enum = enum_parent
            .and_then(|parent| {
                parent
                    .children(&mut parent.walk())
                    .find(|c| c.kind() == "type_identifier")
            })
            .is_none();

        let symbol_kind = if is_anonymous_enum {
            SymbolKind::Constant
        } else {
            SymbolKind::EnumMember
        };

        Some(self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    // Stub implementations for remaining methods (to be implemented incrementally)

    fn extract_function(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Handle both function_definition and function_declarator - port of Miller's extractFunction
        let mut func_node = node;
        if node.kind() == "function_definition" {
            // Look for function_declarator or reference_declarator
            let declarator = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "function_declarator" || c.kind() == "reference_declarator");
            if let Some(declarator) = declarator {
                func_node = declarator;
            }
        }

        let name_node = self.extract_function_name(func_node)?;
        let name = self.base.get_node_text(&name_node);

        // Skip if it's a field_identifier (should be handled as method)
        if name_node.kind() == "field_identifier" {
            return self.extract_method(node, func_node, &name, parent_id);
        }

        // Check if this is a constructor or destructor
        let is_constructor = self.is_constructor(&name, node);
        let is_destructor = name.starts_with('~');
        let is_operator = name.starts_with("operator");

        let kind = if is_constructor {
            SymbolKind::Constructor
        } else if is_destructor {
            SymbolKind::Destructor
        } else if is_operator {
            SymbolKind::Operator
        } else {
            SymbolKind::Function
        };

        // Build signature from Miller's proven approach
        let modifiers = self.extract_function_modifiers(node);
        let return_type = if is_constructor || is_destructor {
            String::new()
        } else {
            self.extract_basic_return_type(node)
        };
        let trailing_return_type = self.extract_trailing_return_type(node);
        let parameters = self.extract_function_parameters(func_node);
        let const_qualifier = self.extract_const_qualifier(func_node);
        let noexcept_spec = self.extract_noexcept_specifier(func_node);

        let mut signature = String::new();

        // Add template parameters if present
        if let Some(template_params) = self.extract_template_parameters(node.parent()) {
            signature.push_str(&template_params);
            signature.push('\n');
        }

        // Add modifiers
        if !modifiers.is_empty() {
            signature.push_str(&modifiers.join(" "));
            signature.push(' ');
        }

        // Add return type
        if !return_type.is_empty() {
            signature.push_str(&return_type);
            signature.push(' ');
        }

        // Add function name and parameters
        signature.push_str(&name);
        signature.push_str(&parameters);

        // Add const qualifier
        if const_qualifier {
            signature.push_str(" const");
        }

        // Add noexcept
        if !noexcept_spec.is_empty() {
            signature.push(' ');
            signature.push_str(&noexcept_spec);
        }

        // Add trailing return type
        if !trailing_return_type.is_empty() {
            if trailing_return_type.starts_with("->") {
                // Already includes the arrow
                signature.push(' ');
                signature.push_str(&trailing_return_type);
            } else {
                signature.push_str(" -> ");
                signature.push_str(&trailing_return_type);
            }
        }

        // Check for = delete, = default (for function_definition nodes)
        if node.kind() == "function_definition" {
            let children: Vec<Node> = node.children(&mut node.walk()).collect();
            for child in &children {
                if child.kind() == "delete_method_clause" {
                    signature.push_str(" = delete");
                    break;
                } else if child.kind() == "default_method_clause" {
                    signature.push_str(" = default");
                    break;
                }
            }
        }

        Some(self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public), // TODO: Extract actual visibility
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_declaration(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port of Miller's extractDeclaration logic

        // Check if this is a friend declaration first
        let node_text = self.base.get_node_text(&node);
        let has_friend = node
            .children(&mut node.walk())
            .any(|c| c.kind() == "friend" || self.base.get_node_text(&c) == "friend");

        // Also check the node's own text for friend keyword
        let has_friend_text = node_text.starts_with("friend") || node_text.contains(" friend ");

        if has_friend || has_friend_text {
            return self.extract_friend_declaration(node, parent_id);
        }

        // Check if this is a conversion operator (e.g., operator double())
        let operator_cast = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "operator_cast");
        if operator_cast.is_some() {
            return self.extract_conversion_operator(node, parent_id);
        }

        // Check if this is a function declaration
        let func_declarator = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "function_declarator");
        if let Some(func_declarator) = func_declarator {
            // Check if this is a destructor by looking for destructor_name
            let destructor_name = func_declarator
                .children(&mut func_declarator.walk())
                .find(|c| c.kind() == "destructor_name");
            if destructor_name.is_some() {
                return self.extract_destructor_from_declaration(node, func_declarator, parent_id);
            }

            // Check if this is a constructor (function name matches class name)
            let name_node = self.extract_function_name(func_declarator)?;
            let name = self.base.get_node_text(&name_node);

            if self.is_constructor(&name, node) {
                return self.extract_constructor_from_declaration(node, func_declarator, parent_id);
            }

            // This is a function declaration, treat it as a function
            return self.extract_function(node, parent_id);
        }

        // Handle variable declarations
        let declarators: Vec<Node> = node
            .children(&mut node.walk())
            .filter(|c| c.kind() == "init_declarator")
            .collect();

        // Check for direct identifier declarations (e.g., extern variables)
        if declarators.is_empty() {
            let identifier_node = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "identifier")?;

            let name = self.base.get_node_text(&identifier_node);

            // Get storage class and type specifiers
            let storage_class = self.extract_storage_class(node);
            let type_specifiers = self.extract_type_specifiers(node);
            let is_constant = self.is_constant_declaration(&storage_class, &type_specifiers);

            // Check if this is a static member variable inside a class
            let is_static_member = self.is_static_member_variable(node, &storage_class);

            let kind = if is_constant || is_static_member {
                SymbolKind::Constant
            } else {
                SymbolKind::Variable
            };

            // Build signature - for direct declarations
            let signature = self.build_direct_variable_signature(node, &name);
            let visibility = self.extract_visibility_from_node(node);

            return Some(self.base.create_symbol(
                &node,
                name,
                kind,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.map(String::from),
                    metadata: None,
                    doc_comment: None,
                },
            ));
        }

        // For now, handle the first declarator
        let declarator = declarators.first()?;
        let name_node = self.extract_declarator_name(*declarator)?;
        let name = self.base.get_node_text(&name_node);

        // Get storage class and type specifiers
        let storage_class = self.extract_storage_class(node);
        let type_specifiers = self.extract_type_specifiers(node);
        let is_constant = self.is_constant_declaration(&storage_class, &type_specifiers);

        let kind = if is_constant {
            SymbolKind::Constant
        } else {
            SymbolKind::Variable
        };

        // Build signature
        let signature = self.build_variable_signature(node, &name);
        let visibility = self.extract_visibility_from_node(node);

        Some(self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_template(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract template declaration - port of Miller's extractTemplate
        let mut cursor = node.walk();
        let declaration = node.children(&mut cursor).find(|c| {
            matches!(
                c.kind(),
                "class_specifier" | "struct_specifier" | "function_definition" | "declaration"
            )
        })?;

        // Extract the symbol from the inner declaration and add template info
        if let Some(mut symbol) = self.extract_symbol(declaration, parent_id) {
            // Add template parameters to signature
            if let Some(template_params) = self.extract_template_parameters(Some(node)) {
                if let Some(ref mut sig) = symbol.signature {
                    *sig = format!(
                        "{}
{}",
                        template_params, sig
                    );
                } else {
                    symbol.signature = Some(template_params);
                }
            }
            Some(symbol)
        } else {
            None
        }
    }

    fn extract_field(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract field declaration - port of Miller's extractField
        let declarators: Vec<Node> = node
            .children(&mut node.walk())
            .filter(|c| matches!(c.kind(), "field_declarator" | "init_declarator"))
            .collect();

        if declarators.is_empty() {
            // Check for field_identifier directly (for simple field declarations like static const members)
            let field_id = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "field_identifier");

            if let Some(field_node) = field_id {
                let name = self.base.get_node_text(&field_node);

                // Get storage class and type specifiers
                let storage_class = self.extract_storage_class(node);
                let type_specifiers = self.extract_type_specifiers(node);
                let is_constant = self.is_constant_declaration(&storage_class, &type_specifiers);

                // Check if this is a static member variable inside a class
                let is_static_member = self.is_static_member_variable(node, &storage_class);

                let kind = if is_constant || is_static_member {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Field
                };

                // Build signature
                let signature = self.build_field_signature(node, &name);
                let visibility = self.extract_field_visibility(node);

                return Some(self.base.create_symbol(
                    &node,
                    name,
                    kind,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(visibility),
                        parent_id: parent_id.map(String::from),
                        metadata: None,
                        doc_comment: None,
                    },
                ));
            }

            return None;
        }

        // For now, handle the first declarator
        let declarator = declarators.first()?;
        let name_node = self.extract_field_name(*declarator)?;
        let name = self.base.get_node_text(&name_node);

        // Get storage class and type specifiers
        let storage_class = self.extract_storage_class(node);
        let type_specifiers = self.extract_type_specifiers(node);
        let is_constant = self.is_constant_declaration(&storage_class, &type_specifiers);

        // Check if this is a static member variable inside a class
        let is_static_member = self.is_static_member_variable(node, &storage_class);

        let kind = if is_constant || is_static_member {
            SymbolKind::Constant
        } else {
            SymbolKind::Field
        };

        // Build signature
        let signature = self.build_field_signature(node, &name);
        let visibility = self.extract_field_visibility(node);

        Some(self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_friend_declaration(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract friend declaration - port of Miller's extractFriendDeclaration
        // Handles both friend functions AND friend operators

        let mut cursor = node.walk();

        // Look for the inner declaration node
        let inner_declaration = node
            .children(&mut cursor)
            .find(|c| c.kind() == "declaration")?;

        // Look for function_declarator in the declaration
        let function_declarator = self.find_function_declarator_in_node(inner_declaration)?;

        // Extract name - handle both operator_name and regular identifier
        let (name, symbol_kind) = if let Some(operator_name) = function_declarator
            .children(&mut function_declarator.walk())
            .find(|c| c.kind() == "operator_name")
        {
            // This is a friend operator (e.g., operator+)
            (
                self.base.get_node_text(&operator_name),
                SymbolKind::Operator,
            )
        } else if let Some(identifier) = function_declarator
            .children(&mut function_declarator.walk())
            .find(|c| c.kind() == "identifier")
        {
            // This is a friend function (e.g., dot)
            (self.base.get_node_text(&identifier), SymbolKind::Function)
        } else {
            return None;
        };

        // Build friend signature
        let return_type = self.extract_basic_return_type(inner_declaration);
        let parameters = self.extract_function_parameters(function_declarator);

        let signature = format!("friend {} {}{}", return_type, name, parameters)
            .trim()
            .to_string();

        // Create the symbol with appropriate kind
        let symbol = self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        );

        Some(symbol)
    }

    fn find_function_declarator_in_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        // Recursively search for function_declarator in the node tree
        if node.kind() == "function_declarator" {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(result) = self.find_function_declarator_in_node(child) {
                return Some(result);
            }
        }

        None
    }

    fn extract_from_error_node(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract from ERROR node - handle malformed code gracefully
        // Try to reconstruct class/struct/etc from token fragments

        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Look for class/struct patterns: "class" + type_identifier
        for i in 0..children.len().saturating_sub(1) {
            let current = children[i];
            let next = children[i + 1];

            if current.kind() == "class" && next.kind() == "type_identifier" {
                let name = self.base.get_node_text(&next);
                let signature = format!("class {}", name);

                return Some(self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Class,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(String::from),
                        metadata: None,
                        doc_comment: None,
                    },
                ));
            } else if current.kind() == "struct" && next.kind() == "type_identifier" {
                let name = self.base.get_node_text(&next);
                let signature = format!("struct {}", name);

                return Some(self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Struct,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(String::from),
                        metadata: None,
                        doc_comment: None,
                    },
                ));
            }
        }

        // No reconstructible symbol found
        None
    }

    // Helper methods

    fn extract_template_parameters(&self, template_node: Option<Node>) -> Option<String> {
        let mut current = template_node;
        while let Some(node) = current {
            if node.kind() == "template_declaration" {
                let mut cursor = node.walk();
                let param_list = node
                    .children(&mut cursor)
                    .find(|c| c.kind() == "template_parameter_list");
                if let Some(param_list) = param_list {
                    return Some(format!("template{}", self.base.get_node_text(&param_list)));
                }
            }
            current = node.parent();
        }
        None
    }

    fn extract_base_classes(&self, base_clause: Node) -> Vec<String> {
        let mut bases = Vec::new();
        let mut cursor = base_clause.walk();
        let children: Vec<Node> = base_clause.children(&mut cursor).collect();

        let mut i = 0;
        while i < children.len() {
            let child = children[i];

            if child.kind() == ":" || child.kind() == "," {
                i += 1;
                continue;
            }

            // For inheritance like ": public Shape", extract access + class name
            if child.kind() == "access_specifier" {
                let access = self.base.get_node_text(&child);
                // Look for the next child which should be the class name
                i += 1;
                if i < children.len() {
                    let class_node = children[i];
                    if matches!(
                        class_node.kind(),
                        "type_identifier" | "qualified_identifier" | "template_type"
                    ) {
                        let class_name = self.base.get_node_text(&class_node);
                        bases.push(format!("{} {}", access, class_name));
                    }
                }
            } else if matches!(
                child.kind(),
                "type_identifier" | "qualified_identifier" | "template_type"
            ) {
                // Class name without explicit access specifier
                let class_name = self.base.get_node_text(&child);
                bases.push(class_name);
            }

            i += 1;
        }

        bases
    }

    fn find_parent_enum<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.kind() == "enum_specifier" {
                return Some(parent);
            }
            current = parent.parent();
        }
        None
    }

    /// Extract relationships from C++ code - port of Miller's extractRelationships
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        let mut symbol_map = HashMap::new();

        // Create a lookup map for symbols by name
        for symbol in symbols {
            symbol_map.insert(symbol.name.clone(), symbol);
        }

        // Walk the tree looking for inheritance relationships
        self.walk_tree_for_relationships(tree.root_node(), &symbol_map, &mut relationships);

        relationships
    }

    fn walk_tree_for_relationships(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        if matches!(node.kind(), "class_specifier" | "struct_specifier") {
            let inheritance = self.extract_inheritance_from_class(node, symbol_map);
            relationships.extend(inheritance);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_relationships(child, symbol_map, relationships);
        }
    }

    fn extract_inheritance_from_class(
        &self,
        class_node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Vec<Relationship> {
        let mut relationships = Vec::new();

        // Get the class name
        let mut cursor = class_node.walk();
        let name_node = class_node
            .children(&mut cursor)
            .find(|c| c.kind() == "type_identifier");

        let Some(name_node) = name_node else {
            return relationships;
        };

        let class_name = self.base.get_node_text(&name_node);
        let Some(derived_symbol) = symbol_map.get(&class_name) else {
            return relationships;
        };

        // Look for base class clause
        let base_clause = class_node
            .children(&mut class_node.walk())
            .find(|c| c.kind() == "base_class_clause");

        let Some(base_clause) = base_clause else {
            return relationships;
        };

        // Extract base classes
        let base_classes = self.extract_base_classes(base_clause);
        for base_class in base_classes {
            // Clean base class name (remove access specifiers)
            let clean_base_name = base_class
                .strip_prefix("public ")
                .or_else(|| base_class.strip_prefix("private "))
                .or_else(|| base_class.strip_prefix("protected "))
                .unwrap_or(&base_class);

            if let Some(base_symbol) = symbol_map.get(clean_base_name) {
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        derived_symbol.id,
                        base_symbol.id,
                        RelationshipKind::Extends,
                        class_node.start_position().row
                    ),
                    from_symbol_id: derived_symbol.id.clone(),
                    to_symbol_id: base_symbol.id.clone(),
                    kind: RelationshipKind::Extends,
                    file_path: self.base.file_path.clone(),
                    line_number: (class_node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: None,
                });
            }
        }

        relationships
    }

    /// Infer types from C++ type annotations and declarations
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut type_map = HashMap::new();

        for symbol in symbols {
            if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                // Extract return type from function signature
                if let Some(return_type) = self.infer_function_return_type(symbol) {
                    type_map.insert(symbol.id.clone(), return_type);
                }
            } else if matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Field) {
                // Extract variable type from signature
                if let Some(variable_type) = self.infer_variable_type(symbol) {
                    type_map.insert(symbol.id.clone(), variable_type);
                }
            }
        }

        type_map
    }

    fn infer_function_return_type(&self, symbol: &Symbol) -> Option<String> {
        let signature = symbol.signature.as_ref()?;

        // Remove template parameters if present
        let signature = if let Some(template_match) = signature.find("template<") {
            if let Some(newline_pos) = signature[template_match..].find('\n') {
                &signature[template_match + newline_pos + 1..]
            } else {
                signature
            }
        } else {
            signature
        };

        // Skip constructors and destructors (no return type)
        if matches!(
            symbol.kind,
            SymbolKind::Constructor | SymbolKind::Destructor
        ) {
            return None;
        }

        // Pattern: "returnType functionName(params)"
        let function_pattern = regex::Regex::new(
            r"^(?:(?:virtual|static|inline|friend)\s+)*(.+?)\s+(\w+|operator\w*|~\w+)\s*\(",
        )
        .ok()?;
        if let Some(captures) = function_pattern.captures(signature) {
            let return_type = captures.get(1)?.as_str().trim();
            return Some(return_type.to_string());
        }

        // Pattern: "auto functionName(params) -> returnType"
        let auto_pattern =
            regex::Regex::new(r"auto\s+(\w+)\s*\([^)]*\)\s*->\s*(.+?)(?:\s|$)").ok()?;
        if let Some(captures) = auto_pattern.captures(signature) {
            return Some(captures.get(2)?.as_str().trim().to_string());
        }

        None
    }

    fn infer_variable_type(&self, symbol: &Symbol) -> Option<String> {
        let signature = symbol.signature.as_ref()?;

        // Pattern: "storageClass? typeSpec variableName initializer?"
        let variable_pattern = regex::Regex::new(
            r"^(?:(?:static|extern|const|constexpr|mutable)\s+)*(.+?)\s+(\w+)(?:\s*=.*)?$",
        )
        .ok()?;
        if let Some(captures) = variable_pattern.captures(signature) {
            return Some(captures.get(1)?.as_str().trim().to_string());
        }

        None
    }

    // Helper methods for function extraction - ported from Miller's proven logic

    fn extract_function_name<'a>(&self, func_node: Node<'a>) -> Option<Node<'a>> {
        // Handle different types of function names - Miller's extractFunctionName

        // operator_name (operator overloading)
        if let Some(operator_node) = func_node
            .children(&mut func_node.walk())
            .find(|c| c.kind() == "operator_name")
        {
            return Some(operator_node);
        }

        // destructor_name
        if let Some(destructor_node) = func_node
            .children(&mut func_node.walk())
            .find(|c| c.kind() == "destructor_name")
        {
            return Some(destructor_node);
        }

        // field_identifier (methods)
        if let Some(field_id_node) = func_node
            .children(&mut func_node.walk())
            .find(|c| c.kind() == "field_identifier")
        {
            return Some(field_id_node);
        }

        // identifier (regular functions)
        if let Some(identifier_node) = func_node
            .children(&mut func_node.walk())
            .find(|c| c.kind() == "identifier")
        {
            return Some(identifier_node);
        }

        // qualified_identifier (e.g., ClassName::method)
        if let Some(qualified_node) = func_node
            .children(&mut func_node.walk())
            .find(|c| c.kind() == "qualified_identifier")
        {
            return Some(qualified_node);
        }

        None
    }

    fn extract_method(
        &mut self,
        node: Node,
        func_node: Node,
        name: &str,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Port of Miller's extractMethod logic
        let is_constructor = self.is_constructor(name, node);
        let is_destructor = name.starts_with('~');
        let is_operator = name.starts_with("operator");

        let kind = if is_constructor {
            SymbolKind::Constructor
        } else if is_destructor {
            SymbolKind::Destructor
        } else if is_operator {
            SymbolKind::Operator
        } else {
            SymbolKind::Method
        };

        // For methods in classes, look for modifiers in the parent declaration node as well
        let modifiers = self.extract_method_modifiers(node, func_node);
        let return_type = if is_constructor || is_destructor {
            String::new()
        } else {
            self.extract_basic_return_type(node)
        };
        let parameters = self.extract_function_parameters(func_node);
        let const_qualifier = self.extract_const_qualifier(func_node);

        let mut signature = String::new();
        if !modifiers.is_empty() {
            signature.push_str(&modifiers.join(" "));
            signature.push(' ');
        }
        if !return_type.is_empty() {
            signature.push_str(&return_type);
            signature.push(' ');
        }
        signature.push_str(name);
        signature.push_str(&parameters);
        if const_qualifier {
            signature.push_str(" const");
        }

        Some(self.base.create_symbol(
            &node,
            name.to_string(),
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public), // TODO: Extract actual visibility
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn is_constructor(&self, name: &str, node: Node) -> bool {
        // Check if this function name matches a containing class name
        let mut current = Some(node);
        while let Some(parent) = current {
            if matches!(parent.kind(), "class_specifier" | "struct_specifier") {
                if let Some(class_name_node) = parent
                    .children(&mut parent.walk())
                    .find(|c| c.kind() == "type_identifier")
                {
                    let class_name = self.base.get_node_text(&class_name_node);
                    if class_name == name {
                        return true;
                    }
                }
            }
            current = parent.parent();
        }
        false
    }

    fn extract_function_modifiers(&self, node: Node) -> Vec<String> {
        let mut modifiers = Vec::new();
        let modifier_types = ["virtual", "static", "explicit", "friend", "inline"];

        // Look at the node itself and all its children recursively
        self.collect_modifiers_recursive(node, &mut modifiers, &modifier_types);

        modifiers
    }

    fn collect_modifiers_recursive(
        &self,
        node: Node,
        modifiers: &mut Vec<String>,
        modifier_types: &[&str],
    ) {
        for child in node.children(&mut node.walk()) {
            if modifier_types.contains(&child.kind()) {
                let modifier = self.base.get_node_text(&child);
                if !modifiers.contains(&modifier) {
                    modifiers.push(modifier);
                }
            } else if child.kind() == "storage_class_specifier" {
                let modifier = self.base.get_node_text(&child);
                if !modifiers.contains(&modifier) {
                    modifiers.push(modifier);
                }
            }
            // Recursively check children but don't go too deep to avoid function bodies
            if !matches!(child.kind(), "compound_statement" | "function_body") {
                self.collect_modifiers_recursive(child, modifiers, modifier_types);
            }
        }
    }

    fn extract_basic_return_type(&self, node: Node) -> String {
        // Look for type specifiers before the function declarator
        for child in node.children(&mut node.walk()) {
            if matches!(
                child.kind(),
                "primitive_type"
                    | "type_identifier"
                    | "qualified_identifier"
                    | "auto"
                    | "placeholder_type_specifier"
            ) {
                return self.base.get_node_text(&child);
            }
        }
        String::new()
    }

    fn extract_trailing_return_type(&self, node: Node) -> String {
        // Look for trailing return type (auto functions)
        // The trailing return type is inside the function_declarator node

        // First, find the function_declarator child
        let func_declarator = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "function_declarator");

        if let Some(declarator) = func_declarator {
            // Look inside the function_declarator for "->" followed by type
            let children: Vec<Node> = declarator.children(&mut declarator.walk()).collect();

            for (i, child) in children.iter().enumerate() {
                // Look for "->" token or "trailing_return_type" node
                if child.kind() == "->" && i + 1 < children.len() {
                    return self.base.get_node_text(&children[i + 1]);
                } else if child.kind() == "trailing_return_type" {
                    // Extract the type from trailing_return_type node
                    return child
                        .children(&mut child.walk())
                        .find(|c| {
                            matches!(
                                c.kind(),
                                "primitive_type" | "type_identifier" | "qualified_identifier"
                            )
                        })
                        .map(|type_node| self.base.get_node_text(&type_node))
                        .unwrap_or_else(|| self.base.get_node_text(child));
                }
            }
        }

        String::new()
    }

    fn extract_function_parameters(&self, func_node: Node) -> String {
        if let Some(param_list) = func_node
            .children(&mut func_node.walk())
            .find(|c| c.kind() == "parameter_list")
        {
            self.base.get_node_text(&param_list)
        } else {
            "()".to_string()
        }
    }

    fn extract_const_qualifier(&self, func_node: Node) -> bool {
        func_node
            .children(&mut func_node.walk())
            .any(|c| c.kind() == "type_qualifier" && self.base.get_node_text(&c) == "const")
    }

    fn extract_noexcept_specifier(&self, func_node: Node) -> String {
        for child in func_node.children(&mut func_node.walk()) {
            if child.kind() == "noexcept" {
                return self.base.get_node_text(&child);
            }
        }
        String::new()
    }

    // Additional helper methods for declaration extraction

    fn extract_conversion_operator(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract conversion operator like "operator double()"

        // Find the operator_cast node
        let operator_cast = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "operator_cast")?;

        // Extract the target type from operator_cast
        // Structure: operator_cast -> operator + primitive_type/type_identifier + abstract_function_declarator
        let mut operator_name = "operator".to_string();

        let mut cursor = operator_cast.walk();
        for child in operator_cast.children(&mut cursor) {
            if matches!(
                child.kind(),
                "primitive_type" | "type_identifier" | "qualified_identifier"
            ) {
                let target_type = self.base.get_node_text(&child);
                operator_name.push(' ');
                operator_name.push_str(&target_type);
                break;
            }
        }

        let signature = self.base.get_node_text(&node);

        Some(self.base.create_symbol(
            &node,
            operator_name,
            SymbolKind::Operator,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_destructor_from_declaration(
        &mut self,
        node: Node,
        _func_declarator: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract destructor from declaration like "virtual ~MyClass();"
        let signature = self.base.get_node_text(&node);
        let name_start = signature.find('~')?;
        let name_end = signature[name_start..].find('(').map(|i| name_start + i)?;
        let name = signature[name_start..name_end].to_string();

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Destructor,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_storage_class(&self, node: Node) -> Vec<String> {
        let mut storage_classes = Vec::new();
        let storage_types = ["static", "extern", "mutable", "thread_local"];

        for child in node.children(&mut node.walk()) {
            if storage_types.contains(&child.kind()) {
                storage_classes.push(self.base.get_node_text(&child));
            } else if child.kind() == "storage_class_specifier" {
                storage_classes.push(self.base.get_node_text(&child));
            }
        }

        storage_classes
    }

    fn extract_type_specifiers(&self, node: Node) -> Vec<String> {
        let mut type_specifiers = Vec::new();
        let type_kinds = ["const", "constexpr", "volatile"];

        for child in node.children(&mut node.walk()) {
            if type_kinds.contains(&child.kind()) {
                type_specifiers.push(self.base.get_node_text(&child));
            } else if child.kind() == "type_qualifier" {
                type_specifiers.push(self.base.get_node_text(&child));
            }
        }

        type_specifiers
    }

    fn is_constant_declaration(
        &self,
        storage_class: &[String],
        type_specifiers: &[String],
    ) -> bool {
        type_specifiers
            .iter()
            .any(|spec| spec == "const" || spec == "constexpr")
            || storage_class.iter().any(|sc| sc == "constexpr")
    }

    fn extract_declarator_name<'a>(&self, declarator: Node<'a>) -> Option<Node<'a>> {
        // Look for identifier in declarator
        declarator
            .children(&mut declarator.walk())
            .find(|c| c.kind() == "identifier")
    }

    fn build_direct_variable_signature(&self, node: Node, name: &str) -> String {
        let mut signature = String::new();

        // Add storage class
        let storage_class = self.extract_storage_class(node);
        if !storage_class.is_empty() {
            signature.push_str(&storage_class.join(" "));
            signature.push(' ');
        }

        // Add type specifiers
        let type_specifiers = self.extract_type_specifiers(node);
        if !type_specifiers.is_empty() {
            signature.push_str(&type_specifiers.join(" "));
            signature.push(' ');
        }

        // Add type
        for child in node.children(&mut node.walk()) {
            if matches!(
                child.kind(),
                "primitive_type" | "type_identifier" | "qualified_identifier"
            ) {
                signature.push_str(&self.base.get_node_text(&child));
                signature.push(' ');
                break;
            }
        }

        signature.push_str(name);
        signature
    }

    fn build_variable_signature(&self, node: Node, name: &str) -> String {
        let mut signature = String::new();

        // Add storage class and type specifiers
        let storage_class = self.extract_storage_class(node);
        let type_specifiers = self.extract_type_specifiers(node);

        let mut parts = Vec::new();
        parts.extend(storage_class);
        parts.extend(type_specifiers);

        // Add type from node
        for child in node.children(&mut node.walk()) {
            if matches!(
                child.kind(),
                "primitive_type" | "type_identifier" | "qualified_identifier"
            ) {
                parts.push(self.base.get_node_text(&child));
                break;
            }
        }

        if !parts.is_empty() {
            signature.push_str(&parts.join(" "));
            signature.push(' ');
        }

        signature.push_str(name);
        signature
    }

    fn extract_visibility_from_node(&self, node: Node) -> Visibility {
        // For now, return Public - TODO: Implement proper visibility extraction
        let _ = node; // Suppress unused warning
        Visibility::Public
    }

    fn extract_field_name<'a>(&self, field_node: Node<'a>) -> Option<Node<'a>> {
        // Look for identifier in field declarator
        field_node
            .children(&mut field_node.walk())
            .find(|c| c.kind() == "field_identifier" || c.kind() == "identifier")
    }

    fn build_field_signature(&self, node: Node, name: &str) -> String {
        let mut signature = String::new();

        // Add storage class and type specifiers
        let storage_class = self.extract_storage_class(node);
        let type_specifiers = self.extract_type_specifiers(node);

        let mut parts = Vec::new();
        parts.extend(storage_class);
        parts.extend(type_specifiers);

        // Add type from node
        for child in node.children(&mut node.walk()) {
            if matches!(
                child.kind(),
                "primitive_type" | "type_identifier" | "qualified_identifier"
            ) {
                parts.push(self.base.get_node_text(&child));
                break;
            }
        }

        if !parts.is_empty() {
            signature.push_str(&parts.join(" "));
            signature.push(' ');
        }

        signature.push_str(name);
        signature
    }

    fn extract_field_visibility(&self, node: Node) -> Visibility {
        // Look for access specifier in parent or preceding siblings
        // For now, return Public - TODO: Implement proper access specifier extraction
        let _ = node; // Suppress unused warning
        Visibility::Public
    }

    fn is_static_member_variable(&self, node: Node, storage_class: &Vec<String>) -> bool {
        // Check if this is a static member variable inside a class

        // First check if it has static storage class
        let has_static = storage_class.iter().any(|sc| sc == "static");

        if !has_static {
            return false;
        }

        // Check if this declaration is inside a class by walking up the tree
        let mut current = node.parent();
        while let Some(parent) = current {
            match parent.kind() {
                "class_specifier" | "struct_specifier" => return true,
                "translation_unit" => return false, // Reached top level
                _ => current = parent.parent(),
            }
        }

        false
    }

    fn extract_constructor_from_declaration(
        &mut self,
        node: Node,
        func_declarator: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract constructor from declaration with = delete, = default, etc.
        let name_node = self.extract_function_name(func_declarator)?;
        let name = self.base.get_node_text(&name_node);

        // Build signature
        let mut signature = String::new();

        // Add modifiers (explicit, etc.)
        let modifiers = self.extract_function_modifiers(node);
        if !modifiers.is_empty() {
            signature.push_str(&modifiers.join(" "));
            signature.push(' ');
        }

        // Add constructor name and parameters
        signature.push_str(&name);
        let parameters = self.extract_function_parameters(func_declarator);
        signature.push_str(&parameters);

        // Check for noexcept
        let noexcept_spec = self.extract_noexcept_specifier(func_declarator);
        if !noexcept_spec.is_empty() {
            signature.push(' ');
            signature.push_str(&noexcept_spec);
        }

        // Check for = delete, = default
        let children: Vec<Node> = node.children(&mut node.walk()).collect();
        for (i, child) in children.iter().enumerate() {
            if child.kind() == "=" && i + 1 < children.len() {
                let next_child = &children[i + 1];
                if matches!(next_child.kind(), "delete" | "default") {
                    signature.push_str(&format!(" = {}", self.base.get_node_text(next_child)));
                    break;
                }
            }
        }

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Constructor,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_method_modifiers(&self, declaration_node: Node, func_node: Node) -> Vec<String> {
        let mut modifiers = Vec::new();
        let modifier_types = [
            "virtual", "static", "explicit", "friend", "inline", "override",
        ];

        // For C++ class methods, check multiple tree levels for modifiers
        let mut nodes_to_check = vec![declaration_node, func_node];

        // Add parent nodes to check
        if let Some(parent) = declaration_node.parent() {
            nodes_to_check.push(parent);
            if let Some(grandparent) = parent.parent() {
                nodes_to_check.push(grandparent);
            }
        }

        // Check all these nodes for modifiers
        for node in nodes_to_check {
            if node.kind() == "field_declaration" || node.kind() == "declaration" {
                // Check direct children for modifier keywords
                for child in node.children(&mut node.walk()) {
                    if modifier_types.contains(&child.kind()) {
                        let modifier = self.base.get_node_text(&child);
                        if !modifiers.contains(&modifier) {
                            modifiers.push(modifier);
                        }
                    }
                    // Also check for storage_class_specifier which might contain "static"
                    else if child.kind() == "storage_class_specifier" {
                        let text = self.base.get_node_text(&child);
                        if modifier_types.contains(&text.as_str()) && !modifiers.contains(&text) {
                            modifiers.push(text);
                        }
                    }
                }
            }

            // Also do recursive search within each node
            self.collect_modifiers_recursive(node, &mut modifiers, &modifier_types);
        }

        modifiers
    }
}
