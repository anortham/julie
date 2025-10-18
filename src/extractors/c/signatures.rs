//! Signature building for various C constructs
//!
//! This module provides methods to construct readable signatures for functions, variables,
//! structs, enums, and typedefs.

use super::helpers;
use super::types;
use crate::extractors::base::BaseExtractor;

/// Build function signature: "return_type function_name(params)"
pub(super) fn build_function_signature(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    let storage_class = types::extract_storage_class(base, node);
    let return_type = types::extract_return_type(base, node);
    let function_name = helpers::extract_function_name(base, node);
    let parameters = extract_function_parameters(base, node);

    let storage_prefix = if let Some(sc) = storage_class {
        format!("{} ", sc)
    } else {
        String::new()
    };

    format!(
        "{}{} {}({})",
        storage_prefix,
        return_type,
        function_name,
        parameters.join(", ")
    )
}

/// Build function declaration signature
pub(super) fn build_function_declaration_signature(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> String {
    let return_type = types::extract_return_type(base, node);
    let function_name = helpers::extract_function_name_from_declaration(base, node);
    let parameters = extract_function_parameters_from_declaration(base, node);

    format!(
        "{} {}({})",
        return_type,
        function_name,
        parameters.join(", ")
    )
}

/// Build variable signature: "[storage] [qualifiers] type name[array] [= initializer]"
pub(super) fn build_variable_signature(
    base: &BaseExtractor,
    node: tree_sitter::Node,
    declarator: tree_sitter::Node,
) -> String {
    let storage_class = types::extract_storage_class(base, node);
    let type_qualifiers = types::extract_type_qualifiers(base, node);
    let data_type = types::extract_variable_type(base, node);
    let variable_name = helpers::extract_variable_name(base, declarator);
    let array_spec = types::extract_array_specifier(base, declarator);
    let initializer = types::extract_initializer(base, declarator);

    let mut signature = String::new();
    if let Some(sc) = storage_class {
        signature.push_str(&format!("{} ", sc));
    }
    if let Some(tq) = type_qualifiers {
        signature.push_str(&format!("{} ", tq));
    }
    signature.push_str(&format!("{} {}", data_type, variable_name));
    if let Some(arr) = array_spec {
        signature.push_str(&arr);
    }
    if let Some(init) = initializer {
        signature.push_str(&format!(" = {}", init));
    }

    signature
}

/// Build struct signature: "struct Name { field_type field_name; ... }"
pub(super) fn build_struct_signature(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    let struct_name = helpers::extract_struct_name(base, node);
    let fields = extract_struct_fields(base, node);
    let attributes = types::extract_struct_attributes(base, node);
    let alignment_attrs = types::extract_alignment_attributes(base, node);

    let mut signature = String::new();

    // Add alignment attributes before struct if they exist
    if !alignment_attrs.is_empty() {
        signature.push_str(&format!("{} ", alignment_attrs.join(" ")));
    }

    signature.push_str(&format!("struct {}", struct_name));

    if !fields.is_empty() {
        let field_signatures: Vec<String> = fields
            .iter()
            .take(3)
            .map(|f| format!("{} {}", f.field_type, f.name))
            .collect();
        signature.push_str(&format!(" {{ {} }}", field_signatures.join("; ")));
    }

    // Add other attributes after the struct definition
    if !attributes.is_empty() {
        signature.push_str(&format!(" {}", attributes.join(" ")));
    }

    signature
}

/// Build enum signature: "enum Name { value, value, ... }"
pub(super) fn build_enum_signature(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    let enum_name = helpers::extract_enum_name(base, node);
    let values = extract_enum_values(base, node);

    let mut signature = format!("enum {}", enum_name);
    if !values.is_empty() {
        let value_names: Vec<String> = values.iter().take(3).map(|v| v.name.clone()).collect();
        signature.push_str(&format!(" {{ {} }}", value_names.join(", ")));
    }

    signature
}

/// Build typedef signature with attributes
pub(super) fn build_typedef_signature(
    base: &BaseExtractor,
    node: &tree_sitter::Node,
    identifier_name: &str,
) -> String {
    let node_text = base.get_node_text(node);

    // Look for various attributes in the node text and parent context
    let mut attributes = Vec::new();
    let mut context_text = node_text.clone();

    // If this is an expression_statement (like "AtomicCounter;"), look at parent context
    if node.kind() == "expression_statement" && node_text.trim().ends_with(';') {
        if let Some(parent) = node.parent() {
            context_text = base.get_node_text(&parent);

            // Also check grandparent if needed
            if !context_text.contains("ALIGN(") && !context_text.contains("PACKED") {
                if let Some(grandparent) = parent.parent() {
                    let grandparent_text = base.get_node_text(&grandparent);
                    context_text = grandparent_text;
                }
            }
        }
    }

    // Check for PACKED attribute
    if context_text.contains("PACKED") {
        attributes.push("PACKED".to_string());
    }

    // Check for ALIGN attribute - find the specific usage for this struct
    let struct_pattern = "typedef struct ALIGN(".to_string();
    if let Some(struct_start) = context_text.find(&struct_pattern) {
        let align_start = struct_start + "typedef struct ".len();
        if let Some(align_end) = context_text[align_start..].find(')') {
            let align_attr = &context_text[align_start..align_start + align_end + 1];
            // Only add if this looks like the specific alignment for our struct
            if context_text[align_start + align_end + 1..].contains(identifier_name) {
                attributes.push(align_attr.to_string());
            }
        }
    }

    // Fallback: look for any ALIGN attribute if we didn't find the specific one
    if attributes.is_empty() {
        if let Some(align_start) = context_text.find("ALIGN(") {
            if let Some(align_end) = context_text[align_start..].find(')') {
                let align_attr = &context_text[align_start..align_start + align_end + 1];
                // Skip generic macro definitions like "ALIGN(n)"
                if !align_attr.contains("n)") && !align_attr.contains("...") {
                    attributes.push(align_attr.to_string());
                }
            }
        }
    }

    // Build signature based on pattern in context_text
    if !attributes.is_empty() {
        format!(
            "typedef struct {} {}",
            attributes.join(" "),
            identifier_name
        )
    } else {
        format!("typedef struct {}", identifier_name)
    }
}

/// Extract parameters from a function declarator
pub(super) fn extract_parameters_from_declarator(
    base: &BaseExtractor,
    declarator: tree_sitter::Node,
) -> Vec<String> {
    let mut parameters = Vec::new();

    if let Some(param_list) = declarator.child_by_field_name("parameters") {
        let mut cursor = param_list.walk();
        for child in param_list.children(&mut cursor) {
            match child.kind() {
                "parameter_declaration" => {
                    parameters.push(base.get_node_text(&child));
                }
                "variadic_parameter" => {
                    parameters.push("...".to_string());
                }
                _ => {}
            }
        }
    }

    parameters
}

/// Extract parameters from a function definition
pub(super) fn extract_function_parameters(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Vec<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_declarator" {
            return extract_parameters_from_declarator(base, child);
        }
        if child.kind() == "pointer_declarator" {
            let mut pointer_cursor = child.walk();
            for pointer_child in child.children(&mut pointer_cursor) {
                if pointer_child.kind() == "function_declarator" {
                    return extract_parameters_from_declarator(base, pointer_child);
                }
            }
        }
    }
    Vec::new()
}

/// Extract parameters from a function declaration
pub(super) fn extract_function_parameters_from_declaration(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Vec<String> {
    if let Some(function_declarator) = helpers::find_function_declarator(node) {
        return extract_parameters_from_declarator(base, function_declarator);
    }
    Vec::new()
}

/// Extract struct fields
pub(super) fn extract_struct_fields(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Vec<StructField> {
    let mut fields = Vec::new();

    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "field_declaration" {
                let field_type = types::extract_variable_type(base, child);
                let declarators = helpers::find_variable_declarators(child);

                for declarator in declarators {
                    let field_name = helpers::extract_variable_name(base, declarator);
                    fields.push(StructField {
                        name: field_name,
                        field_type: field_type.clone(),
                    });
                }
            }
        }
    }

    fields
}

/// Extract enum values
pub(super) fn extract_enum_values(base: &BaseExtractor, node: tree_sitter::Node) -> Vec<EnumValue> {
    let mut values = Vec::new();

    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "enumerator" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = base.get_node_text(&name_node);
                    let value = child
                        .child_by_field_name("value")
                        .map(|v| base.get_node_text(&v));

                    values.push(EnumValue { name, value });
                }
            }
        }
    }

    values
}

// Helper structs (re-exported from declarations)
#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub field_type: String,
}

#[derive(Debug, Clone)]
pub struct EnumValue {
    pub name: String,
    #[allow(dead_code)]
    pub value: Option<String>,
}
