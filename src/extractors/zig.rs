use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, SymbolOptions, Visibility};
use tree_sitter::{Node, Tree};
use std::collections::HashMap;

pub struct ZigExtractor {
    base: BaseExtractor,
}

impl ZigExtractor {
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

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) -> Option<String> {
        if node.kind().is_empty() {
            return parent_id;
        }

        let mut current_parent_id = parent_id.clone();

        if let Some(symbol) = self.extract_symbol_from_node(node, parent_id.as_ref()) {
            current_parent_id = Some(symbol.id.clone());
            symbols.push(symbol);
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }

        current_parent_id
    }

    fn extract_symbol_from_node(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        match node.kind() {
            "function_declaration" | "function_definition" => self.extract_function(node, parent_id),
            "test_declaration" => self.extract_test(node, parent_id),
            "struct_declaration" => self.extract_struct(node, parent_id),
            "union_declaration" => self.extract_union(node, parent_id),
            "enum_declaration" => self.extract_enum(node, parent_id),
            "variable_declaration" | "const_declaration" => self.extract_variable(node, parent_id),
            "error_declaration" => self.extract_error_type(node, parent_id),
            "type_declaration" => self.extract_type_alias(node, parent_id),
            "parameter" => self.extract_parameter(node, parent_id),
            "field_declaration" | "struct_field" | "container_field" => self.extract_struct_field(node, parent_id),
            "enum_field" | "enum_variant" => self.extract_enum_variant(node, parent_id),
            "ERROR" => self.extract_from_error_node(node, parent_id),
            _ => None,
        }
    }

    fn extract_function(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let name = self.base.get_node_text(&name_node);

        // Check function visibility and modifiers
        let is_public = self.is_public_function(node);
        let is_export = self.is_export_function(node);
        let is_inside_struct = self.is_inside_struct(node);

        let symbol_kind = if is_inside_struct {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };

        let signature = self.extract_function_signature(node);
        let visibility = if is_public || is_export {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let doc_comment = self.base.extract_documentation(&node);

        Some(self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment,
            },
        ))
    }

    fn extract_test(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        // Extract test name from string node
        let string_node = self.base.find_child_by_type(&node,"string")?;

        // Get the actual test name from string_content
        let string_content_node = self.base.find_child_by_type(&string_node,"string_content");
        let test_name = if let Some(content_node) = string_content_node {
            self.base.get_node_text(&content_node)
        } else {
            // Fallback to the full string text, removing quotes
            let full_text = self.base.get_node_text(&string_node);
            full_text.trim_matches('"').to_string()
        };

        let signature = format!("test \"{}\"", test_name);
        let doc_comment = self.base.extract_documentation(&node);

        let metadata = Some({
            let mut meta = HashMap::new();
            meta.insert("isTest".to_string(), serde_json::Value::Bool(true));
            meta
        });

        Some(self.base.create_symbol(
            &node,
            test_name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata,
                doc_comment,
            },
        ))
    }

    fn extract_parameter(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let param_name = self.base.get_node_text(&name_node);

        let type_node = self.base.find_child_by_type(&node,"type_expression")
            .or_else(|| self.base.find_child_by_type(&node,"builtin_type"))
            .or_else(|| {
                // Look for identifier after colon for type
                let mut cursor = node.walk();
                let children: Vec<Node> = node.children(&mut cursor).collect();
                let colon_index = children.iter().position(|child| child.kind() == ":")?;
                children.get(colon_index + 1).copied()
            });

        let param_type = if let Some(type_node) = type_node {
            self.base.get_node_text(&type_node)
        } else {
            "unknown".to_string()
        };

        let signature = format!("{}: {}", param_name, param_type);

        Some(self.base.create_symbol(
            &node,
            param_name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_struct(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let name = self.base.get_node_text(&name_node);
        let is_public = self.is_public_declaration(node);

        let signature = format!("struct {}", name);
        let visibility = if is_public {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let doc_comment = self.base.extract_documentation(&node);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment,
            },
        ))
    }

    fn extract_union(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let name = self.base.get_node_text(&name_node);
        let is_public = self.is_public_declaration(node);

        // Check if it's a union(enum) or regular union
        let node_text = self.base.get_node_text(&node);
        let union_type = if node_text.contains("union(enum)") {
            "union(enum)"
        } else {
            "union"
        };

        let signature = format!("{} {}", union_type, name);
        let visibility = if is_public {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let doc_comment = self.base.extract_documentation(&node);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment,
            },
        ))
    }

    fn extract_struct_field(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let field_name = self.base.get_node_text(&name_node);

        // Look for type information in various forms
        let type_node = self.base.find_child_by_type(&node,"type_expression")
            .or_else(|| self.base.find_child_by_type(&node,"builtin_type"))
            .or_else(|| self.base.find_child_by_type(&node,"slice_type"))
            .or_else(|| {
                // Look for identifier after colon for type
                let mut cursor = node.walk();
                let children: Vec<Node> = node.children(&mut cursor).collect();
                let colon_index = children.iter().position(|child| child.kind() == ":")?;
                children.get(colon_index + 1).copied()
            });

        let field_type = if let Some(type_node) = type_node {
            self.base.get_node_text(&type_node)
        } else {
            "unknown".to_string()
        };

        let signature = format!("{}: {}", field_name, field_type);

        Some(self.base.create_symbol(
            &node,
            field_name,
            SymbolKind::Field,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public), // Zig struct fields are generally public
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_enum(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let name = self.base.get_node_text(&name_node);
        let is_public = self.is_public_declaration(node);

        let signature = format!("enum {}", name);
        let visibility = if is_public {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let doc_comment = self.base.extract_documentation(&node);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Enum,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment,
            },
        ))
    }

    fn extract_enum_variant(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let variant_name = self.base.get_node_text(&name_node);

        Some(self.base.create_symbol(
            &node,
            variant_name.clone(),
            SymbolKind::EnumMember,
            SymbolOptions {
                signature: Some(variant_name),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: None,
            },
        ))
    }

    fn extract_variable(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let name = self.base.get_node_text(&name_node);
        let is_const = node.kind() == "const_declaration" || self.base.get_node_text(&node).contains("const");
        let is_public = self.is_public_declaration(node);

        let node_text = self.base.get_node_text(&node);

        // Check for generic type constructor
        if node_text.contains("(comptime") && node_text.contains("= struct") {
            let param_match = regex::Regex::new(r"\(([^)]+)\)").unwrap().find(&node_text);
            let params = if let Some(param_match) = param_match {
                param_match.as_str()
            } else {
                "(comptime T: type)"
            };

            let signature = format!("fn {}({}) type", name, &params[1..params.len()-1]);
            let visibility = if is_public {
                Visibility::Public
            } else {
                Visibility::Private
            };

            let metadata = Some({
                let mut meta = HashMap::new();
                meta.insert("isGenericTypeConstructor".to_string(), serde_json::Value::Bool(true));
                meta
            });

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Function,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.cloned(),
                    metadata,
                    doc_comment: self.base.extract_documentation(&node),
                },
            ));
        }

        // Check for struct declaration
        let struct_node = self.base.find_child_by_type(&node,"struct_declaration");
        if let Some(_struct_node) = struct_node {
            let struct_type = if node_text.contains("packed struct") {
                "packed struct"
            } else if node_text.contains("extern struct") {
                "extern struct"
            } else {
                "struct"
            };

            let signature = format!("const {} = {}", name, struct_type);
            let visibility = if is_public {
                Visibility::Public
            } else {
                Visibility::Private
            };

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Class,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.cloned(),
                    metadata: None,
                    doc_comment: self.base.extract_documentation(&node),
                },
            ));
        }

        // Check for union declaration
        let union_node = self.base.find_child_by_type(&node,"union_declaration");
        if let Some(_union_node) = union_node {
            let union_type = if node_text.contains("union(enum)") {
                "union(enum)"
            } else {
                "union"
            };

            let signature = format!("const {} = {}", name, union_type);
            let visibility = if is_public {
                Visibility::Public
            } else {
                Visibility::Private
            };

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Class,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.cloned(),
                    metadata: None,
                    doc_comment: self.base.extract_documentation(&node),
                },
            ));
        }

        // Check for enum declaration
        let enum_node = self.base.find_child_by_type(&node,"enum_declaration");
        if let Some(_enum_node) = enum_node {
            let enum_match = regex::Regex::new(r"enum\(([^)]+)\)").unwrap().find(&node_text);
            let enum_type = if let Some(enum_match) = enum_match {
                enum_match.as_str().to_string()
            } else {
                "enum".to_string()
            };

            let signature = format!("const {} = {}", name, enum_type);
            let visibility = if is_public {
                Visibility::Public
            } else {
                Visibility::Private
            };

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Enum,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.cloned(),
                    metadata: None,
                    doc_comment: self.base.extract_documentation(&node),
                },
            ));
        }

        // Check for error set or error union declaration
        if node_text.contains("error{") || node_text.contains("error {") {
            let mut signature = format!("const {} = ", name);

            if node_text.contains("||") {
                let union_match = regex::Regex::new(r"error\s*\{[^}]*\}\s*\|\|\s*(\w+)").unwrap().captures(&node_text);
                if let Some(union_match) = union_match {
                    signature.push_str(&format!("error{{...}} || {}", &union_match[1]));
                } else {
                    signature.push_str("error{...} || ...");
                }
            } else {
                signature.push_str("error{...}");
            }

            let visibility = if is_public {
                Visibility::Public
            } else {
                Visibility::Private
            };

            let metadata = Some({
                let mut meta = HashMap::new();
                meta.insert("isErrorSet".to_string(), serde_json::Value::Bool(true));
                meta
            });

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Class,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.cloned(),
                    metadata,
                    doc_comment: self.base.extract_documentation(&node),
                },
            ));
        }

        // Check for function type declaration
        if node_text.contains("fn (") || node_text.contains("fn(") {
            let fn_type_match = regex::Regex::new(r"=\s*(fn\s*\([^}]*\).*?)(?:;|$)").unwrap().captures(&node_text);
            let fn_type = if let Some(fn_type_match) = fn_type_match {
                fn_type_match[1].to_string()
            } else {
                "fn (...)".to_string()
            };

            let signature = format!("const {} = {}", name, fn_type);
            let visibility = if is_public {
                Visibility::Public
            } else {
                Visibility::Private
            };

            let metadata = Some({
                let mut meta = HashMap::new();
                meta.insert("isFunctionType".to_string(), serde_json::Value::Bool(true));
                meta
            });

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Interface,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.cloned(),
                    metadata,
                    doc_comment: self.base.extract_documentation(&node),
                },
            ));
        }

        // Extract type if available, or detect switch expressions
        let type_node = self.base.find_child_by_type(&node,"type_expression");
        let switch_node = self.base.find_child_by_type(&node,"switch_expression");

        let mut var_type = if let Some(type_node) = type_node {
            self.base.get_node_text(&type_node)
        } else {
            "inferred".to_string()
        };

        // For type aliases, extract the assignment value
        if var_type == "inferred" && is_const {
            let assignment_match = regex::Regex::new(r"=\s*([^;]+)").unwrap().captures(&node_text);
            if let Some(assignment_match) = assignment_match {
                var_type = assignment_match[1].trim().to_string();
            }
        }

        // If it contains a switch expression, include that in the signature
        if let Some(switch_node) = switch_node {
            let switch_text = self.base.get_node_text(&switch_node);
            if switch_text.len() > 50 {
                var_type = format!("switch({}...)", &switch_text[0..20]);
            } else {
                var_type = switch_text;
            }
        }

        let symbol_kind = if is_const {
            SymbolKind::Constant
        } else {
            SymbolKind::Variable
        };

        let signature = format!("{} {}: {}", if is_const { "const" } else { "var" }, name, var_type);
        let visibility = if is_public {
            Visibility::Public
        } else {
            Visibility::Private
        };

        Some(self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.cloned(),
                metadata: None,
                doc_comment: self.base.extract_documentation(&node),
            },
        ))
    }

    fn extract_error_type(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let name = self.base.get_node_text(&name_node);

        let signature = format!("error {}", name);
        let metadata = Some({
            let mut meta = HashMap::new();
            meta.insert("isErrorType".to_string(), serde_json::Value::Bool(true));
            meta
        });

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata,
                doc_comment: self.base.extract_documentation(&node),
            },
        ))
    }

    fn extract_type_alias(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        let name_node = self.base.find_child_by_type(&node,"identifier")?;
        let name = self.base.get_node_text(&name_node);
        let is_public = self.is_public_declaration(node);

        let signature = format!("type {}", name);
        let visibility = if is_public {
            Visibility::Public
        } else {
            Visibility::Private
        };

        let metadata = Some({
            let mut meta = HashMap::new();
            meta.insert("isTypeAlias".to_string(), serde_json::Value::Bool(true));
            meta
        });

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Interface,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.cloned(),
                metadata,
                doc_comment: self.base.extract_documentation(&node),
            },
        ))
    }

    fn extract_from_error_node(&mut self, node: Node, parent_id: Option<&String>) -> Option<Symbol> {
        // Try to extract meaningful symbols from ERROR nodes
        let node_text = self.base.get_node_text(&node);

        // Look for partial generic type constructor pattern in fragmented ERROR nodes
        let partial_match = regex::Regex::new(r"^const\s+(\w+)\s*\($").unwrap().captures(&node_text);

        if let Some(partial_match) = partial_match {
            let name = partial_match[1].to_string();

            let signature = format!("fn {}(comptime T: type) type", name);
            let metadata = Some({
                let mut meta = HashMap::new();
                meta.insert("isGenericTypeConstructor".to_string(), serde_json::Value::Bool(true));
                meta
            });

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Function,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.cloned(),
                    metadata,
                    doc_comment: self.base.extract_documentation(&node),
                },
            ));
        }

        None
    }

    fn is_public_function(&self, node: Node) -> bool {
        // Check for "pub" keyword as first child of function
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "pub" || self.base.get_node_text(&child) == "pub" {
                return true;
            }
        }

        // Also check for "pub" keyword before function (fallback)
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "pub" || self.base.get_node_text(&prev) == "pub" {
                return true;
            }
        }

        false
    }

    fn is_export_function(&self, node: Node) -> bool {
        // Check for "export" keyword as first child of function
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "export" || self.base.get_node_text(&child) == "export" {
                return true;
            }
        }

        // Also check for "export" keyword before function (fallback)
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "export" || self.base.get_node_text(&prev) == "export" {
                return true;
            }
        }

        false
    }

    fn is_inline_function(&self, node: Node) -> bool {
        // Check for "inline" keyword in function children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "inline" || self.base.get_node_text(&child) == "inline" {
                return true;
            }
        }

        // Also check for "inline" keyword before function (fallback)
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "inline" || self.base.get_node_text(&prev) == "inline" {
                return true;
            }
        }

        false
    }

    fn is_public_declaration(&self, node: Node) -> bool {
        // Check for "pub" keyword before declaration
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "pub" || self.base.get_node_text(&prev) == "pub" {
                return true;
            }
        }
        false
    }

    fn is_inside_struct(&self, node: Node) -> bool {
        // Walk up the tree to see if we're inside a struct declaration
        let mut current = node.parent();
        while let Some(parent) = current {
            match parent.kind() {
                "struct_declaration" | "container_declaration" | "enum_declaration" => {
                    return true;
                }
                _ => {
                    current = parent.parent();
                }
            }
        }
        false
    }

    fn extract_function_signature(&mut self, node: Node) -> String {
        let name_node = self.base.find_child_by_type(&node,"identifier");
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "unknown".to_string()
        };

        // Check for visibility and function modifiers
        let is_public = self.is_public_function(node);
        let is_export = self.is_export_function(node);
        let is_inline = self.is_inline_function(node);

        let mut modifier_prefix = String::new();
        if is_public {
            modifier_prefix.push_str("pub ");
        }
        if is_export {
            modifier_prefix.push_str("export ");
        }
        if is_inline {
            modifier_prefix.push_str("inline ");
        }

        // Check for extern prefix
        let extern_node = self.base.find_child_by_type(&node,"extern");
        let string_node = self.base.find_child_by_type(&node,"string");
        let mut extern_prefix = String::new();
        if extern_node.is_some() && string_node.is_some() {
            let linkage = self.base.get_node_text(&string_node.unwrap());
            extern_prefix = format!("extern {} ", linkage);
        }

        // Extract parameters
        let mut params = Vec::new();
        let param_list = self.base.find_child_by_type(&node,"parameters")
            .or_else(|| self.base.find_child_by_type(&node,"parameter_list"));

        if let Some(param_list) = param_list {
            let mut cursor = param_list.walk();
            for child in param_list.children(&mut cursor) {
                if child.kind() == "parameter" {
                    // Handle comptime parameters
                    let comptime_node = self.base.find_child_by_type(&child,"comptime");
                    let param_name_node = self.base.find_child_by_type(&child,"identifier");

                    // Look for type nodes
                    let type_node = self.base.find_child_by_type(&child,"type_expression")
                        .or_else(|| self.base.find_child_by_type(&child,"builtin_type"))
                        .or_else(|| self.base.find_child_by_type(&child,"pointer_type"))
                        .or_else(|| self.base.find_child_by_type(&child,"slice_type"))
                        .or_else(|| self.base.find_child_by_type(&child,"optional_type"))
                        .or_else(|| {
                            // Look for identifier after colon
                            let mut param_cursor = child.walk();
                            let param_children: Vec<Node> = child.children(&mut param_cursor).collect();
                            let colon_index = param_children.iter().position(|c| c.kind() == ":")?;
                            param_children.get(colon_index + 1).copied()
                        });

                    if let Some(param_name_node) = param_name_node {
                        let param_name = self.base.get_node_text(&param_name_node);
                        let param_type = if let Some(type_node) = type_node {
                            self.base.get_node_text(&type_node)
                        } else {
                            String::new()
                        };

                        let param_str = if comptime_node.is_some() {
                            if param_type.is_empty() {
                                format!("comptime {}", param_name)
                            } else {
                                format!("comptime {}: {}", param_name, param_type)
                            }
                        } else if !param_type.is_empty() {
                            format!("{}: {}", param_name, param_type)
                        } else {
                            param_name
                        };

                        params.push(param_str);
                    }
                } else if child.kind() == "variadic_parameter" || self.base.get_node_text(&child) == "..." {
                    params.push("...".to_string());
                }
            }
        }

        // Check if the raw function text contains "..." for variadic parameters
        let full_function_text = self.base.get_node_text(&node);
        if full_function_text.contains("...") && !params.iter().any(|p| p == "...") {
            params.push("...".to_string());
        }

        // Extract return type
        let return_type_node = self.base.find_child_by_type(&node,"return_type")
            .or_else(|| self.base.find_child_by_type(&node,"type_expression"))
            .or_else(|| self.base.find_child_by_type(&node,"pointer_type"))
            .or_else(|| self.base.find_child_by_type(&node,"error_union_type"))
            .or_else(|| self.base.find_child_by_type(&node,"nullable_type"))
            .or_else(|| self.base.find_child_by_type(&node,"optional_type"))
            .or_else(|| self.base.find_child_by_type(&node,"slice_type"))
            .or_else(|| self.base.find_child_by_type(&node,"builtin_type"));

        let return_type = if let Some(return_type_node) = return_type_node {
            self.base.get_node_text(&return_type_node)
        } else {
            "void".to_string()
        };

        format!("{}{}fn {}({}) {}", modifier_prefix, extern_prefix, name, params.join(", "), return_type)
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.traverse_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn traverse_for_relationships(&mut self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        match node.kind() {
            "struct_declaration" => {
                self.extract_struct_relationships(node, symbols, relationships);
            }
            "const_declaration" => {
                // Check const declarations for struct definitions
                if let Some(_struct_node) = self.base.find_child_by_type(&node,"struct_declaration") {
                    self.extract_struct_relationships(node, symbols, relationships);
                }
            }
            "call_expression" => {
                self.extract_function_call_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        // Recursively traverse children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_for_relationships(child, symbols, relationships);
        }
    }

    fn extract_struct_relationships(&mut self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        if node.kind() != "struct_declaration" {
            return;
        }

        // Find a symbol that matches this struct_declaration by position
        let struct_symbol = symbols.iter().find(|s| {
            s.kind == SymbolKind::Class &&
            s.start_line == (node.start_position().row + 1) as u32 &&
            s.start_column == node.start_position().column as u32
        }).or_else(|| {
            // Try finding by nearby position (within a few lines)
            symbols.iter().find(|s| {
                s.kind == SymbolKind::Class &&
                (s.start_line as i32 - (node.start_position().row + 1) as i32).abs() <= 2
            })
        });

        if let Some(target_symbol) = struct_symbol {
            self.traverse_struct_fields(node, symbols, relationships, target_symbol);
        }
    }

    fn traverse_struct_fields(&mut self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>, target_symbol: &Symbol) {
        let mut cursor = node.walk();
        for field_node in node.children(&mut cursor) {
            if field_node.kind() == "container_field" {
                if let Some(field_name_node) = self.base.find_child_by_type(&field_node, "identifier") {
                    let _field_name = self.base.get_node_text(&field_name_node);

                    // Look for type information
                    let type_node = self.base.find_child_by_type(&field_node, "type_expression")
                        .or_else(|| self.base.find_child_by_type(&field_node, "builtin_type"))
                        .or_else(|| self.base.find_child_by_type(&field_node, "slice_type"))
                        .or_else(|| self.base.find_child_by_type(&field_node, "pointer_type"))
                        .or_else(|| {
                            // Look for identifier after colon
                            let mut field_cursor = field_node.walk();
                            let field_children: Vec<Node> = field_node.children(&mut field_cursor).collect();
                            let colon_index = field_children.iter().position(|c| c.kind() == ":")?;
                            field_children.get(colon_index + 1).copied()
                        });

                    if let Some(type_node) = type_node {
                        let type_name = self.base.get_node_text(&type_node).trim().to_string();

                        // Look for referenced symbols that are struct types
                        let referenced_symbol = symbols.iter().find(|s| {
                            s.name == type_name &&
                            matches!(s.kind, SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct)
                        });

                        if let Some(referenced_symbol) = referenced_symbol {
                            if referenced_symbol.id != target_symbol.id {
                                // Create composition relationship
                                relationships.push(Relationship {
                                    id: format!("{}_{}_{:?}_{}", target_symbol.id, referenced_symbol.id, RelationshipKind::Composition, field_node.start_position().row),
                                    from_symbol_id: target_symbol.id.clone(),
                                    to_symbol_id: referenced_symbol.id.clone(),
                                    kind: RelationshipKind::Composition,
                                    file_path: self.base.file_path.clone(),
                                    line_number: (field_node.start_position().row + 1) as u32,
                                    confidence: 0.8,
                                    metadata: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn extract_function_call_relationships(&mut self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        let mut called_func_name: Option<String> = None;

        // Check for direct function call (identifier + arguments)
        if let Some(func_name_node) = self.base.find_child_by_type(&node,"identifier") {
            called_func_name = Some(self.base.get_node_text(&func_name_node));
        } else if let Some(field_expr_node) = self.base.find_child_by_type(&node,"field_expression") {
            // Check for method call (field_expression + arguments)
            let identifiers = self.base.find_children_by_type(&field_expr_node, "identifier");
            if identifiers.len() >= 2 {
                called_func_name = Some(self.base.get_node_text(&identifiers[1])); // Second identifier is the method name
            }
        }

        if let Some(called_func_name) = called_func_name {
            let called_symbol = symbols.iter().find(|s| s.name == called_func_name && s.kind == SymbolKind::Function);

            if let Some(called_symbol) = called_symbol {
                // Find the calling function
                let mut current = node.parent();
                while let Some(parent) = current {
                    if matches!(parent.kind(), "function_declaration" | "function_definition") {
                        if let Some(caller_name_node) = self.base.find_child_by_type(&parent, "identifier") {
                            let caller_name = self.base.get_node_text(&caller_name_node);
                            let caller_symbol = symbols.iter().find(|s| s.name == caller_name && s.kind == SymbolKind::Function);

                            if let Some(caller_symbol) = caller_symbol {
                                if caller_symbol.id != called_symbol.id {
                                    relationships.push(Relationship {
                                        id: format!("{}_{}_{:?}_{}", caller_symbol.id, called_symbol.id, RelationshipKind::Calls, node.start_position().row),
                                        from_symbol_id: caller_symbol.id.clone(),
                                        to_symbol_id: called_symbol.id.clone(),
                                        kind: RelationshipKind::Calls,
                                        file_path: self.base.file_path.clone(),
                                        line_number: (node.start_position().row + 1) as u32,
                                        confidence: 0.9,
                                        metadata: None,
                                    });
                                }
                            }
                        }
                        break;
                    }
                    current = parent.parent();
                }
            }
        }
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        // Zig type inference based on symbol metadata and signatures
        for symbol in symbols {
            if let Some(signature) = &symbol.signature {
                // Extract Zig types from signatures
                let zig_type_pattern = regex::Regex::new(r":\s*([\w\[\]!?*]+)").unwrap();
                if let Some(type_match) = zig_type_pattern.captures(signature) {
                    types.insert(symbol.name.clone(), type_match[1].to_string());
                }
            }

            // Use metadata for Zig-specific types
            if let Some(is_error) = symbol.metadata.as_ref().and_then(|m| m.get("isErrorType")).and_then(|v| v.as_bool()) {
                if is_error {
                    types.insert(symbol.name.clone(), "error".to_string());
                }
            }
            if let Some(is_type_alias) = symbol.metadata.as_ref().and_then(|m| m.get("isTypeAlias")).and_then(|v| v.as_bool()) {
                if is_type_alias {
                    types.insert(symbol.name.clone(), "type".to_string());
                }
            }

            match symbol.kind {
                SymbolKind::Class => {
                    if symbol.metadata.as_ref().and_then(|m| m.get("isErrorType")).and_then(|v| v.as_bool()) != Some(true) {
                        types.insert(symbol.name.clone(), "struct".to_string());
                    }
                }
                SymbolKind::Enum => {
                    types.insert(symbol.name.clone(), "enum".to_string());
                }
                _ => {}
            }
        }

        types
    }
}