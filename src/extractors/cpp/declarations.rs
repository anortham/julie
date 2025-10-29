//! Declaration extraction for C++ symbols
//! Handles extraction of declarations, fields, friend declarations, and using declarations

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

use super::functions;
use super::helpers;

/// Extract namespace declaration
pub(super) fn extract_namespace(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "namespace_identifier")?;

    let name = base.get_node_text(&name_node);
    let signature = format!("namespace {}", name);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Namespace,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract using declarations and namespace aliases
pub(super) fn extract_using(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut name = String::new();
    let mut signature = String::new();

    if node.kind() == "using_declaration" {
        let mut cursor = node.walk();
        let qualified_id_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "qualified_identifier" || c.kind() == "identifier")?;

        let full_path = base.get_node_text(&qualified_id_node);

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

        name = base.get_node_text(alias_node);
        let target = base.get_node_text(target_node);
        signature = format!("namespace {} = {}", name, target);
    }

    if name.is_empty() {
        return None;
    }

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract template declaration
/// Templates are handled during tree walking - this is a stub for now
pub(super) fn extract_template(
    _base: &mut BaseExtractor,
    _node: Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // Templates are handled by extracting the inner declaration
    // during tree walking in the main extract_symbol logic
    None
}

/// Extract declaration (which may contain variables, functions, etc.)
pub(super) fn extract_declaration(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Check if this is a friend declaration first
    let node_text = base.get_node_text(&node);
    let has_friend = node
        .children(&mut node.walk())
        .any(|c| c.kind() == "friend" || base.get_node_text(&c) == "friend");

    let has_friend_text = node_text.starts_with("friend") || node_text.contains(" friend ");

    if has_friend || has_friend_text {
        return extract_friend_declaration(base, node, parent_id);
    }

    // Check if this is a conversion operator (e.g., operator double())
    let operator_cast = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "operator_cast");
    if operator_cast.is_some() {
        return extract_conversion_operator(base, node, parent_id);
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
            return extract_destructor_from_declaration(base, node, func_declarator, parent_id);
        }

        // Check if this is a constructor (function name matches class name)
        let name_node = functions::extract_function_name(func_declarator)?;
        let name = base.get_node_text(&name_node);

        if functions::is_constructor(base, &name, node) {
            return extract_constructor_from_declaration(base, node, func_declarator, parent_id);
        }

        // This is a function declaration, treat it as a function
        return functions::extract_function(base, node, parent_id);
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

        let name = base.get_node_text(&identifier_node);

        // Get storage class and type specifiers
        let storage_class = helpers::extract_storage_class(base, node);
        let type_specifiers = helpers::extract_type_specifiers(base, node);
        let is_constant = helpers::is_constant_declaration(&storage_class, &type_specifiers);

        // Check if this is a static member variable inside a class
        let is_static_member = helpers::is_static_member_variable(node, &storage_class);

        let kind = if is_constant || is_static_member {
            SymbolKind::Constant
        } else {
            SymbolKind::Variable
        };

        // Build signature
        let signature = build_direct_variable_signature(base, node, &name);
        let visibility = extract_visibility_from_node(base, node);

        let doc_comment = base.find_doc_comment(&node);

        return Some(base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment,
            },
        ));
    }

    // For now, handle the first declarator
    let declarator = declarators.first()?;
    let name_node = helpers::extract_declarator_name(*declarator)?;
    let name = base.get_node_text(&name_node);

    // Get storage class and type specifiers
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);
    let is_constant = helpers::is_constant_declaration(&storage_class, &type_specifiers);

    let kind = if is_constant {
        SymbolKind::Constant
    } else {
        SymbolKind::Variable
    };

    // Build signature
    let signature = build_variable_signature(base, node, &name);
    let visibility = extract_visibility_from_node(base, node);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract field declaration (class member variable)
/// Returns Vec<Symbol> because a single declaration can define multiple fields
/// Examples:
///   size_t rows, cols;  → extracts both "rows" and "cols"
///   double* data;       → extracts "data" (pointer_declarator)
pub(super) fn extract_field(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Vec<Symbol> {
    // C++ field declarations can have multiple declarators on same line
    // Also need to handle pointer_declarator for pointer fields
    let declarators: Vec<Node> = node
        .children(&mut node.walk())
        .filter(|c| {
            matches!(
                c.kind(),
                "field_declarator" | "init_declarator" | "pointer_declarator"
            )
        })
        .collect();

    if declarators.is_empty() {
        // No declarators found - could be:
        // 1. Direct field_identifiers (e.g., size_t rows, cols;)
        // 2. function_declarator (method declaration inside class)

        // Check for function_declarator (method declarations)
        if node
            .children(&mut node.walk())
            .any(|c| c.kind() == "function_declarator")
        {
            // This is a method declaration inside a class - not a field
            // Don't handle it here - let it be processed as a function
            // Return empty vec to signal this should be handled elsewhere
            return vec![];
        }

        // Check for direct field_identifiers
        let field_identifiers: Vec<Node> = node
            .children(&mut node.walk())
            .filter(|c| c.kind() == "field_identifier")
            .collect();

        if field_identifiers.is_empty() {
            return vec![];
        }

        // Get storage class and type specifiers once (shared by all fields)
        let storage_class = helpers::extract_storage_class(base, node);
        let type_specifiers = helpers::extract_type_specifiers(base, node);
        let is_constant = helpers::is_constant_declaration(&storage_class, &type_specifiers);
        let is_static_member = helpers::is_static_member_variable(node, &storage_class);

        // Create a symbol for EACH field_identifier (handles: size_t rows, cols;)
        let mut symbols = Vec::new();
        let doc_comment = base.find_doc_comment(&node);
        for field_node in field_identifiers {
            let name = base.get_node_text(&field_node);

            let kind = if is_constant || is_static_member {
                SymbolKind::Constant
            } else {
                SymbolKind::Field
            };

            // Build signature
            let signature = build_field_signature(base, node, &name);
            let visibility = extract_field_visibility(base, node);

            symbols.push(base.create_symbol(
                &node,
                name,
                kind,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(visibility),
                    parent_id: parent_id.map(String::from),
                    metadata: None,
                    doc_comment: doc_comment.clone(),
                },
            ));
        }

        return symbols;
    }

    // Get storage class and type specifiers once (shared by all declarators)
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);
    let is_constant = helpers::is_constant_declaration(&storage_class, &type_specifiers);
    let is_static_member = helpers::is_static_member_variable(node, &storage_class);

    // Handle ALL declarators (size_t rows, cols; extracts both rows and cols)
    let mut symbols = Vec::new();
    let doc_comment = base.find_doc_comment(&node);
    for declarator in declarators {
        // Extract field name from declarator (handles pointer_declarator, field_declarator, etc.)
        let name_node = match extract_field_name_from_declarator(declarator) {
            Some(n) => n,
            None => continue, // Skip if we can't find the name
        };

        let name = base.get_node_text(&name_node);

        let kind = if is_constant || is_static_member {
            SymbolKind::Constant
        } else {
            SymbolKind::Field
        };

        // Build signature
        let signature = build_field_signature(base, node, &name);
        let visibility = extract_field_visibility(base, node);

        symbols.push(base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: doc_comment.clone(),
            },
        ));
    }

    symbols
}

/// Extract field name from various declarator types
/// Handles: field_declarator, pointer_declarator, init_declarator, etc.
fn extract_field_name_from_declarator(declarator: Node) -> Option<Node> {
    // For pointer_declarator: need to recursively find field_identifier
    if declarator.kind() == "pointer_declarator" {
        return declarator.children(&mut declarator.walk()).find_map(|c| {
            if c.kind() == "field_identifier" || c.kind() == "identifier" {
                Some(c)
            } else if matches!(c.kind(), "pointer_declarator" | "field_declarator") {
                // Recursively search nested declarators (e.g., double**)
                extract_field_name_from_declarator(c)
            } else {
                None
            }
        });
    }

    // For field_declarator and init_declarator: direct children
    declarator
        .children(&mut declarator.walk())
        .find(|c| c.kind() == "field_identifier" || c.kind() == "identifier")
}

/// Extract friend declaration
pub(super) fn extract_friend_declaration(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();

    // Look for the inner declaration node
    let inner_declaration = node
        .children(&mut cursor)
        .find(|c| c.kind() == "declaration")?;

    // Look for function_declarator in the declaration
    let function_declarator = helpers::find_function_declarator_in_node(inner_declaration)?;

    // Extract name - handle both operator_name and regular identifier
    let (name, symbol_kind) = if let Some(operator_name) = function_declarator
        .children(&mut function_declarator.walk())
        .find(|c| c.kind() == "operator_name")
    {
        // This is a friend operator
        (base.get_node_text(&operator_name), SymbolKind::Operator)
    } else if let Some(identifier) = function_declarator
        .children(&mut function_declarator.walk())
        .find(|c| c.kind() == "identifier")
    {
        // This is a friend function
        (base.get_node_text(&identifier), SymbolKind::Function)
    } else {
        return None;
    };

    // Build friend signature
    let return_type = functions::extract_basic_return_type(base, inner_declaration);
    let parameters = functions::extract_function_parameters(base, function_declarator);

    let signature = format!("friend {} {}{}", return_type, name, parameters)
        .trim()
        .to_string();

    // Extract doc comment
    let doc_comment = base.find_doc_comment(&node);

    // Create the symbol
    let symbol = base.create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    );

    Some(symbol)
}

// Helper functions

fn extract_conversion_operator(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Find the operator_cast node
    let operator_cast = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "operator_cast")?;

    // Extract the target type from operator_cast
    let mut operator_name = "operator".to_string();

    let mut cursor = operator_cast.walk();
    for child in operator_cast.children(&mut cursor) {
        if matches!(
            child.kind(),
            "primitive_type" | "type_identifier" | "qualified_identifier"
        ) {
            let target_type = base.get_node_text(&child);
            operator_name.push(' ');
            operator_name.push_str(&target_type);
            break;
        }
    }

    let signature = base.get_node_text(&node);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        operator_name,
        SymbolKind::Operator,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

fn extract_destructor_from_declaration(
    base: &mut BaseExtractor,
    node: Node,
    _func_declarator: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let signature = base.get_node_text(&node);
    let name_start = signature.find('~')?;
    let name_end = signature[name_start..].find('(').map(|i| name_start + i)?;
    // SAFETY: Check char boundaries before slicing to prevent UTF-8 panic
    if !signature.is_char_boundary(name_start) || !signature.is_char_boundary(name_end) {
        return None;
    }
    let name = signature[name_start..name_end].to_string();

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Destructor,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

fn extract_constructor_from_declaration(
    base: &mut BaseExtractor,
    node: Node,
    func_declarator: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = functions::extract_function_name(func_declarator)?;
    let name = base.get_node_text(&name_node);

    // Build signature
    let mut signature = String::new();

    // Add modifiers
    let modifiers = functions::extract_function_modifiers(base, node);
    if !modifiers.is_empty() {
        signature.push_str(&modifiers.join(" "));
        signature.push(' ');
    }

    // Add constructor name and parameters
    signature.push_str(&name);
    let parameters = functions::extract_function_parameters(base, func_declarator);
    signature.push_str(&parameters);

    // Check for noexcept
    let noexcept_spec = functions::extract_noexcept_specifier(base, func_declarator);
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
                signature.push_str(&format!(" = {}", base.get_node_text(next_child)));
                break;
            }
        }
    }

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Constructor,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

fn build_direct_variable_signature(base: &mut BaseExtractor, node: Node, name: &str) -> String {
    let mut signature = String::new();

    // Add storage class
    let storage_class = helpers::extract_storage_class(base, node);
    if !storage_class.is_empty() {
        signature.push_str(&storage_class.join(" "));
        signature.push(' ');
    }

    // Add type specifiers
    let type_specifiers = helpers::extract_type_specifiers(base, node);
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
            signature.push_str(&base.get_node_text(&child));
            signature.push(' ');
            break;
        }
    }

    signature.push_str(name);
    signature
}

fn build_variable_signature(base: &mut BaseExtractor, node: Node, name: &str) -> String {
    let mut signature = String::new();

    // Add storage class and type specifiers
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);

    let mut parts = Vec::new();
    parts.extend(storage_class);
    parts.extend(type_specifiers);

    // Add type from node
    for child in node.children(&mut node.walk()) {
        if matches!(
            child.kind(),
            "primitive_type" | "type_identifier" | "qualified_identifier"
        ) {
            parts.push(base.get_node_text(&child));
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

fn build_field_signature(base: &mut BaseExtractor, node: Node, name: &str) -> String {
    let mut signature = String::new();

    // Add storage class and type specifiers
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);

    let mut parts = Vec::new();
    parts.extend(storage_class);
    parts.extend(type_specifiers);

    // Add type from node
    for child in node.children(&mut node.walk()) {
        if matches!(
            child.kind(),
            "primitive_type" | "type_identifier" | "qualified_identifier"
        ) {
            parts.push(base.get_node_text(&child));
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

/// Extract visibility for any C++ member (field, method, constructor, etc.)
/// This is the main public interface for visibility extraction
pub(super) fn extract_cpp_visibility(base: &mut BaseExtractor, node: Node) -> Visibility {
    extract_field_visibility(base, node)
}

fn extract_visibility_from_node(base: &mut BaseExtractor, node: Node) -> Visibility {
    // Same logic as extract_field_visibility - access specifiers apply to all members
    extract_field_visibility(base, node)
}

fn extract_field_visibility(base: &mut BaseExtractor, node: Node) -> Visibility {
    // C++ visibility is determined by the most recent access_spec node in the class body
    // Walk up to find the parent class/struct, then walk through siblings backwards

    // First, find the parent class/struct/union specifier
    let parent = find_parent_class_or_struct(node);

    match parent {
        Some((parent_kind, parent_node)) => {
            // Determine default visibility based on parent type
            let default_visibility = if parent_kind == "class_specifier" {
                Visibility::Private // class defaults to private
            } else {
                Visibility::Public // struct and union default to public
            };

            // Find the field_list body
            let field_list = parent_node
                .children(&mut parent_node.walk())
                .find(|c| c.kind() == "field_declaration_list");

            if let Some(field_list) = field_list {
                // Walk through field_list children to find the most recent access_spec before our node
                find_access_spec_before_node(base, field_list, node, default_visibility)
            } else {
                default_visibility
            }
        }
        None => Visibility::Public, // Not inside a class/struct, assume public
    }
}

/// Find the parent class_specifier or struct_specifier node
fn find_parent_class_or_struct(mut node: Node) -> Option<(&'static str, Node)> {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "class_specifier" => return Some(("class_specifier", parent)),
            "struct_specifier" => return Some(("struct_specifier", parent)),
            "union_specifier" => return Some(("union_specifier", parent)),
            _ => node = parent,
        }
    }
    None
}

/// Find the most recent access_spec before the target node
fn find_access_spec_before_node(
    base: &BaseExtractor,
    field_list: Node,
    target: Node,
    default_visibility: Visibility,
) -> Visibility {
    let target_start = target.start_position();
    let mut current_visibility = default_visibility;

    // Walk through all children of field_list
    for child in field_list.children(&mut field_list.walk()) {
        let child_pos = child.start_position();

        // If we've passed the target node, return the last visibility we saw
        if child_pos >= target_start {
            break;
        }

        // Check if this is an access_specifier node (private, protected, public keywords)
        // Note: tree-sitter uses "access_specifier" not "access_spec"
        if child.kind() == "access_specifier" {
            let spec_text = base.get_node_text(&child);
            current_visibility = match spec_text.trim() {
                "private" => Visibility::Private,
                "protected" => Visibility::Protected,
                "public" => Visibility::Public,
                _ => current_visibility, // Unknown, keep current
            };
        }
    }

    current_visibility
}
