// C# Language Extractor
//
// Direct port of Miller's csharp-extractor.ts (1027 lines) to idiomatic Rust
// Original: /Users/murphy/Source/miller/src/extractors/csharp-extractor.ts
//
// This extractor handles C#-specific constructs including:
// - Namespaces and using statements (regular, static, global)
// - Classes, interfaces, structs, and enums
// - Methods, constructors, and properties
// - Fields, events, and delegates
// - Records and nested types
// - Attributes and generics
// - Inheritance and implementation relationships
// - Modern C# features (nullable types, records, pattern matching)

use crate::extractors::base::{
    self, BaseExtractor, Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// C# extractor using tree-sitter-c-sharp parser
pub struct CSharpExtractor {
    base: BaseExtractor,
}

impl CSharpExtractor {
    /// Create new C# extractor
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    /// Extract symbols from C# code - port of Miller's extractSymbols method
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Walk tree and extract symbols - port of Miller's walkTree method
    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        let symbol = self.extract_symbol(node, parent_id.clone());
        let current_parent_id = if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            Some(sym.id.clone())
        } else {
            parent_id
        };

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract symbol from node - port of Miller's extractSymbol method
    fn extract_symbol(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        match node.kind() {
            "namespace_declaration" => self.extract_namespace(node, parent_id),
            "using_directive" => self.extract_using(node, parent_id),
            "class_declaration" => self.extract_class(node, parent_id),
            "interface_declaration" => self.extract_interface(node, parent_id),
            "struct_declaration" => self.extract_struct(node, parent_id),
            "enum_declaration" => self.extract_enum(node, parent_id),
            "enum_member_declaration" => self.extract_enum_member(node, parent_id),
            "method_declaration" => self.extract_method(node, parent_id),
            "constructor_declaration" => self.extract_constructor(node, parent_id),
            "property_declaration" => self.extract_property(node, parent_id),
            "field_declaration" => self.extract_field(node, parent_id),
            "event_field_declaration" => self.extract_event(node, parent_id),
            "delegate_declaration" => self.extract_delegate(node, parent_id),
            "record_declaration" => self.extract_record(node, parent_id),
            "destructor_declaration" => self.extract_destructor(node, parent_id),
            "operator_declaration" => self.extract_operator(node, parent_id),
            "conversion_operator_declaration" => self.extract_conversion_operator(node, parent_id),
            "indexer_declaration" => self.extract_indexer(node, parent_id),
            _ => None,
        }
    }

    /// Extract namespace - port of Miller's extractNamespace
    fn extract_namespace(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "qualified_name" || c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let signature = format!("namespace {}", name);

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Namespace, options),
        )
    }

    /// Extract using statement - port of Miller's extractUsing
    fn extract_using(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node.children(&mut cursor).find(|c| {
            matches!(
                c.kind(),
                "qualified_name" | "identifier" | "member_access_expression"
            )
        })?;

        let full_using_path = self.base.get_node_text(&name_node);

        // Check if it's a static using
        let is_static = node.children(&mut cursor).any(|c| c.kind() == "static");

        // Check for alias (using alias = namespace)
        let mut cursor2 = node.walk();
        let alias_node = node
            .children(&mut cursor2)
            .find(|c| c.kind() == "name_equals");
        let name = if let Some(alias_node) = alias_node {
            // Extract alias name
            let mut alias_cursor = alias_node.walk();
            let alias_identifier = alias_node
                .children(&mut alias_cursor)
                .find(|c| c.kind() == "identifier");
            if let Some(alias_identifier) = alias_identifier {
                self.base.get_node_text(&alias_identifier)
            } else {
                full_using_path.clone()
            }
        } else {
            // Extract the last part of the namespace for the symbol name
            full_using_path
                .split('.')
                .next_back()
                .unwrap_or(&full_using_path)
                .to_string()
        };

        let signature = if is_static {
            format!("using static {}", full_using_path)
        } else {
            format!("using {}", full_using_path)
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Import, options),
        )
    }

    /// Extract class - port of Miller's extractClass
    fn extract_class(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("class {}", name)
        } else {
            format!("{} class {}", modifiers.join(" "), name)
        };

        // Handle generic type parameters
        if let Some(type_params) = self.extract_type_parameters(&node) {
            signature = signature.replace(
                &format!("class {}", name),
                &format!("class {}{}", name, type_params),
            );
        }

        // Check for inheritance and implementations
        let base_list = self.extract_base_list(&node);
        if !base_list.is_empty() {
            signature += &format!(" : {}", base_list.join(", "));
        }

        // Handle where clauses (type parameter constraints)
        let mut node_cursor = node.walk();
        let where_clauses: Vec<String> = node
            .children(&mut node_cursor)
            .filter(|c| c.kind() == "type_parameter_constraints_clause")
            .map(|clause| self.base.get_node_text(&clause))
            .collect();

        if !where_clauses.is_empty() {
            signature += &format!(" {}", where_clauses.join(" "));
        }

        // Store actual C# visibility in metadata for internal classes
        let mut metadata = std::collections::HashMap::new();
        let csharp_visibility = self.get_csharp_visibility_string(&modifiers);
        metadata.insert(
            "csharp_visibility".to_string(),
            serde_json::Value::String(csharp_visibility),
        );

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            metadata: Some(metadata),
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Class, options),
        )
    }

    /// Extract interface - port of Miller's extractInterface
    fn extract_interface(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("interface {}", name)
        } else {
            format!("{} interface {}", modifiers.join(" "), name)
        };

        // Handle generic type parameters
        if let Some(type_params) = self.extract_type_parameters(&node) {
            signature = signature.replace(
                &format!("interface {}", name),
                &format!("interface {}{}", name, type_params),
            );
        }

        // Check for interface inheritance
        let base_list = self.extract_base_list(&node);
        if !base_list.is_empty() {
            signature += &format!(" : {}", base_list.join(", "));
        }

        // Handle where clauses (type parameter constraints)
        let mut node_cursor = node.walk();
        let where_clauses: Vec<String> = node
            .children(&mut node_cursor)
            .filter(|c| c.kind() == "type_parameter_constraints_clause")
            .map(|clause| self.base.get_node_text(&clause))
            .collect();

        if !where_clauses.is_empty() {
            signature += &format!(" {}", where_clauses.join(" "));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Interface, options),
        )
    }

    /// Extract struct - port of Miller's extractStruct
    fn extract_struct(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("struct {}", name)
        } else {
            format!("{} struct {}", modifiers.join(" "), name)
        };

        // Handle generic type parameters
        if let Some(type_params) = self.extract_type_parameters(&node) {
            signature = signature.replace(
                &format!("struct {}", name),
                &format!("struct {}{}", name, type_params),
            );
        }

        // Check for interface implementations
        let base_list = self.extract_base_list(&node);
        if !base_list.is_empty() {
            signature += &format!(" : {}", base_list.join(", "));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Struct, options),
        )
    }

    /// Extract enum - port of Miller's extractEnum
    fn extract_enum(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("enum {}", name)
        } else {
            format!("{} enum {}", modifiers.join(" "), name)
        };

        // Check for base type (e.g., : byte)
        let base_list = self.extract_base_list(&node);
        if !base_list.is_empty() {
            signature += &format!(" : {}", base_list[0]);
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Enum, options),
        )
    }

    /// Extract enum member - port of Miller's extractEnumMember
    fn extract_enum_member(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);

        // Build signature - include value if present
        let mut signature = name.clone();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        if let Some(equals_index) = children.iter().position(|c| c.kind() == "=") {
            if equals_index + 1 < children.len() {
                let value_nodes: Vec<String> = children[equals_index + 1..]
                    .iter()
                    .map(|n| self.base.get_node_text(n))
                    .collect();
                let value = value_nodes.join("").trim().to_string();
                if !value.is_empty() {
                    signature += &format!(" = {}", value);
                }
            }
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public), // Enum members are always public in C#
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::EnumMember, options),
        )
    }

    /// Extract method - port of Miller's extractMethod
    fn extract_method(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        // Find method name identifier - comes before parameter_list (may have type_parameter_list in between)
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let param_list_index = children.iter().position(|c| c.kind() == "parameter_list")?;

        // Look backwards from parameter_list to find the method name identifier
        let name_node = children[..param_list_index]
            .iter()
            .rev()
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Get return type
        let return_type = self
            .extract_return_type(&node)
            .unwrap_or_else(|| "void".to_string());

        // Get parameters
        let param_list = children.iter().find(|c| c.kind() == "parameter_list");
        let params = param_list
            .map(|p| self.base.get_node_text(p))
            .unwrap_or_else(|| "()".to_string());

        // Handle generic type parameters on the method
        let type_params = self.extract_type_parameters(&node);

        // Build signature
        let modifier_str = if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        };
        let type_param_str = type_params
            .as_ref()
            .map(|tp| format!("{} ", tp))
            .unwrap_or_default();
        let mut signature = format!(
            "{}{}{} {}{}",
            modifier_str, type_param_str, return_type, name, params
        );

        // Handle expression-bodied method (=> expression)
        let arrow_clause = children
            .iter()
            .find(|c| c.kind() == "arrow_expression_clause");
        if let Some(arrow_clause) = arrow_clause {
            signature += &format!(" {}", self.base.get_node_text(arrow_clause));
        }

        // Handle where clauses (type parameter constraints)
        let where_clauses: Vec<String> = children
            .iter()
            .filter(|c| c.kind() == "type_parameter_constraints_clause")
            .map(|clause| self.base.get_node_text(clause))
            .collect();

        if !where_clauses.is_empty() {
            signature += &format!(" {}", where_clauses.join(" "));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Method, options),
        )
    }

    /// Extract constructor - port of Miller's extractConstructor
    fn extract_constructor(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, Some("constructor_declaration"));

        // Get parameters
        let param_list = node
            .children(&mut cursor)
            .find(|c| c.kind() == "parameter_list");
        let params = param_list
            .map(|p| self.base.get_node_text(&p))
            .unwrap_or_else(|| "()".to_string());

        // Build signature (constructors don't have return types)
        let signature = if modifiers.is_empty() {
            format!("{}{}", name, params)
        } else {
            format!("{} {}{}", modifiers.join(" "), name, params)
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Constructor, options),
        )
    }

    /// Extract property - port of Miller's extractProperty
    fn extract_property(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Get property type
        let prop_type = self
            .extract_property_type(&node)
            .unwrap_or_else(|| "var".to_string());

        // Get accessor list (get/set)
        let accessor_list = node
            .children(&mut cursor)
            .find(|c| c.kind() == "accessor_list");
        let accessors = if let Some(accessor_list) = accessor_list {
            format!(" {}", self.base.get_node_text(&accessor_list))
        } else {
            // Expression-bodied property
            let arrow_clause = node
                .children(&mut cursor)
                .find(|c| c.kind() == "arrow_expression_clause");
            if let Some(arrow_clause) = arrow_clause {
                format!(" {}", self.base.get_node_text(&arrow_clause))
            } else {
                String::new()
            }
        };

        // Build signature
        let signature = if modifiers.is_empty() {
            format!("{} {}{}", prop_type, name, accessors)
        } else {
            format!(
                "{} {} {}{}",
                modifiers.join(" "),
                prop_type,
                name,
                accessors
            )
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Property, options),
        )
    }

    /// Extract field - port of Miller's extractField
    fn extract_field(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Get field type
        let field_type = self
            .extract_field_type(&node)
            .unwrap_or_else(|| "var".to_string());

        // Get variable declaration and then variable declarator(s)
        let mut cursor = node.walk();
        let var_declaration = node
            .children(&mut cursor)
            .find(|c| c.kind() == "variable_declaration")?;

        let mut var_cursor = var_declaration.walk();
        let declarators: Vec<Node> = var_declaration
            .children(&mut var_cursor)
            .filter(|c| c.kind() == "variable_declarator")
            .collect();

        // For now, handle the first declarator (we could extend to handle multiple)
        let declarator = declarators.first()?;
        let mut decl_cursor = declarator.walk();
        let name_node = declarator
            .children(&mut decl_cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);

        // Check if it's a constant (const or static readonly)
        let is_constant = modifiers.contains(&"const".to_string())
            || (modifiers.contains(&"static".to_string())
                && modifiers.contains(&"readonly".to_string()));
        let symbol_kind = if is_constant {
            SymbolKind::Constant
        } else {
            SymbolKind::Field
        };

        // Get initializer if present
        let children: Vec<Node> = declarator.children(&mut decl_cursor).collect();
        let initializer = if let Some(equals_index) = children.iter().position(|c| c.kind() == "=")
        {
            if equals_index + 1 < children.len() {
                let init_nodes: Vec<String> = children[equals_index + 1..]
                    .iter()
                    .map(|n| self.base.get_node_text(n))
                    .collect();
                let init_text = init_nodes.join("").trim().to_string();
                if !init_text.is_empty() {
                    format!(" = {}", init_text)
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Build signature
        let signature = if modifiers.is_empty() {
            format!("{} {}{}", field_type, name, initializer)
        } else {
            format!(
                "{} {} {}{}",
                modifiers.join(" "),
                field_type,
                name,
                initializer
            )
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, symbol_kind, options))
    }

    /// Extract event - port of Miller's extractEvent
    fn extract_event(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        // For event_field_declaration, the structure is:
        // modifier* event variable_declaration
        let mut cursor = node.walk();
        let var_declaration = node
            .children(&mut cursor)
            .find(|c| c.kind() == "variable_declaration")?;

        let mut var_cursor = var_declaration.walk();
        let var_declarator = var_declaration
            .children(&mut var_cursor)
            .find(|c| c.kind() == "variable_declarator")?;

        let mut decl_cursor = var_declarator.walk();
        let name_node = var_declarator
            .children(&mut decl_cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Get event type (first child of variable_declaration that's not variable_declarator)
        let type_node = var_declaration
            .children(&mut var_cursor)
            .find(|c| c.kind() != "variable_declarator");
        let event_type = type_node
            .map(|node| self.base.get_node_text(&node))
            .unwrap_or_else(|| "EventHandler".to_string());

        // Build signature
        let signature = if modifiers.is_empty() {
            format!("event {} {}", event_type, name)
        } else {
            format!("{} event {} {}", modifiers.join(" "), event_type, name)
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Event, options),
        )
    }

    /// Extract delegate - port of Miller's extractDelegate
    fn extract_delegate(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        // For delegates, we need to find the delegate name identifier, not the return type identifier
        // The structure is: modifiers delegate returnType delegateName<typeParams>(params)
        // We need to find the identifier that comes after the return type

        // First, find the 'delegate' keyword
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let _delegate_keyword = children.iter().find(|c| c.kind() == "delegate")?;
        let delegate_index = children.iter().position(|c| c.kind() == "delegate")?;

        // Find identifiers after the delegate keyword
        let identifiers_after_delegate: Vec<&Node> = children[delegate_index + 1..]
            .iter()
            .filter(|c| c.kind() == "identifier")
            .collect();

        // The delegate name is typically the last identifier before type_parameter_list or parameter_list
        let name_node = if identifiers_after_delegate.len() == 1 {
            // Simple case: delegate void EventHandler<T>(T data)
            identifiers_after_delegate[0]
        } else if identifiers_after_delegate.len() >= 2 {
            // Complex case: delegate TResult Func<T>(T input) - the name is the second identifier
            identifiers_after_delegate[1]
        } else {
            return None;
        };

        let name = self.base.get_node_text(name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Get return type - for delegates, it's the first type-like node after 'delegate'
        let mut return_type = "void".to_string();
        for child in &children[delegate_index + 1..] {
            if matches!(
                child.kind(),
                "predefined_type" | "identifier" | "qualified_name" | "generic_name"
            ) {
                return_type = self.base.get_node_text(child);
                break;
            }
        }

        // Get parameters
        let param_list = children.iter().find(|c| c.kind() == "parameter_list");
        let params = param_list
            .map(|p| self.base.get_node_text(p))
            .unwrap_or_else(|| "()".to_string());

        // Handle generic type parameters
        let type_params = self.extract_type_parameters(&node);

        // Build signature
        let modifier_str = if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        };
        let name_with_type_params = type_params
            .map(|tp| format!("{}{}", name, tp))
            .unwrap_or_else(|| name.clone());
        let signature = format!(
            "{}delegate {} {}{}",
            modifier_str, return_type, name_with_type_params, params
        );

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Delegate, options),
        )
    }

    /// Extract record - port of Miller's extractRecord
    fn extract_record(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let name = self.base.get_node_text(&name_node);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Determine if it's a record struct
        let is_struct = modifiers.contains(&"struct".to_string())
            || node.children(&mut cursor).any(|c| c.kind() == "struct");

        // Build signature
        let record_type = if is_struct { "record struct" } else { "record" };
        let mut signature = if modifiers.is_empty() {
            format!("{} {}", record_type, name)
        } else {
            format!("{} {} {}", modifiers.join(" "), record_type, name)
        };

        // Handle record parameters
        if let Some(param_list) = node
            .children(&mut cursor)
            .find(|c| c.kind() == "parameter_list")
        {
            signature += &self.base.get_node_text(&param_list);
        }

        // Handle inheritance (base_list)
        if let Some(base_list) = node.children(&mut cursor).find(|c| c.kind() == "base_list") {
            signature += &format!(" {}", self.base.get_node_text(&base_list));
        }

        let symbol_kind = if is_struct {
            SymbolKind::Struct
        } else {
            SymbolKind::Class
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(self.base.create_symbol(&node, name, symbol_kind, options))
    }

    /// Extract destructor - port of Miller's extractDestructor
    fn extract_destructor(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        // Find class name identifier (Child 1)
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier")?;

        let class_name = self.base.get_node_text(&name_node);
        let name = format!("~{}", class_name);

        // Get parameters
        let param_list = node
            .children(&mut cursor)
            .find(|c| c.kind() == "parameter_list");
        let params = param_list
            .map(|p| self.base.get_node_text(&p))
            .unwrap_or_else(|| "()".to_string());

        // Build signature
        let signature = format!("~{}{}", class_name, params);

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Protected), // Destructors are implicitly protected
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Method, options),
        )
    }

    /// Extract operator - port of Miller's extractOperator
    fn extract_operator(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        // Find operator symbol
        let mut cursor = node.walk();
        let operator_symbol = node.children(&mut cursor).find(|c| {
            matches!(
                c.kind(),
                "+" | "-"
                    | "*"
                    | "/"
                    | "=="
                    | "!="
                    | "<"
                    | ">"
                    | "<="
                    | ">="
                    | "!"
                    | "~"
                    | "++"
                    | "--"
                    | "%"
                    | "&"
                    | "|"
                    | "^"
                    | "<<"
                    | ">>"
                    | "true"
                    | "false"
            )
        })?;

        let operator_text = self.base.get_node_text(&operator_symbol);
        let name = format!("operator {}", operator_text);
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Find return type (before 'operator' keyword)
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let operator_keyword_index = children
            .iter()
            .position(|c| self.base.get_node_text(c) == "operator")?;

        let return_type_node = children[..operator_keyword_index]
            .iter()
            .find(|c| matches!(c.kind(), "predefined_type" | "identifier" | "generic_name"));
        let return_type = return_type_node
            .map(|node| self.base.get_node_text(node))
            .unwrap_or_else(|| "void".to_string());

        // Get parameters
        let param_list = children.iter().find(|c| c.kind() == "parameter_list");
        let params = param_list
            .map(|p| self.base.get_node_text(p))
            .unwrap_or_else(|| "()".to_string());

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("{} operator {}{}", return_type, operator_text, params)
        } else {
            format!(
                "{} {} operator {}{}",
                modifiers.join(" "),
                return_type,
                operator_text,
                params
            )
        };

        // Handle expression-bodied operator
        if let Some(arrow_clause) = children
            .iter()
            .find(|c| c.kind() == "arrow_expression_clause")
        {
            signature += &format!(" {}", self.base.get_node_text(arrow_clause));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Method, options),
        )
    }

    /// Extract conversion operator - port of Miller's extractConversionOperator
    fn extract_conversion_operator(
        &mut self,
        node: Node,
        parent_id: Option<String>,
    ) -> Option<Symbol> {
        // Find conversion type (implicit/explicit)
        let mut cursor = node.walk();
        let conversion_type = node.children(&mut cursor).find(|c| {
            self.base.get_node_text(c) == "implicit" || self.base.get_node_text(c) == "explicit"
        })?;
        let conversion_text = self.base.get_node_text(&conversion_type);

        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Find target type (after 'operator' keyword)
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let operator_keyword_index = children
            .iter()
            .position(|c| self.base.get_node_text(c) == "operator")?;

        let target_type_node = children[operator_keyword_index + 1..]
            .iter()
            .find(|c| matches!(c.kind(), "predefined_type" | "identifier" | "generic_name"));
        let target_type = target_type_node
            .map(|node| self.base.get_node_text(node))
            .unwrap_or_else(|| "unknown".to_string());

        let name = format!("{} operator {}", conversion_text, target_type);

        // Get parameters
        let param_list = children.iter().find(|c| c.kind() == "parameter_list");
        let params = param_list
            .map(|p| self.base.get_node_text(p))
            .unwrap_or_else(|| "()".to_string());

        // Build signature
        let mut signature = if modifiers.is_empty() {
            format!("{} operator {}{}", conversion_text, target_type, params)
        } else {
            format!(
                "{} {} operator {}{}",
                modifiers.join(" "),
                conversion_text,
                target_type,
                params
            )
        };

        // Handle expression-bodied operator
        if let Some(arrow_clause) = children
            .iter()
            .find(|c| c.kind() == "arrow_expression_clause")
        {
            signature += &format!(" {}", self.base.get_node_text(arrow_clause));
        }

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Method, options),
        )
    }

    /// Extract indexer - port of Miller's extractIndexer
    fn extract_indexer(&mut self, node: Node, parent_id: Option<String>) -> Option<Symbol> {
        let modifiers = self.extract_modifiers(&node);
        let visibility = self.determine_visibility(&modifiers, None);

        // Find return type
        let mut cursor = node.walk();
        let return_type_node = node
            .children(&mut cursor)
            .find(|c| matches!(c.kind(), "predefined_type" | "identifier" | "generic_name"));
        let return_type = return_type_node
            .map(|node| self.base.get_node_text(&node))
            .unwrap_or_else(|| "object".to_string());

        // Get bracketed parameters
        let bracketed_params = node
            .children(&mut cursor)
            .find(|c| c.kind() == "bracketed_parameter_list");
        let params = bracketed_params
            .map(|p| self.base.get_node_text(&p))
            .unwrap_or_else(|| "[object index]".to_string());

        let name = format!("this{}", params);

        // Build signature
        let signature = if modifiers.is_empty() {
            format!("{} this{}", return_type, params)
        } else {
            format!("{} {} this{}", modifiers.join(" "), return_type, params)
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            ..Default::default()
        };

        Some(
            self.base
                .create_symbol(&node, name, SymbolKind::Property, options),
        )
    }

    /// Extract relationships - port of Miller's extractRelationships
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    /// Visit node for relationships - port of Miller's visitNode
    fn visit_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_declaration" | "interface_declaration" | "struct_declaration" => {
                self.extract_inheritance_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_relationships(child, symbols, relationships);
        }
    }

    /// Extract inheritance relationships - port of Miller's extractInheritanceRelationships
    fn extract_inheritance_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // Find the current symbol (class/interface/struct)
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier");
        let Some(name_node) = name_node else { return };

        let current_symbol_name = self.base.get_node_text(&name_node);
        let Some(current_symbol) = symbols.iter().find(|s| s.name == current_symbol_name) else {
            return;
        };

        // Find base_list (inheritance/implementation)
        let base_list = node.children(&mut cursor).find(|c| c.kind() == "base_list");
        let Some(base_list) = base_list else { return };

        // Extract base types
        let mut base_cursor = base_list.walk();
        let base_types: Vec<String> = base_list
            .children(&mut base_cursor)
            .filter(|c| c.kind() != ":" && c.kind() != ",")
            .map(|c| self.base.get_node_text(&c))
            .collect();

        for base_type_name in base_types {
            if let Some(base_symbol) = symbols.iter().find(|s| s.name == base_type_name) {
                // Determine relationship kind based on base type
                let relationship_kind = if base_symbol.kind == SymbolKind::Interface {
                    RelationshipKind::Implements
                } else {
                    RelationshipKind::Extends
                };

                let relationship = Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        current_symbol.id,
                        base_symbol.id,
                        relationship_kind,
                        node.start_position().row
                    ),
                    from_symbol_id: current_symbol.id.clone(),
                    to_symbol_id: base_symbol.id.clone(),
                    kind: relationship_kind,
                    file_path: self.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: None,
                };

                relationships.push(relationship);
            }
        }
    }

    /// Infer types - port of Miller's inferTypes
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut type_map = HashMap::new();

        for symbol in symbols {
            let inferred_type = match symbol.kind {
                SymbolKind::Method | SymbolKind::Function => self.infer_method_return_type(symbol),
                SymbolKind::Property => self.infer_property_type(symbol),
                SymbolKind::Field | SymbolKind::Constant => self.infer_field_type(symbol),
                SymbolKind::Variable => self.infer_variable_type(symbol),
                _ => None,
            };

            if let Some(inferred_type) = inferred_type {
                type_map.insert(symbol.id.clone(), inferred_type);
            }
        }

        type_map
    }

    /// Infer method return type - port of Miller's inferMethodReturnType
    fn infer_method_return_type(&self, symbol: &Symbol) -> Option<String> {
        let signature = symbol.signature.as_ref()?;

        // Parse method signature to extract return type
        let parts: Vec<&str> = signature.split_whitespace().collect();
        let modifiers = [
            "public",
            "private",
            "protected",
            "internal",
            "static",
            "virtual",
            "override",
            "abstract",
            "async",
            "sealed",
        ];

        // Find the method name position
        let method_name_index = parts.iter().position(|part| part.contains(&symbol.name))?;

        if method_name_index > 0 {
            // The return type is typically the part just before the method name
            // Skip modifiers like public, static, async, etc.
            for i in (0..method_name_index).rev() {
                let part = parts[i];
                if !modifiers.contains(&part) && !part.is_empty() {
                    return Some(part.to_string());
                }
            }
        }

        None
    }

    /// Infer property type - port of Miller's inferPropertyType
    fn infer_property_type(&self, symbol: &Symbol) -> Option<String> {
        let signature = symbol.signature.as_ref()?;

        // Parse property signature to extract type
        let parts: Vec<&str> = signature.split_whitespace().collect();
        let modifiers = [
            "public",
            "private",
            "protected",
            "internal",
            "static",
            "virtual",
            "override",
            "abstract",
        ];

        // Find the first non-modifier part which should be the type
        for part in &parts {
            if !modifiers.contains(part) && !part.is_empty() {
                return Some(part.to_string());
            }
        }

        None
    }

    /// Infer field type - port of Miller's inferFieldType
    fn infer_field_type(&self, symbol: &Symbol) -> Option<String> {
        let signature = symbol.signature.as_ref()?;

        // Parse field signature to extract type
        let parts: Vec<&str> = signature.split_whitespace().collect();
        let modifiers = [
            "public",
            "private",
            "protected",
            "internal",
            "static",
            "readonly",
            "const",
            "volatile",
        ];

        // Find the first non-modifier part which should be the type
        for part in &parts {
            if !modifiers.contains(part) && !part.is_empty() {
                return Some(part.to_string());
            }
        }

        None
    }

    /// Infer variable type - port of Miller's inferVariableType
    fn infer_variable_type(&self, _symbol: &Symbol) -> Option<String> {
        // For variables, we'd need more context from the AST
        // For now, return None as it's not covered in the test
        None
    }

    // Helper methods for C#-specific parsing - direct ports of Miller's helper methods

    /// Extract modifiers - port of Miller's extractModifiers
    fn extract_modifiers(&self, node: &Node) -> Vec<String> {
        let mut attributes = Vec::new();
        let mut modifiers = Vec::new();

        let mut cursor = node.walk();

        // Extract attributes
        for child in node.children(&mut cursor) {
            if child.kind() == "attribute_list" {
                attributes.push(self.base.get_node_text(&child));
            }
        }

        // Extract modifiers
        for child in node.children(&mut cursor) {
            if child.kind() == "modifier" {
                modifiers.push(self.base.get_node_text(&child));
            }
        }

        // Combine attributes and modifiers
        [attributes, modifiers].concat()
    }

    /// Determine visibility - port of Miller's determineVisibility
    fn determine_visibility(&self, modifiers: &[String], node_type: Option<&str>) -> Visibility {
        if modifiers.contains(&"public".to_string()) {
            return Visibility::Public;
        }
        if modifiers.contains(&"private".to_string()) {
            return Visibility::Private;
        }
        if modifiers.contains(&"protected".to_string()) {
            return Visibility::Protected;
        }
        if modifiers.contains(&"internal".to_string()) {
            return Visibility::Private; // Map internal to Private, store actual value in metadata
        }

        // Special cases for default visibility
        if node_type == Some("constructor_declaration") {
            return Visibility::Public; // Constructors default to public when in public classes
        }

        // Default visibility in C#
        Visibility::Private
    }

    /// Get C# visibility string including internal
    fn get_csharp_visibility_string(&self, modifiers: &[String]) -> String {
        if modifiers.contains(&"public".to_string()) {
            "public".to_string()
        } else if modifiers.contains(&"private".to_string()) {
            "private".to_string()
        } else if modifiers.contains(&"protected".to_string()) {
            "protected".to_string()
        } else if modifiers.contains(&"internal".to_string()) {
            "internal".to_string()
        } else {
            "private".to_string() // Default
        }
    }

    /// Extract base list - port of Miller's extractBaseList
    fn extract_base_list(&self, node: &Node) -> Vec<String> {
        let mut cursor = node.walk();
        let base_list = node.children(&mut cursor).find(|c| c.kind() == "base_list");

        if let Some(base_list) = base_list {
            let mut base_cursor = base_list.walk();
            base_list
                .children(&mut base_cursor)
                .filter(|c| c.kind() != ":" && c.kind() != ",")
                .map(|c| self.base.get_node_text(&c))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Extract type parameters - port of Miller's extractTypeParameters
    fn extract_type_parameters(&self, node: &Node) -> Option<String> {
        let mut cursor = node.walk();
        let type_params = node
            .children(&mut cursor)
            .find(|c| c.kind() == "type_parameter_list");
        type_params.map(|tp| self.base.get_node_text(&tp))
    }

    /// Extract return type - port of Miller's extractReturnType
    fn extract_return_type(&self, node: &Node) -> Option<String> {
        // Find method name identifier - comes before parameter_list (may have type_parameter_list in between)
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let param_list_index = children.iter().position(|c| c.kind() == "parameter_list")?;

        // Look backwards from parameter_list to find the method name identifier
        let name_node = children[..param_list_index]
            .iter()
            .rev()
            .find(|c| c.kind() == "identifier")?;

        let name_index = children.iter().position(|c| std::ptr::eq(c, name_node))?;
        // Look for return type, but exclude modifiers
        let return_type_node = children[..name_index].iter().find(|c| {
            matches!(
                c.kind(),
                "predefined_type"
                    | "identifier"
                    | "qualified_name"
                    | "generic_name"
                    | "array_type"
                    | "nullable_type"
                    | "tuple_type"
            )
        });

        return_type_node.map(|node| self.base.get_node_text(node))
    }

    /// Extract property type - port of Miller's extractPropertyType
    fn extract_property_type(&self, node: &Node) -> Option<String> {
        // In C# property declarations, the type is typically the first significant node
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Skip modifiers and find the type node
        let modifiers = [
            "public",
            "private",
            "protected",
            "internal",
            "static",
            "virtual",
            "override",
            "abstract",
        ];

        for child in &children {
            let child_text = self.base.get_node_text(child);

            // Skip modifier nodes
            if modifiers.contains(&child_text.as_str()) {
                continue;
            }

            // Look for type nodes
            if matches!(
                child.kind(),
                "predefined_type"
                    | "identifier"
                    | "qualified_name"
                    | "generic_name"
                    | "array_type"
                    | "nullable_type"
                    | "tuple_type"
            ) {
                return Some(child_text);
            }
        }

        None
    }

    /// Extract field type - port of Miller's extractFieldType
    fn extract_field_type(&self, node: &Node) -> Option<String> {
        // Field type is the first child of variable_declaration
        let mut cursor = node.walk();
        let var_declaration = node
            .children(&mut cursor)
            .find(|c| c.kind() == "variable_declaration")?;

        let mut var_cursor = var_declaration.walk();
        let type_node = var_declaration.children(&mut var_cursor).find(|c| {
            matches!(
                c.kind(),
                "predefined_type"
                    | "identifier"
                    | "qualified_name"
                    | "generic_name"
                    | "array_type"
                    | "nullable_type"
            )
        });

        type_node.map(|node| self.base.get_node_text(&node))
    }

    // ========================================================================
    // Identifier Extraction (for LSP-quality find_references)
    // ========================================================================

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);

        // Return the collected identifiers
        self.base.identifiers.clone()
    }

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // Function/method calls: foo(), bar.Baz()
            "invocation_expression" => {
                // The name is typically a child of the invocation_expression
                // Look for identifier or member_access_expression
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = self.base.get_node_text(&child);
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    } else if child.kind() == "member_access_expression" {
                        // For member access, extract the rightmost identifier (the method name)
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.base.get_node_text(&name_node);
                            let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &name_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                        break;
                    }
                }
            }

            // Member access: object.field
            "member_access_expression" => {
                // Only extract if it's NOT part of an invocation_expression
                // (we handle those in the invocation_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "invocation_expression" {
                        return; // Skip - handled by invocation_expression
                    }
                }

                // Extract the rightmost identifier (the member name)
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
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
