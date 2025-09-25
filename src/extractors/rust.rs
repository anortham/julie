use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, SymbolOptions, Visibility};
use tree_sitter::{Tree, Node};
use std::collections::HashMap;

/// Rust extractor that handles Rust-specific constructs including:
/// - Structs and enums
/// - Traits and implementations (impl blocks)
/// - Functions with ownership patterns (&self, &mut self, etc.)
/// - Modules (mod)
/// - Macros (macro_rules!)
/// - Use statements (imports)
/// - Constants and statics
/// - Type definitions
///
/// Port of Miller's comprehensive Rust extractor with two-phase processing
pub struct RustExtractor {
    base: BaseExtractor,
    impl_blocks: Vec<ImplBlockInfo>,
    is_processing_impl_blocks: bool,
}

#[derive(Debug, Clone)]
struct ImplBlockInfo {
    node: Node<'static>,
    type_name: String,
    #[allow(dead_code)]
    parent_id: Option<String>,
}

impl RustExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
            impl_blocks: Vec::new(),
            is_processing_impl_blocks: false,
        }
    }

    /// Extract symbols using Miller's two-phase approach
    /// Phase 1: Extract all symbols except methods in impl blocks
    /// Phase 2: Process impl blocks and link methods to parent structs/traits
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Phase 1: Extract symbols (skip impl block methods)
        self.impl_blocks.clear();
        self.is_processing_impl_blocks = false;
        self.walk_tree(tree.root_node(), &mut symbols, None);

        // Phase 2: Process impl blocks after all symbols are extracted
        self.is_processing_impl_blocks = true;
        self.process_impl_blocks(&mut symbols);

        symbols
    }

    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if let Some(symbol) = self.extract_symbol(node, parent_id.clone()) {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);

            // Continue traversing with new parent_id for nested symbols
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree(child, symbols, Some(symbol_id.clone()));
            }
        } else {
            // No symbol extracted, continue with current parent_id
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree(child, symbols, parent_id.clone());
            }
        }
    }

    fn extract_symbol(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        match node.kind() {
            "struct_item" => Some(self.extract_struct(node, parent_id)),
            "enum_item" => Some(self.extract_enum(node, parent_id)),
            "trait_item" => Some(self.extract_trait(node, parent_id)),
            "impl_item" => {
                self.extract_impl(node, parent_id);
                None // impl blocks don't create symbols directly
            }
            "function_item" => {
                // Skip if inside impl block during phase 1
                if self.is_inside_impl(node) && !self.is_processing_impl_blocks {
                    None
                } else {
                    Some(self.extract_function(node, parent_id))
                }
            }
            "function_signature_item" => Some(self.extract_function_signature(node, parent_id)),
            "associated_type" => Some(self.extract_associated_type(node, parent_id)),
            "union_item" => Some(self.extract_union(node, parent_id)),
            "macro_invocation" => self.extract_macro_invocation(node, parent_id),
            "mod_item" => Some(self.extract_module(node, parent_id)),
            "use_declaration" => self.extract_use(node, parent_id),
            "const_item" => Some(self.extract_const(node, parent_id)),
            "static_item" => Some(self.extract_static(node, parent_id)),
            "macro_definition" => Some(self.extract_macro(node, parent_id)),
            "type_item" => Some(self.extract_type_alias(node, parent_id)),
            _ => None,
        }
    }

    fn extract_struct(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        // Extract visibility and attributes
        let visibility = self.extract_visibility(node);
        let attributes = self.get_preceding_attributes(node);
        let derived_traits = self.extract_derived_traits(&attributes);

        // Extract generic type parameters
        let type_params = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_parameters")
            .map(|c| self.base.get_node_text(&c))
            .unwrap_or_default();

        // Build signature
        let mut signature = format!("{}struct {}{}", visibility, name, type_params);
        if !derived_traits.is_empty() {
            signature = format!("#[derive({})] {}", derived_traits.join(", "), signature);
        }

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_enum(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        let visibility = self.extract_visibility(node);
        let attributes = self.get_preceding_attributes(node);
        let derived_traits = self.extract_derived_traits(&attributes);

        // Extract generic type parameters
        let type_params = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_parameters")
            .map(|c| self.base.get_node_text(&c))
            .unwrap_or_default();

        let mut signature = format!("{}enum {}{}", visibility, name, type_params);
        if !derived_traits.is_empty() {
            signature = format!("#[derive({})] {}", derived_traits.join(", "), signature);
        }

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_trait(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        let visibility = self.extract_visibility(node);

        // Extract generic type parameters
        let type_params = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_parameters")
            .map(|c| self.base.get_node_text(&c))
            .unwrap_or_default();

        // Extract trait bounds
        let trait_bounds = node.children(&mut node.walk())
            .find(|c| c.kind() == "trait_bounds")
            .map(|c| self.base.get_node_text(&c))
            .unwrap_or_default();

        // Extract associated types from declaration_list
        let mut associated_types = Vec::new();
        if let Some(declaration_list) = node.children(&mut node.walk())
            .find(|c| c.kind() == "declaration_list")
        {
            for child in declaration_list.children(&mut declaration_list.walk()) {
                if child.kind() == "associated_type" {
                    let assoc_type = self.base.get_node_text(&child).replace(";", "");
                    associated_types.push(assoc_type);
                }
            }
        }

        // Build signature
        let mut signature = format!("{}trait {}{}{}", visibility, name, type_params, trait_bounds);
        if !associated_types.is_empty() {
            signature = format!("{} {{ {} }}", signature, associated_types.join("; "));
        }

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Interface,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_impl(&mut self, node: Node, parent_id: Option<String>) {
        // Store impl block info for phase 2 processing
        let type_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let type_name = type_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        // Convert Node to Node<'static> (this is safe for our use case)
        let static_node = unsafe { std::mem::transmute(node) };

        self.impl_blocks.push(ImplBlockInfo {
            node: static_node,
            type_name,
            parent_id,
        });
    }

    fn process_impl_blocks(&mut self, symbols: &mut Vec<Symbol>) {
        let impl_blocks = self.impl_blocks.clone();

        for impl_block in impl_blocks {
            // Find the struct/enum this impl is for
            let struct_symbol = symbols.iter()
                .find(|s| s.name == impl_block.type_name &&
                     (s.kind == SymbolKind::Class || s.kind == SymbolKind::Interface));

            if let Some(struct_symbol) = struct_symbol {
                let parent_id = struct_symbol.id.clone();

                // Extract methods with correct parent_id
                if let Some(declaration_list) = impl_block.node.children(&mut impl_block.node.walk())
                    .find(|c| c.kind() == "declaration_list")
                {
                    for child in declaration_list.children(&mut declaration_list.walk()) {
                        if child.kind() == "function_item" {
                            let mut method_symbol = self.extract_function(child, Some(parent_id.clone()));
                            method_symbol.kind = SymbolKind::Method;
                            symbols.push(method_symbol);
                        }
                    }
                }
            }
        }
    }

    fn extract_function(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        // Determine if this is a method (inside impl block) or standalone function
        let is_method = self.is_inside_impl(node);
        let kind = if is_method { SymbolKind::Method } else { SymbolKind::Function };

        // Extract function signature components
        let visibility = self.extract_visibility(node);
        let is_async = self.has_async_keyword(node);
        let is_unsafe = self.has_unsafe_keyword(node);
        let extern_modifier = self.extract_extern_modifier(node);
        let params = self.extract_function_parameters(node);
        let return_type = self.extract_return_type(node);

        // Extract generic type parameters
        let type_params = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_parameters")
            .map(|c| self.base.get_node_text(&c))
            .unwrap_or_default();

        // Extract where clause
        let where_clause = node.children(&mut node.walk())
            .find(|c| c.kind() == "where_clause")
            .map(|c| format!(" {}", self.base.get_node_text(&c)))
            .unwrap_or_default();

        // Build signature
        let mut signature = String::new();
        if !visibility.is_empty() {
            signature.push_str(&visibility);
        }
        if !extern_modifier.is_empty() {
            signature.push_str(&format!("{} ", extern_modifier));
        }
        if is_unsafe {
            signature.push_str("unsafe ");
        }
        if is_async {
            signature.push_str("async ");
        }
        signature.push_str(&format!("fn {}{}", name, type_params));
        signature.push_str(&format!("({})", params.join(", ")));
        if !return_type.is_empty() {
            signature.push_str(&format!(" -> {}", return_type));
        }
        signature.push_str(&where_clause);

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_module(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        let visibility = self.extract_visibility(node);
        let signature = format!("{}mod {}", visibility, name);

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Namespace,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_const(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        let visibility = self.extract_visibility(node);
        let type_node = node.child_by_field_name("type");
        let value_node = node.child_by_field_name("value");

        let mut signature = format!("{}const {}", visibility, name);
        if let Some(type_node) = type_node {
            signature.push_str(&format!(": {}", self.base.get_node_text(&type_node)));
        }
        if let Some(value_node) = value_node {
            signature.push_str(&format!(" = {}", self.base.get_node_text(&value_node)));
        }

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_static(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        let visibility = self.extract_visibility(node);
        let is_mutable = node.children(&mut node.walk())
            .any(|c| c.kind() == "mutable_specifier");
        let type_node = node.child_by_field_name("type");
        let value_node = node.child_by_field_name("value");

        let mut signature = format!("{}static ", visibility);
        if is_mutable {
            signature.push_str("mut ");
        }
        signature.push_str(&name);
        if let Some(type_node) = type_node {
            signature.push_str(&format!(": {}", self.base.get_node_text(&type_node)));
        }
        if let Some(value_node) = value_node {
            signature.push_str(&format!(" = {}", self.base.get_node_text(&value_node)));
        }

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_macro(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        let signature = format!("macro_rules! {}", name);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_type_alias(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        let visibility = self.extract_visibility(node);

        // Extract generic type parameters
        let type_params = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_parameters")
            .map(|c| self.base.get_node_text(&c))
            .unwrap_or_default();

        // Extract the type definition (after =)
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        let equals_index = children.iter().position(|c| c.kind() == "=");
        let type_def = if let Some(index) = equals_index {
            if index + 1 < children.len() {
                format!(" = {}", self.base.get_node_text(&children[index + 1]))
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let signature = format!("{}type {}{}{}", visibility, name, type_params, type_def);

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Type,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_union(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        let visibility = self.extract_visibility(node);
        let signature = format!("{}union {}", visibility, name);

        let visibility_enum = if visibility.trim().is_empty() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Union,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility_enum),
                parent_id,
                doc_comment: self.find_doc_comment(node),
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_use(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let use_text = self.base.get_node_text(&node);

        // Simple pattern matching for common use cases
        if use_text.contains(" as ") {
            // use std::collections::HashMap as Map;
            let parts: Vec<&str> = use_text.split(" as ").collect();
            if parts.len() == 2 {
                let alias = parts[1].replace(";", "").trim().to_string();
                return Some(self.base.create_symbol(
                    &node,
                    alias,
                    SymbolKind::Import,
                    SymbolOptions {
                        signature: Some(use_text),
                        visibility: Some(Visibility::Public),
                        parent_id,
                        doc_comment: None,
                        metadata: Some(HashMap::new()),
                    },
                ));
            }
        } else {
            // use std::collections::HashMap;
            if let Some(captures) = regex::Regex::new(r"use\s+(?:.*::)?(\w+)\s*;")
                .ok()
                .and_then(|re| re.captures(&use_text))
            {
                if let Some(name_match) = captures.get(1) {
                    let name = name_match.as_str().to_string();
                    return Some(self.base.create_symbol(
                        &node,
                        name,
                        SymbolKind::Import,
                        SymbolOptions {
                            signature: Some(use_text),
                            visibility: Some(Visibility::Public),
                            parent_id,
                            doc_comment: None,
                            metadata: Some(HashMap::new()),
                        },
                    ));
                }
            }
        }

        None
    }

    fn extract_macro_invocation(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let macro_name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier");
        let macro_name = macro_name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_default();

        // Look for struct-generating macros or known patterns
        if macro_name.contains("struct") || macro_name.contains("generate") {
            let token_tree_node = node.children(&mut node.walk())
                .find(|c| c.kind() == "token_tree");
            if let Some(token_tree) = token_tree_node {
                // Extract the first identifier from the token tree as the struct name
                let struct_name_node = token_tree.children(&mut token_tree.walk())
                    .find(|c| c.kind() == "identifier");
                if let Some(struct_name_node) = struct_name_node {
                    let struct_name = self.base.get_node_text(&struct_name_node);
                    let signature = format!("struct {}", struct_name);

                    return Some(self.base.create_symbol(
                        &node,
                        struct_name,
                        SymbolKind::Class,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(Visibility::Public), // assume macro-generated types are public
                            parent_id,
                            doc_comment: None,
                            metadata: Some(HashMap::new()),
                        },
                    ));
                }
            }
        }

        None
    }

    fn extract_function_signature(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        // Extract parameters
        let params_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "parameters");
        let params = params_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "()".to_string());

        // Extract return type (after -> token)
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        let arrow_index = children.iter().position(|c| c.kind() == "->");
        let return_type = if let Some(index) = arrow_index {
            if index + 1 < children.len() {
                format!(" -> {}", self.base.get_node_text(&children[index + 1]))
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let signature = format!("fn {}{}{}", name, params, return_type);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public), // extern functions are typically public
                parent_id,
                doc_comment: None,
                metadata: Some(HashMap::new()),
            },
        )
    }

    fn extract_associated_type(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "anonymous".to_string());

        // Extract trait bounds (: Debug + Clone, etc.)
        let trait_bounds = node.children(&mut node.walk())
            .find(|c| c.kind() == "trait_bounds")
            .map(|c| self.base.get_node_text(&c))
            .unwrap_or_default();

        let signature = format!("type {}{}", name, trait_bounds);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Type,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public), // associated types in traits are public
                parent_id,
                doc_comment: None,
                metadata: Some(HashMap::new()),
            },
        )
    }

    // Helper methods for Rust-specific parsing

    fn extract_visibility(&self, node: Node) -> String {
        let visibility_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "visibility_modifier");

        if let Some(vis_node) = visibility_node {
            let vis_text = self.base.get_node_text(&vis_node);
            if vis_text == "pub" {
                "pub ".to_string()
            } else if vis_text.starts_with("pub(") {
                format!("{} ", vis_text)
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    fn get_preceding_attributes<'a>(&self, node: Node<'a>) -> Vec<Node<'a>> {
        let mut attributes = Vec::new();

        if let Some(parent) = node.parent() {
            let siblings: Vec<_> = parent.children(&mut parent.walk()).collect();
            if let Some(node_index) = siblings.iter().position(|&n| n.id() == node.id()) {
                // Look backwards for attribute_item nodes
                for i in (0..node_index).rev() {
                    let sibling = siblings[i];
                    if sibling.kind() == "attribute_item" {
                        attributes.insert(0, sibling);
                    } else {
                        break; // Stop at the first non-attribute
                    }
                }
            }
        }

        attributes
    }

    fn extract_derived_traits(&self, attributes: &[Node]) -> Vec<String> {
        let mut traits = Vec::new();

        for attr in attributes {
            // Look for derive attribute
            let attribute_node = attr.children(&mut attr.walk())
                .find(|c| c.kind() == "attribute");

            if let Some(attr_node) = attribute_node {
                let identifier_node = attr_node.children(&mut attr_node.walk())
                    .find(|c| c.kind() == "identifier");

                if let Some(ident) = identifier_node {
                    if self.base.get_node_text(&ident) == "derive" {
                        // Find the token tree with the trait list
                        let token_tree = attr_node.children(&mut attr_node.walk())
                            .find(|c| c.kind() == "token_tree");

                        if let Some(tree) = token_tree {
                            for child in tree.children(&mut tree.walk()) {
                                if child.kind() == "identifier" {
                                    traits.push(self.base.get_node_text(&child));
                                }
                            }
                        }
                    }
                }
            }
        }

        traits
    }

    fn is_inside_impl(&self, node: Node) -> bool {
        let mut parent = node.parent();
        while let Some(p) = parent {
            if p.kind() == "impl_item" {
                return true;
            }
            parent = p.parent();
        }
        false
    }

    fn has_async_keyword(&self, node: Node) -> bool {
        node.children(&mut node.walk())
            .any(|c| c.kind() == "async" || self.base.get_node_text(&c) == "async")
    }

    fn has_unsafe_keyword(&self, node: Node) -> bool {
        node.children(&mut node.walk())
            .any(|c| c.kind() == "unsafe" || self.base.get_node_text(&c) == "unsafe")
    }

    fn extract_extern_modifier(&self, node: Node) -> String {
        let function_modifiers_node = node.children(&mut node.walk())
            .find(|c| c.kind() == "function_modifiers");

        if let Some(modifiers) = function_modifiers_node {
            let extern_modifier_node = modifiers.children(&mut modifiers.walk())
                .find(|c| c.kind() == "extern_modifier");

            if let Some(extern_node) = extern_modifier_node {
                return self.base.get_node_text(&extern_node);
            }
        }

        String::new()
    }

    fn extract_function_parameters(&self, node: Node) -> Vec<String> {
        let mut parameters = Vec::new();
        let param_list = node.child_by_field_name("parameters");

        if let Some(params) = param_list {
            for child in params.children(&mut params.walk()) {
                if child.kind() == "parameter" {
                    let param_text = self.base.get_node_text(&child);
                    parameters.push(param_text);
                } else if child.kind() == "self_parameter" {
                    // Handle &self, &mut self, self, etc.
                    let self_text = self.base.get_node_text(&child);
                    parameters.push(self_text);
                }
            }
        }

        parameters
    }

    fn extract_return_type(&self, node: Node) -> String {
        let return_type_node = node.child_by_field_name("return_type");

        if let Some(ret_type) = return_type_node {
            // Skip the -> token and get the actual type
            let type_nodes: Vec<_> = ret_type.children(&mut ret_type.walk())
                .filter(|c| c.kind() != "->" && self.base.get_node_text(c) != "->")
                .collect();

            if !type_nodes.is_empty() {
                return type_nodes.iter()
                    .map(|n| self.base.get_node_text(n))
                    .collect::<Vec<_>>()
                    .join("");
            }
        }

        String::new()
    }

    fn find_doc_comment(&self, node: Node) -> Option<String> {
        // Look for preceding doc comments (///)
        if let Some(parent) = node.parent() {
            let siblings: Vec<_> = parent.children(&mut parent.walk()).collect();
            if let Some(node_index) = siblings.iter().position(|&n| n.id() == node.id()) {
                if node_index > 0 {
                    let prev_sibling = siblings[node_index - 1];
                    if prev_sibling.kind() == "line_comment" {
                        let comment_text = self.base.get_node_text(&prev_sibling);
                        // Rust doc comments start with ///
                        if comment_text.starts_with("///") {
                            return Some(comment_text[3..].trim().to_string());
                        }
                    }
                }
            }
        }

        // Look for attribute doc comments like #[doc = "..."]
        let attributes = self.get_preceding_attributes(node);
        for attr in &attributes {
            if let Some(doc_comment) = self.extract_doc_from_attribute(*attr) {
                return Some(doc_comment);
            }
        }

        None
    }

    fn extract_doc_from_attribute(&self, node: Node) -> Option<String> {
        let attr_text = self.base.get_node_text(&node);
        if let Some(captures) = regex::Regex::new(r#"#\[doc\s*=\s*"([^"]+)"\]"#)
            .ok()
            .and_then(|re| re.captures(&attr_text))
        {
            if let Some(doc_match) = captures.get(1) {
                return Some(doc_match.as_str().to_string());
            }
        }
        None
    }

    /// Extract relationships between Rust symbols
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        let symbol_map: HashMap<String, &Symbol> = symbols.iter()
            .map(|s| (s.name.clone(), s))
            .collect();

        self.walk_tree_for_relationships(tree.root_node(), &symbol_map, &mut relationships);
        relationships
    }

    fn walk_tree_for_relationships(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "impl_item" => {
                self.extract_impl_relationships(node, symbol_map, relationships);
            }
            "struct_item" | "enum_item" => {
                self.extract_type_relationships(node, symbol_map, relationships);
            }
            "call_expression" => {
                self.extract_call_relationships(node, symbol_map, relationships);
            }
            _ => {}
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_relationships(child, symbol_map, relationships);
        }
    }

    fn extract_impl_relationships(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // Look for "impl TraitName for TypeName" pattern
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        let mut trait_name = String::new();
        let mut type_name = String::new();
        let mut found_for = false;

        for child in children {
            if child.kind() == "type_identifier" {
                if !found_for {
                    trait_name = self.base.get_node_text(&child);
                } else {
                    type_name = self.base.get_node_text(&child);
                    break;
                }
            } else if child.kind() == "for" {
                found_for = true;
            }
        }

        // If we found both trait and type, create implements relationship
        if !trait_name.is_empty() && !type_name.is_empty() {
            if let (Some(trait_symbol), Some(type_symbol)) =
                (symbol_map.get(&trait_name), symbol_map.get(&type_name))
            {
                relationships.push(Relationship {
                    id: format!("{}_{}_{:?}_{}", type_symbol.id, trait_symbol.id, RelationshipKind::Implements, node.start_position().row),
                    from_symbol_id: type_symbol.id.clone(),
                    to_symbol_id: trait_symbol.id.clone(),
                    kind: RelationshipKind::Implements,
                    file_path: self.base.file_path.clone(),
                    line_number: node.start_position().row as u32 + 1,
                    confidence: 0.95,
                    metadata: None,
                });
            }
        }
    }

    fn extract_type_relationships(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        let name_node = node.child_by_field_name("name");
        if let Some(name_node) = name_node {
            let type_name = self.base.get_node_text(&name_node);
            if let Some(type_symbol) = symbol_map.get(&type_name) {
                // Look for field types that reference other symbols
                let declaration_list = node.children(&mut node.walk())
                    .find(|c| c.kind() == "field_declaration_list" || c.kind() == "enum_variant_list");

                if let Some(decl_list) = declaration_list {
                    for field in decl_list.children(&mut decl_list.walk()) {
                        if field.kind() == "field_declaration" || field.kind() == "enum_variant" {
                            self.extract_field_type_references(field, type_symbol, symbol_map, relationships);
                        }
                    }
                }
            }
        }
    }

    fn extract_field_type_references(
        &self,
        field_node: Node,
        container_symbol: &Symbol,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // Find type references in field declarations
        for child in field_node.children(&mut field_node.walk()) {
            if child.kind() == "type_identifier" {
                let referenced_type_name = self.base.get_node_text(&child);
                if let Some(referenced_symbol) = symbol_map.get(&referenced_type_name) {
                    if referenced_symbol.id != container_symbol.id {
                        relationships.push(Relationship {
                            id: format!("{}_{}_{:?}_{}", container_symbol.id, referenced_symbol.id, RelationshipKind::Uses, field_node.start_position().row),
                            from_symbol_id: container_symbol.id.clone(),
                            to_symbol_id: referenced_symbol.id.clone(),
                            kind: RelationshipKind::Uses,
                            file_path: self.base.file_path.clone(),
                            line_number: field_node.start_position().row as u32 + 1,
                            confidence: 0.8,
                            metadata: None,
                        });
                    }
                }
            }
        }
    }

    fn extract_call_relationships(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // Extract function/method call relationships
        let function_node = node.child_by_field_name("function");
        if let Some(func_node) = function_node {
            // Handle method calls (receiver.method())
            if func_node.kind() == "field_expression" {
                let method_node = func_node.child_by_field_name("field");
                if let Some(method_node) = method_node {
                    let method_name = self.base.get_node_text(&method_node);
                    if let Some(called_symbol) = symbol_map.get(&method_name) {
                        // Find the calling function context
                        if let Some(calling_function) = self.find_containing_function(node) {
                            if let Some(caller_symbol) = symbol_map.get(&calling_function) {
                                relationships.push(Relationship {
                                    id: format!("{}_{}_{:?}_{}", caller_symbol.id, called_symbol.id, RelationshipKind::Calls, node.start_position().row),
                                    from_symbol_id: caller_symbol.id.clone(),
                                    to_symbol_id: called_symbol.id.clone(),
                                    kind: RelationshipKind::Calls,
                                    file_path: self.base.file_path.clone(),
                                    line_number: node.start_position().row as u32 + 1,
                                    confidence: 0.9,
                                    metadata: None,
                                });
                            }
                        }
                    }
                }
            }
            // Handle direct function calls
            else if func_node.kind() == "identifier" {
                let function_name = self.base.get_node_text(&func_node);
                if let Some(called_symbol) = symbol_map.get(&function_name) {
                    if let Some(calling_function) = self.find_containing_function(node) {
                        if let Some(caller_symbol) = symbol_map.get(&calling_function) {
                            relationships.push(Relationship {
                                id: format!("{}_{}_{:?}_{}", caller_symbol.id, called_symbol.id, RelationshipKind::Calls, node.start_position().row),
                                from_symbol_id: caller_symbol.id.clone(),
                                to_symbol_id: called_symbol.id.clone(),
                                kind: RelationshipKind::Calls,
                                file_path: self.base.file_path.clone(),
                                line_number: node.start_position().row as u32 + 1,
                                confidence: 0.9,
                                metadata: None,
                            });
                        }
                    }
                }
            }
        }
    }

    fn find_containing_function(&self, node: Node) -> Option<String> {
        let mut parent = node.parent();

        while let Some(p) = parent {
            if p.kind() == "function_item" {
                let name_node = p.child_by_field_name("name");
                return name_node.map(|n| self.base.get_node_text(&n));
            }
            parent = p.parent();
        }

        None
    }
}