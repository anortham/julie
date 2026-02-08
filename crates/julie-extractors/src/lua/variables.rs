/// Variable and assignment extraction
///
/// Handles extraction of:
/// - Local variable declarations: `local x = 5`
/// - Variable assignments: `x = 5`
/// - Assignment statements: `x, y = 1, 2`
/// - Property assignments: `obj.prop = value`
/// - Module property assignments: `M.PI = 3.14159`
use super::helpers;
use super::tables;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Collect non-comma children from an expression_list node.
fn collect_expression_nodes<'a>(expr_list: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = expr_list.walk();
    expr_list
        .children(&mut cursor)
        .filter(|child| child.kind() != ",")
        .collect()
}

/// Infer SymbolKind and data type from an expression node.
///
/// `is_field` controls whether function definitions become Method (true) or Function (false).
/// Returns (kind, data_type) where kind is the override (if any) and data_type is the
/// inferred type string.
fn infer_kind_and_type(
    base: &BaseExtractor,
    expression: Node,
    is_field: bool,
) -> (SymbolKind, String) {
    match expression.kind() {
        "function_definition" | "function" | "function_expression" => {
            let kind = if is_field {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            (kind, "function".to_string())
        }
        "expression_list" => {
            if helpers::contains_function_definition(expression) {
                let kind = if is_field {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                (kind, "function".to_string())
            } else {
                let data_type = helpers::infer_type_from_expression(base, expression);
                let kind = if data_type == "import" {
                    SymbolKind::Import
                } else if is_field {
                    SymbolKind::Field
                } else {
                    SymbolKind::Variable
                };
                (kind, data_type)
            }
        }
        _ => {
            let data_type = helpers::infer_type_from_expression(base, expression);
            let kind = if data_type == "import" {
                SymbolKind::Import
            } else if is_field {
                SymbolKind::Field
            } else {
                SymbolKind::Variable
            };
            (kind, data_type)
        }
    }
}

/// Resolve dot-notation name (e.g., "M.PI") into property name and parent symbol ID.
///
/// Returns Some((property_name, parent_id)) for valid two-part dot notation,
/// or None if the name doesn't contain a dot or has more than two parts.
fn resolve_dot_property(name: &str, symbols: &[Symbol]) -> Option<(String, Option<String>)> {
    if !name.contains('.') {
        return None;
    }
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() != 2 {
        return None;
    }
    let object_name = parts[0];
    let property_name = parts[1];
    let parent_id = symbols
        .iter()
        .find(|s| s.name == object_name)
        .map(|s| s.id.clone());
    Some((property_name.to_string(), parent_id))
}

/// Build metadata HashMap with dataType and create + push a symbol.
///
/// If the expression is a table constructor, also extracts table fields as children.
fn push_variable_symbol(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    name_node: &Node,
    name: String,
    kind: SymbolKind,
    data_type: String,
    signature: String,
    parent_id: Option<String>,
    visibility: Visibility,
    doc_comment: Option<String>,
    expression: Option<&Node>,
) {
    let mut metadata = HashMap::new();
    metadata.insert("dataType".to_string(), data_type.into());

    let options = SymbolOptions {
        signature: Some(signature),
        parent_id,
        visibility: Some(visibility),
        metadata: Some(metadata),
        doc_comment,
    };

    let symbol = base.create_symbol(name_node, name, kind, options);
    symbols.push(symbol);

    // If the expression is a table, extract its fields with this symbol as parent
    if let Some(expr) = expression {
        if expr.kind() == "table_constructor" || expr.kind() == "table" {
            let parent_id = symbols.last().unwrap().id.clone();
            tables::extract_table_fields(symbols, base, *expr, Some(&parent_id));
        }
    }
}

/// Extract local variable declarations: `local x = 5` or `local x, y = 1, 2`
pub(super) fn extract_local_variable_declaration(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let assignment_statement = helpers::find_child_by_type(node, "assignment_statement")?;
    let variable_list = helpers::find_child_by_type(assignment_statement, "variable_list")?;
    let expression_list = helpers::find_child_by_type(assignment_statement, "expression_list");

    let signature = base.get_node_text(&node);
    let mut cursor = variable_list.walk();
    let variables: Vec<Node> = variable_list
        .children(&mut cursor)
        .filter(|child| child.kind() == "variable" || child.kind() == "identifier")
        .collect();

    let expressions: Vec<Node> = expression_list
        .map(collect_expression_nodes)
        .unwrap_or_default();

    for (i, var_node) in variables.iter().enumerate() {
        let name_node = if var_node.kind() == "identifier" {
            Some(*var_node)
        } else if var_node.kind() == "variable" {
            helpers::find_child_by_type(*var_node, "identifier")
        } else {
            None
        };

        if let Some(name_node) = name_node {
            let name = base.get_node_text(&name_node);
            let expression = expressions.get(i);

            let (kind, data_type) = expression
                .map(|expr| infer_kind_and_type(base, *expr, false))
                .unwrap_or((SymbolKind::Variable, String::new()));

            let doc_comment = base.find_doc_comment(&node);

            push_variable_symbol(
                symbols,
                base,
                &name_node,
                name,
                kind,
                data_type,
                signature.clone(),
                parent_id.map(|s| s.to_string()),
                Visibility::Private,
                doc_comment,
                expression,
            );
        }
    }

    None
}

/// Extract assignment statements: `x = 5` or `x, y = 1, 2`
pub(super) fn extract_assignment_statement(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    if children.len() < 3 {
        return None;
    }

    let left = children[0];
    let right = children[2]; // Skip the '=' operator

    // Handle variable_list assignments
    if left.kind() == "variable_list" {
        let mut left_cursor = left.walk();
        let variables: Vec<Node> = left
            .children(&mut left_cursor)
            .filter(|child| {
                child.kind() == "variable"
                    || child.kind() == "identifier"
                    || child.kind() == "dot_index_expression"
            })
            .collect();

        for (i, var_node) in variables.iter().enumerate() {
            let name_node = if var_node.kind() == "identifier" {
                *var_node
            } else if var_node.kind() == "dot_index_expression" {
                *var_node
            } else {
                helpers::find_child_by_type(*var_node, "identifier")?
            };

            let name = base.get_node_text(&name_node);
            let signature = base.get_node_text(&node);

            // Resolve dot notation (e.g., M.PI = 3.14159)
            let (actual_name, parent_symbol_id, is_field) =
                if var_node.kind() == "dot_index_expression" {
                    if let Some((prop_name, prop_parent_id)) =
                        resolve_dot_property(&name, symbols)
                    {
                        (prop_name, prop_parent_id, true)
                    } else {
                        (name, None, false)
                    }
                } else {
                    (name, None, false)
                };

            // Determine kind and type from the right-hand side
            let (kind, data_type) = if right.kind() == "expression_list" {
                let expressions = collect_expression_nodes(right);
                if let Some(expression) = expressions.get(i) {
                    infer_kind_and_type(base, *expression, is_field)
                } else {
                    (
                        if is_field {
                            SymbolKind::Field
                        } else {
                            SymbolKind::Variable
                        },
                        String::new(),
                    )
                }
            } else {
                infer_kind_and_type(base, right, is_field)
            };

            let doc_comment = base.find_doc_comment(&node);

            push_variable_symbol(
                symbols,
                base,
                &name_node,
                actual_name,
                kind,
                data_type,
                signature,
                parent_symbol_id,
                Visibility::Public,
                doc_comment,
                None, // extract_assignment_statement doesn't extract table fields
            );
        }
    }
    // Handle simple identifier assignments and dot notation
    else if left.kind() == "variable" {
        let full_variable_name = base.get_node_text(&left);

        if let Some((property_name, property_parent_id)) =
            resolve_dot_property(&full_variable_name, symbols)
        {
            // Dot notation assignment: M.PI = 3.14159
            let (kind, data_type) = infer_kind_and_type(base, right, true);
            let signature = base.get_node_text(&node);
            let doc_comment = base.find_doc_comment(&node);

            push_variable_symbol(
                symbols,
                base,
                &left,
                property_name,
                kind,
                data_type,
                signature,
                property_parent_id,
                Visibility::Public,
                doc_comment,
                None,
            );
        } else if let Some(name_node) = helpers::find_child_by_type(left, "identifier") {
            // Simple identifier assignment: PI = 3.14159
            let name = base.get_node_text(&name_node);
            let (kind, data_type) = infer_kind_and_type(base, right, false);
            let signature = base.get_node_text(&node);
            let doc_comment = base.find_doc_comment(&node);

            push_variable_symbol(
                symbols,
                base,
                &name_node,
                name,
                kind,
                data_type,
                signature,
                parent_id.map(|s| s.to_string()),
                Visibility::Public,
                doc_comment,
                None,
            );
        }
    }

    None
}

/// Extract variable assignments: `PI = 3.14159` or similar global assignments
pub(super) fn extract_variable_assignment(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let variable_list = helpers::find_child_by_type(node, "variable_list")?;
    let expression_list = helpers::find_child_by_type(node, "expression_list");

    let signature = base.get_node_text(&node);
    let mut var_cursor = variable_list.walk();
    let variables: Vec<Node> = variable_list
        .children(&mut var_cursor)
        .filter(|child| child.kind() == "variable")
        .collect();

    let expressions: Vec<Node> = expression_list
        .map(collect_expression_nodes)
        .unwrap_or_default();

    for (i, var_node) in variables.iter().enumerate() {
        let full_variable_name = base.get_node_text(var_node);
        let expression = expressions.get(i);

        if let Some((property_name, property_parent_id)) =
            resolve_dot_property(&full_variable_name, symbols)
        {
            // Dot notation: M.PI = 3.14159
            let (kind, data_type) = expression
                .map(|expr| infer_kind_and_type(base, *expr, true))
                .unwrap_or((SymbolKind::Field, String::new()));

            push_variable_symbol(
                symbols,
                base,
                var_node,
                property_name,
                kind,
                data_type,
                signature.clone(),
                property_parent_id,
                Visibility::Public,
                None, // doc_comment handled by create_symbol fallback
                expression,
            );
        } else if let Some(name_node) = helpers::find_child_by_type(*var_node, "identifier") {
            // Simple variable: PI = 3.14159
            let name = base.get_node_text(&name_node);

            let (kind, data_type) = expression
                .map(|expr| infer_kind_and_type(base, *expr, false))
                .unwrap_or((SymbolKind::Variable, String::new()));

            push_variable_symbol(
                symbols,
                base,
                &name_node,
                name,
                kind,
                data_type,
                signature.clone(),
                parent_id.map(|s| s.to_string()),
                Visibility::Public,
                None, // doc_comment handled by create_symbol fallback
                expression,
            );
        }
    }

    None
}
