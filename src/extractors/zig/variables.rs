use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use regex::Regex;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract variable and const declarations, including compound declarations
/// (generic type constructors, struct/union/enum assignments, error sets, function types)
pub(super) fn extract_variable(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    is_public_fn: fn(&BaseExtractor, Node) -> bool,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let name = base.get_node_text(&name_node);
    let is_const =
        node.kind() == "const_declaration" || base.get_node_text(&node).contains("const");
    let is_public = is_public_fn(base, node);

    let node_text = base.get_node_text(&node);

    // Check for generic type constructor
    if node_text.contains("(comptime") && node_text.contains("= struct") {
        return extract_generic_type_constructor(base, node, name, parent_id, is_public);
    }

    // Check for struct declaration
    let struct_node = base.find_child_by_type(&node, "struct_declaration");
    if struct_node.is_some() {
        return extract_struct_assignment(base, node, name, parent_id, is_public, &node_text);
    }

    // Check for union declaration
    let union_node = base.find_child_by_type(&node, "union_declaration");
    if union_node.is_some() {
        return extract_union_assignment(base, node, name, parent_id, is_public, &node_text);
    }

    // Check for enum declaration
    let enum_node = base.find_child_by_type(&node, "enum_declaration");
    if enum_node.is_some() {
        return extract_enum_assignment(base, node, name, parent_id, is_public, &node_text);
    }

    // Check for error set or error union declaration
    if node_text.contains("error{") || node_text.contains("error {") {
        return extract_error_set_assignment(base, node, name, parent_id, is_public, &node_text);
    }

    // Check for function type declaration
    if node_text.contains("fn (") || node_text.contains("fn(") {
        return extract_function_type_assignment(
            base, node, name, parent_id, is_public, &node_text,
        );
    }

    // Standard variable/constant extraction
    extract_standard_variable(base, node, name, parent_id, is_public, is_const)
}

fn extract_generic_type_constructor(
    base: &mut BaseExtractor,
    node: Node,
    name: String,
    parent_id: Option<&String>,
    is_public: bool,
) -> Option<Symbol> {
    let node_text = base.get_node_text(&node);
    let param_match = Regex::new(r"\(([^)]+)\)").unwrap().find(&node_text);
    let params = if let Some(param_match) = param_match {
        param_match.as_str()
    } else {
        "(comptime T: type)"
    };

    let signature = format!("fn {}({}) type", name, &params[1..params.len() - 1]);
    let visibility = if is_public {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let metadata = Some({
        let mut meta = HashMap::new();
        meta.insert(
            "isGenericTypeConstructor".to_string(),
            serde_json::Value::Bool(true),
        );
        meta
    });

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}

fn extract_struct_assignment(
    base: &mut BaseExtractor,
    node: Node,
    name: String,
    parent_id: Option<&String>,
    is_public: bool,
    node_text: &str,
) -> Option<Symbol> {
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

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}

fn extract_union_assignment(
    base: &mut BaseExtractor,
    node: Node,
    name: String,
    parent_id: Option<&String>,
    is_public: bool,
    node_text: &str,
) -> Option<Symbol> {
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

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}

fn extract_enum_assignment(
    base: &mut BaseExtractor,
    node: Node,
    name: String,
    parent_id: Option<&String>,
    is_public: bool,
    node_text: &str,
) -> Option<Symbol> {
    let enum_match = Regex::new(r"enum\(([^)]+)\)").unwrap().find(node_text);
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

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}

fn extract_error_set_assignment(
    base: &mut BaseExtractor,
    node: Node,
    name: String,
    parent_id: Option<&String>,
    is_public: bool,
    node_text: &str,
) -> Option<Symbol> {
    let mut signature = format!("const {} = ", name);

    if node_text.contains("||") {
        let union_match = Regex::new(r"error\s*\{[^}]*\}\s*\|\|\s*(\w+)")
            .unwrap()
            .captures(node_text);
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

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}

fn extract_function_type_assignment(
    base: &mut BaseExtractor,
    node: Node,
    name: String,
    parent_id: Option<&String>,
    is_public: bool,
    node_text: &str,
) -> Option<Symbol> {
    let fn_type_match = Regex::new(r"=\s*(fn\s*\([^}]*\).*?)(?:;|$)")
        .unwrap()
        .captures(node_text);
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

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Interface,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}

fn extract_standard_variable(
    base: &mut BaseExtractor,
    node: Node,
    name: String,
    parent_id: Option<&String>,
    is_public: bool,
    is_const: bool,
) -> Option<Symbol> {
    // Extract type if available, or detect switch expressions
    let type_node = base.find_child_by_type(&node, "type_expression");
    let switch_node = base.find_child_by_type(&node, "switch_expression");

    let mut var_type = if let Some(type_node) = type_node {
        base.get_node_text(&type_node)
    } else {
        "inferred".to_string()
    };

    // For type aliases, extract the assignment value
    if var_type == "inferred" && is_const {
        let node_text = base.get_node_text(&node);
        let assignment_match = Regex::new(r"=\s*([^;]+)").unwrap().captures(&node_text);
        if let Some(assignment_match) = assignment_match {
            var_type = assignment_match[1].trim().to_string();
        }
    }

    // If it contains a switch expression, include that in the signature
    if let Some(switch_node) = switch_node {
        let switch_text = base.get_node_text(&switch_node);
        // Safely truncate UTF-8 string at character boundary
        if switch_text.chars().count() > 20 {
            var_type = format!(
                "switch({}...)",
                BaseExtractor::truncate_string(&switch_text, 20)
            );
        } else {
            var_type = switch_text;
        }
    }

    let symbol_kind = if is_const {
        SymbolKind::Constant
    } else {
        SymbolKind::Variable
    };

    let signature = format!(
        "{} {}: {}",
        if is_const { "const" } else { "var" },
        name,
        var_type
    );
    let visibility = if is_public {
        Visibility::Public
    } else {
        Visibility::Private
    };

    Some(base.create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}
