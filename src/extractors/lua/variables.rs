use super::helpers;
use super::tables;
/// Variable and assignment extraction
///
/// Handles extraction of:
/// - Local variable declarations: `local x = 5`
/// - Variable assignments: `x = 5`
/// - Assignment statements: `x, y = 1, 2`
/// - Property assignments: `obj.prop = value`
/// - Module property assignments: `M.PI = 3.14159`
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract local variable declarations: `local x = 5` or `local x, y = 1, 2`
pub(super) fn extract_local_variable_declaration(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Get the assignment_statement child first
    let assignment_statement = helpers::find_child_by_type(node, "assignment_statement")?;

    // Now get variable_list and expression_list from assignment_statement
    let variable_list = helpers::find_child_by_type(assignment_statement, "variable_list")?;
    let expression_list = helpers::find_child_by_type(assignment_statement, "expression_list");

    let signature = base.get_node_text(&node);
    let mut cursor = variable_list.walk();
    let variables: Vec<Node> = variable_list
        .children(&mut cursor)
        .filter(|child| child.kind() == "variable" || child.kind() == "identifier")
        .collect();

    // Get the corresponding expressions if they exist
    let expressions: Vec<Node> = if let Some(expr_list) = expression_list {
        let mut expr_cursor = expr_list.walk();
        expr_list
            .children(&mut expr_cursor)
            .filter(|child| child.kind() != ",") // Filter out commas
            .collect()
    } else {
        Vec::new()
    };

    // Create symbols for each local variable
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

            // Check if the corresponding expression is a function or import
            let expression = expressions.get(i);
            let mut kind = SymbolKind::Variable;
            let mut data_type = "unknown".to_string();

            if let Some(expression) = expression {
                match expression.kind() {
                    "function_definition" | "function" | "function_expression" => {
                        kind = SymbolKind::Function;
                        data_type = "function".to_string();
                    }
                    "expression_list" => {
                        // Check if expression_list contains a function_definition (for anonymous functions)
                        if helpers::contains_function_definition(*expression) {
                            kind = SymbolKind::Function;
                            data_type = "function".to_string();
                        } else {
                            data_type = helpers::infer_type_from_expression(base, *expression);
                            if data_type == "import" {
                                kind = SymbolKind::Import;
                            }
                        }
                    }
                    _ => {
                        data_type = helpers::infer_type_from_expression(base, *expression);
                        if data_type == "import" {
                            kind = SymbolKind::Import;
                        }
                    }
                }
            }

            let mut metadata = HashMap::new();
            metadata.insert("dataType".to_string(), data_type.clone().into());

            // Extract LuaDoc comment
            let doc_comment = base.find_doc_comment(&node);

            let options = SymbolOptions {
                signature: Some(signature.clone()),
                parent_id: parent_id.map(|s| s.to_string()),
                visibility: Some(Visibility::Private),
                metadata: Some(metadata),
                doc_comment,
            };

            let mut symbol = base.create_symbol(&name_node, name, kind, options);

            // Set dataType as direct property for tests (matching pattern)
            if let Some(ref mut metadata) = symbol.metadata {
                metadata.insert("dataType".to_string(), data_type.into());
            } else {
                let mut metadata = HashMap::new();
                metadata.insert("dataType".to_string(), data_type.into());
                symbol.metadata = Some(metadata);
            }

            symbols.push(symbol.clone());

            // If this is a table, extract its fields with this symbol as parent
            if let Some(expression) = expression {
                if expression.kind() == "table_constructor" || expression.kind() == "table" {
                    let parent_id = symbols.last().unwrap().id.clone();
                    tables::extract_table_fields(symbols, base, *expression, Some(&parent_id));
                }
            }
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
            // Handle "variable" nodes, direct "identifier" nodes, and "dot_index_expression" nodes
            let name_node = if var_node.kind() == "identifier" {
                *var_node
            } else if var_node.kind() == "dot_index_expression" {
                // Handle dot notation assignments like M.PI = 3.14159
                *var_node
            } else {
                helpers::find_child_by_type(*var_node, "identifier")?
            };

            let name = base.get_node_text(&name_node);
            let signature = base.get_node_text(&node);

            // Handle dot notation assignments like M.PI = 3.14159
            let (actual_name, parent_symbol_id, kind_override) =
                if var_node.kind() == "dot_index_expression" && name.contains('.') {
                    let parts: Vec<&str> = name.split('.').collect();
                    if parts.len() == 2 {
                        let object_name = parts[0];
                        let property_name = parts[1];

                        // Find the parent object
                        let parent_id = symbols
                            .iter()
                            .find(|s| s.name == object_name)
                            .map(|s| s.id.clone());

                        (
                            property_name.to_string(),
                            parent_id,
                            Some(SymbolKind::Field),
                        )
                    } else {
                        (name, None, None)
                    }
                } else {
                    (name, None, None)
                };

            // Determine kind and type based on the assignment
            let is_field_assignment = matches!(kind_override, Some(SymbolKind::Field));
            let mut kind = kind_override.unwrap_or(SymbolKind::Variable);
            let mut data_type = "unknown".to_string();

            if right.kind() == "expression_list" {
                let mut right_cursor = right.walk();
                let expressions: Vec<Node> = right
                    .children(&mut right_cursor)
                    .filter(|child| child.kind() != ",")
                    .collect();

                if let Some(expression) = expressions.get(i) {
                    if expression.kind() == "function_definition" {
                        // Override kind based on context
                        kind = if is_field_assignment {
                            SymbolKind::Method // Function assigned to object property = Method
                        } else {
                            SymbolKind::Function
                        };
                        data_type = "function".to_string();
                    } else {
                        data_type = helpers::infer_type_from_expression(base, *expression);
                    }
                }
            } else if right.kind() == "function_definition" {
                kind = SymbolKind::Function;
                data_type = "function".to_string();
            } else {
                data_type = helpers::infer_type_from_expression(base, right);
                // Update kind based on inferred type
                if data_type == "import" {
                    kind = SymbolKind::Import;
                }
            }

            let mut metadata = HashMap::new();
            metadata.insert("dataType".to_string(), data_type.clone().into());

            // Extract LuaDoc comment
            let doc_comment = base.find_doc_comment(&node);

            let options = SymbolOptions {
                signature: Some(signature),
                parent_id: parent_symbol_id,
                visibility: Some(Visibility::Public),
                metadata: Some(metadata),
                doc_comment,
            };

            let symbol = base.create_symbol(&name_node, actual_name, kind, options);
            symbols.push(symbol);
        }
    }
    // Handle simple identifier assignments and dot notation
    else if left.kind() == "variable" {
        let full_variable_name = base.get_node_text(&left);

        // Handle dot notation assignments: M.PI = 3.14159
        if full_variable_name.contains('.') {
            let parts: Vec<&str> = full_variable_name.split('.').collect();
            if parts.len() == 2 {
                let object_name = parts[0];
                let property_name = parts[1];
                let signature = base.get_node_text(&node);

                // Determine kind and type based on the assignment
                let mut kind = SymbolKind::Field; // Property assignments are fields
                let data_type = if right.kind() == "function_definition" {
                    kind = SymbolKind::Method; // Methods on objects
                    "function".to_string()
                } else {
                    helpers::infer_type_from_expression(base, right)
                };

                // Find the object this property belongs to
                let property_parent_id = symbols
                    .iter()
                    .find(|s| s.name == object_name)
                    .map(|s| s.id.clone());

                let mut metadata = HashMap::new();
                metadata.insert("dataType".to_string(), data_type.clone().into());

                // Extract LuaDoc comment
                let doc_comment = base.find_doc_comment(&node);

                let options = SymbolOptions {
                    signature: Some(signature),
                    parent_id: property_parent_id,
                    visibility: Some(Visibility::Public),
                    metadata: Some(metadata),
                    doc_comment,
                };

                let symbol = base.create_symbol(&left, property_name.to_string(), kind, options);
                symbols.push(symbol);
            }
        }
        // Handle simple identifier assignments: PI = 3.14159
        else if let Some(name_node) = helpers::find_child_by_type(left, "identifier") {
            let name = base.get_node_text(&name_node);
            let signature = base.get_node_text(&node);

            // Determine kind and type based on the assignment
            let mut kind = SymbolKind::Variable;
            let data_type = if right.kind() == "function_definition" {
                kind = SymbolKind::Function;
                "function".to_string()
            } else {
                helpers::infer_type_from_expression(base, right)
            };

            let mut metadata = HashMap::new();
            metadata.insert("dataType".to_string(), data_type.clone().into());

            // Extract LuaDoc comment
            let doc_comment = base.find_doc_comment(&node);

            let options = SymbolOptions {
                signature: Some(signature),
                parent_id: parent_id.map(|s| s.to_string()),
                visibility: Some(Visibility::Public), // Global assignments are public
                metadata: Some(metadata),
                doc_comment,
            };

            let symbol = base.create_symbol(&name_node, name, kind, options);
            symbols.push(symbol);
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
    // Extract global variable assignments like: PI = 3.14159
    let variable_list = helpers::find_child_by_type(node, "variable_list")?;
    let expression_list = helpers::find_child_by_type(node, "expression_list");

    let signature = base.get_node_text(&node);
    let mut var_cursor = variable_list.walk();
    let variables: Vec<Node> = variable_list
        .children(&mut var_cursor)
        .filter(|child| child.kind() == "variable")
        .collect();

    let expressions: Vec<Node> = if let Some(expr_list) = expression_list {
        let mut expr_cursor = expr_list.walk();
        expr_list
            .children(&mut expr_cursor)
            .filter(|child| child.kind() != ",") // Filter out commas
            .collect()
    } else {
        Vec::new()
    };

    // Create symbols for each variable
    for (i, var_node) in variables.iter().enumerate() {
        let full_variable_name = base.get_node_text(var_node);

        // Handle dot notation: M.PI = 3.14159
        if full_variable_name.contains('.') {
            let parts: Vec<&str> = full_variable_name.split('.').collect();
            if parts.len() == 2 {
                let object_name = parts[0];
                let property_name = parts[1];

                // Determine kind and type based on the assignment
                // Module properties (M.PI) should be classified as Field
                let mut kind = SymbolKind::Field;
                let mut data_type = "unknown".to_string();

                if let Some(expression) = expressions.get(i) {
                    if expression.kind() == "function_definition" {
                        kind = SymbolKind::Method; // Module methods should be Method, not Function
                        data_type = "function".to_string();
                    } else {
                        data_type = helpers::infer_type_from_expression(base, *expression);
                    }
                }

                // Find the object this property belongs to
                let property_parent_id = symbols
                    .iter()
                    .find(|s| s.name == object_name)
                    .map(|s| s.id.clone());

                let mut metadata = HashMap::new();
                metadata.insert("dataType".to_string(), data_type.clone().into());

                let options = SymbolOptions {
                    signature: Some(signature.clone()),
                    parent_id: property_parent_id,
                    visibility: Some(Visibility::Public),
                    metadata: Some(metadata),
                    ..Default::default()
                };

                let symbol = base.create_symbol(var_node, property_name.to_string(), kind, options);
                symbols.push(symbol);

                // If this is a table, extract its fields with this symbol as parent
                if let Some(expression) = expressions.get(i) {
                    if expression.kind() == "table_constructor" || expression.kind() == "table" {
                        let parent_id = symbols.last().unwrap().id.clone();
                        tables::extract_table_fields(symbols, base, *expression, Some(&parent_id));
                    }
                }
            }
        }
        // Handle simple variable: PI = 3.14159
        else if let Some(name_node) = helpers::find_child_by_type(*var_node, "identifier") {
            let name = base.get_node_text(&name_node);

            // Determine kind and type based on the assignment
            let mut kind = SymbolKind::Variable;
            let mut data_type = "unknown".to_string();

            if let Some(expression) = expressions.get(i) {
                if expression.kind() == "function_definition" {
                    kind = SymbolKind::Function;
                    data_type = "function".to_string();
                } else {
                    data_type = helpers::infer_type_from_expression(base, *expression);
                }
            }

            let mut metadata = HashMap::new();
            metadata.insert("dataType".to_string(), data_type.clone().into());

            let options = SymbolOptions {
                signature: Some(signature.clone()),
                parent_id: parent_id.map(|s| s.to_string()),
                visibility: Some(Visibility::Public), // Global variables are public
                metadata: Some(metadata),
                ..Default::default()
            };

            let symbol = base.create_symbol(&name_node, name, kind, options);
            symbols.push(symbol);

            // If this is a table, extract its fields with this symbol as parent
            if let Some(expression) = expressions.get(i) {
                if expression.kind() == "table_constructor" || expression.kind() == "table" {
                    let parent_id = symbols.last().unwrap().id.clone();
                    tables::extract_table_fields(symbols, base, *expression, Some(&parent_id));
                }
            }
        }
    }

    None
}
