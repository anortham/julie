// R Relationship Extraction
// Extracts relationships between R symbols: function calls, library usage, pipes

use crate::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::r::RExtractor;
use tree_sitter::{Node, Tree};

/// Extract all relationships from R code
pub(super) fn extract_relationships(
    extractor: &mut RExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    extract_call_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    extract_pipe_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    extract_member_access_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

/// Extract function call relationships
fn extract_call_relationships(
    extractor: &mut RExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // R function calls are represented as "call" nodes
    if node.kind() == "call" {
        // The function being called is the first child
        if let Some(function_node) = node.child(0) {
            let function_name = match function_node.kind() {
                "identifier" => extractor.base.get_node_text(&function_node),
                "namespace_operator" => {
                    // Handle package::function syntax
                    if let Some(function_child) = function_node.child(2) {
                        extractor.base.get_node_text(&function_child)
                    } else {
                        extractor.base.get_node_text(&function_node)
                    }
                }
                "extract_operator" => {
                    // Handle $ operator: object$method()
                    if let Some(member) = function_node.child(2) {
                        extractor.base.get_node_text(&member)
                    } else {
                        extractor.base.get_node_text(&function_node)
                    }
                }
                _ => extractor.base.get_node_text(&function_node),
            };

            // Find the containing function (caller)
            if let Some(caller_symbol) = find_containing_function(node, symbols) {
                // Find the called function symbol (might be user-defined or built-in)
                if let Some(called_symbol) = symbols
                    .iter()
                    .find(|s| s.name == function_name && s.kind == SymbolKind::Function)
                {
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
                        file_path: extractor.base.file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 1.0,
                        metadata: None,
                    };
                    relationships.push(relationship);
                } else if is_builtin_function(&function_name) {
                    // Built-in function call - create relationship with builtin_ prefix
                    // (same as old behavior, to maintain backward compatibility with tests)
                    let relationship = Relationship {
                        id: format!(
                            "{}_{:?}_{}_{}",
                            caller_symbol.id,
                            RelationshipKind::Calls,
                            function_name,
                            node.start_position().row
                        ),
                        from_symbol_id: caller_symbol.id.clone(),
                        to_symbol_id: format!("builtin_{}", function_name),
                        kind: RelationshipKind::Calls,
                        file_path: extractor.base.file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 0.8,
                        metadata: None,
                    };
                    relationships.push(relationship);
                } else {
                    // Unknown function call - create PendingRelationship for cross-file resolution
                    let pending = PendingRelationship {
                        from_symbol_id: caller_symbol.id.clone(),
                        callee_name: function_name.clone(),
                        kind: RelationshipKind::Calls,
                        file_path: extractor.base.file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 0.7,
                    };
                    extractor.add_pending_relationship(pending);
                }
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_call_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract pipe operator relationships (%>%, |>, etc.)
fn extract_pipe_relationships(
    extractor: &mut RExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Pipe operators in R are binary operators
    if node.kind() == "binary_operator" {
        if let Some(operator) = node.child(1) {
            let op_text = extractor.base.get_node_text(&operator);

            // Check if this is a pipe operator
            if op_text == "%>%" || op_text == "|>" {
                // The right side of the pipe is typically a function call
                if let Some(right_child) = node.child(2) {
                    if right_child.kind() == "call" {
                        // Extract the function being called
                        if let Some(function_node) = right_child.child(0) {
                            let function_name = extractor.base.get_node_text(&function_node);

                            // Find containing function
                            if let Some(containing_symbol) = find_containing_function(node, symbols)
                            {
                                // Create a relationship for the piped call
                                let relationship = Relationship {
                                    id: format!(
                                        "{}_{:?}_{}_{}",
                                        containing_symbol.id,
                                        RelationshipKind::Calls,
                                        function_name,
                                        node.start_position().row
                                    ),
                                    from_symbol_id: containing_symbol.id.clone(),
                                    to_symbol_id: format!("piped_{}", function_name),
                                    kind: RelationshipKind::Calls,
                                    file_path: extractor.base.file_path.clone(),
                                    line_number: (node.start_position().row + 1) as u32,
                                    confidence: 0.9,
                                    metadata: None,
                                };
                                relationships.push(relationship);
                            }
                        }
                    }
                }
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_pipe_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract member access relationships ($ operator)
fn extract_member_access_relationships(
    extractor: &mut RExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // R uses extract_operator for $ and @
    if node.kind() == "extract_operator" {
        // The member being accessed is the third child (index 2)
        if let Some(member_node) = node.child(2) {
            let member_name = extractor.base.get_node_text(&member_node);

            // Find containing function
            if let Some(containing_symbol) = find_containing_function(node, symbols) {
                // Create a Uses relationship for member access
                let relationship = Relationship {
                    id: format!(
                        "{}_{:?}_{}_{}",
                        containing_symbol.id,
                        RelationshipKind::Uses,
                        member_name,
                        node.start_position().row
                    ),
                    from_symbol_id: containing_symbol.id.clone(),
                    to_symbol_id: format!("member_{}", member_name),
                    kind: RelationshipKind::Uses,
                    file_path: extractor.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 0.8,
                    metadata: None,
                };
                relationships.push(relationship);
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_member_access_relationships(extractor, child, symbols, relationships);
    }
}

/// Find the containing function for a node
fn find_containing_function<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        // R function definitions are inside binary_operator nodes
        if parent.kind() == "binary_operator" {
            // Check if the right side is a function_definition
            if let Some(right_child) = parent.child(2) {
                if right_child.kind() == "function_definition" {
                    // Find the symbol that matches this function
                    let func_line = parent.start_position().row + 1;
                    if let Some(symbol) = symbols
                        .iter()
                        .find(|s| (s.kind == SymbolKind::Function || s.kind == SymbolKind::Method) && s.start_line == func_line as u32)
                    {
                        return Some(symbol);
                    }
                }
            }
        }
        current = parent;
    }
    None
}

/// Check if a function name is a built-in R function
/// Built-in functions should not create pending relationships (they're known to be in base R)
fn is_builtin_function(name: &str) -> bool {
    matches!(
        name,
        // Core R functions
        "c" | "list" | "data.frame" | "matrix" | "array" | "factor" | "as.numeric" |
        "as.character" | "as.logical" | "as.integer" | "as.data.frame" | "as.matrix" |
        "as.list" | "as.vector" | "length" | "names" | "dim" | "nrow" | "ncol" | "class" |
        "typeof" | "is.na" | "is.null" | "is.numeric" | "is.character" | "is.logical" |
        // Math functions
        "mean" | "median" | "sum" | "min" | "max" | "abs" | "sqrt" | "exp" | "log" | "log10" |
        "sin" | "cos" | "tan" | "round" | "floor" | "ceiling" | "trunc" | "sign" |
        // Statistics functions
        "sd" | "var" | "cov" | "cor" | "lm" | "glm" | "summary" | "quantile" | "range" |
        // I/O functions
        "print" | "cat" | "paste" | "paste0" | "sprintf" | "format" | "write" | "read.csv" |
        "read.table" | "write.csv" | "write.table" | "readLines" | "writeLines" |
        // Control flow
        "if" | "else" | "for" | "while" | "repeat" | "break" | "next" | "return" | "stop" |
        "warning" | "message" | "invisible" | "do.call" | "lapply" | "sapply" | "mapply" |
        "tapply" | "Reduce" | "Filter" | "Map" | "Vectorize" |
        // Common utility functions
        "seq" | "rep" | "sort" | "order" | "rank" | "unique" | "duplicated" | "which" |
        "head" | "tail" | "str" | "View" | "rm" | "ls" | "exists" | "all" | "any" |
        "subset" | "merge" | "rbind" | "cbind" | "t" | "apply" |
        // String functions
        "nchar" | "substr" | "substring" | "strsplit" | "tolower" | "toupper" | "trimws" |
        "grep" | "grepl" | "sub" | "gsub" | "match" | "pmatch" | "charmatch" |
        // Type checking and conversion
        "is.data.frame" | "is.matrix" | "is.array" | "is.factor" | "is.ordered" |
        "is.function" | "is.list" | "is.atomic" | "is.recursive" |
        // Operators
        "+" | "-" | "*" | "/" | "^" | "%%" | "%/%" | "%*%" | ":" | "~" |
        // Base functions
        "Sys.time" | "Sys.Date" | "system" | "system2" | "shell" | "getwd" | "setwd" |
        "list.files" | "dir" | "file.exists" | "file.create" | "file.remove" | "file.rename" |
        "dir.create" | "tempdir" | "tempfile" | "library" | "require" | "source" |
        "eval" | "parse" | "deparse" | "substitute" | "quote" | "expression" |
        // Environment/Scope
        "parent.frame" | "parent.env" | "environment" | "new.env" | "with" | "within" |
        "attach" | "detach" | "search" | "get" | "assign" | "remove" |
        // Common functions from tidyverse-like operations
        "filter" | "select" | "mutate" | "arrange" | "group_by" | "summarize" | "summarise" |
        "join" | "left_join" | "right_join" | "inner_join" | "full_join" | "ggplot" | "aes"
    )
}
