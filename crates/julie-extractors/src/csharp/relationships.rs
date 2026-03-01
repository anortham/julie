// C# Relationship Extraction

use crate::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::csharp::member_type_relationships::{
    extract_field_type_relationships, extract_parameter_type_name,
    extract_property_type_relationships, find_containing_class,
};
use crate::csharp::CSharpExtractor;
use tree_sitter::Tree;

/// Extract relationships from the tree
pub fn extract_relationships(
    extractor: &mut CSharpExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    visit_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

fn visit_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_declaration" | "interface_declaration" | "struct_declaration" => {
            extract_inheritance_relationships(extractor, node, symbols, relationships);
        }
        // TODO: C# 12 primary constructors (e.g. `class Foo(IBar bar) {}`) put params
        // on class_declaration, not constructor_declaration. Known gap for future work.
        "constructor_declaration" => {
            extract_constructor_parameter_relationships(extractor, node, symbols, relationships);
        }
        "field_declaration" => {
            extract_field_type_relationships(extractor, node, symbols, relationships);
        }
        "property_declaration" => {
            extract_property_type_relationships(extractor, node, symbols, relationships);
        }
        "invocation_expression" => {
            crate::csharp::di_relationships::extract_di_registration_relationships(
                extractor,
                node,
                symbols,
                relationships,
            );
            extract_call_relationships(extractor, node, symbols, relationships);
        }
        // In C#, method calls are represented as member_access_expression followed by argument_list
        "member_access_expression" => {
            // Check if this is followed by an argument_list (i.e., a method call)
            if let Some(sibling) = node.next_sibling() {
                if sibling.kind() == "argument_list" {
                    extract_call_relationships(extractor, node, symbols, relationships);
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_relationships(extractor, child, symbols, relationships);
    }
}

fn extract_inheritance_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Phase 1: Collect all data using immutable borrow of extractor
    let (current_symbol_id, base_types, file_path, line_number) = {
        let base = extractor.get_base();
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier");
        let Some(name_node) = name_node else { return };

        let current_symbol_name = base.get_node_text(&name_node);
        let Some(current_symbol) = symbols.iter().find(|s| s.name == current_symbol_name) else {
            return;
        };

        let base_list = node.children(&mut cursor).find(|c| c.kind() == "base_list");
        let Some(base_list) = base_list else { return };

        let mut base_cursor = base_list.walk();
        let base_types: Vec<String> = base_list
            .children(&mut base_cursor)
            .filter(|c| c.kind() != ":" && c.kind() != ",")
            .map(|c| base.get_node_text(&c))
            .collect();

        (
            current_symbol.id.clone(),
            base_types,
            base.file_path.clone(),
            (node.start_position().row + 1) as u32,
        )
    };

    // Phase 2: Create relationships (may need &mut extractor for pending)
    for base_type_name in base_types {
        if let Some(base_symbol) = symbols.iter().find(|s| s.name == base_type_name) {
            // Same-file: we know the target's kind, so we can resolve directly
            let relationship_kind = if base_symbol.kind == SymbolKind::Interface {
                RelationshipKind::Implements
            } else {
                RelationshipKind::Extends
            };

            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    current_symbol_id,
                    base_symbol.id,
                    relationship_kind,
                    node.start_position().row
                ),
                from_symbol_id: current_symbol_id.clone(),
                to_symbol_id: base_symbol.id.clone(),
                kind: relationship_kind,
                file_path: file_path.clone(),
                line_number,
                confidence: 1.0,
                metadata: None,
            });
        } else {
            // Cross-file: base type is defined in another file.
            // Use terminal identifier with C# naming convention (IFoo = interface)
            // so qualified names like Namespace.IFoo still infer Implements.
            let inferred_name = terminal_identifier(&base_type_name);
            let relationship_kind = if is_interface_name(inferred_name) {
                RelationshipKind::Implements
            } else {
                RelationshipKind::Extends
            };

            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: current_symbol_id.clone(),
                callee_name: base_type_name,
                kind: relationship_kind,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
        }
    }
}

/// Check if a type name follows C# interface naming convention (IFoo).
/// Requires 'I' prefix followed by an uppercase letter to avoid false positives
/// with regular names like "Item" or "Index".
fn is_interface_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!((chars.next(), chars.next()), (Some('I'), Some(c)) if c.is_ascii_uppercase())
}

fn terminal_identifier(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// Extract constructor parameter type relationships (DI injection pattern)
///
/// In C#/.NET, dependency injection via constructor parameters is THE primary
/// wiring mechanism. This function creates `Uses` relationships from the
/// containing class to each parameter type, enabling centrality scoring.
///
/// Handles:
/// - Simple types: `ILogger` -> identifier
/// - Generic types: `ILogger<MyService>` -> generic_name (extracts base name `ILogger`)
/// - Nullable types: `ILogger?` -> nullable_type (unwraps to inner type)
/// - Skips predefined types: `string`, `int`, `bool`, etc. (not interesting relationships)
fn extract_constructor_parameter_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Phase 1: Collect all data using immutable borrow of extractor
    // (we need base for get_node_text, but also need &mut extractor for pending_relationships)
    let (class_symbol_id, param_types) = {
        let base = extractor.get_base();

        let Some(class_symbol) = find_containing_class(base, node, symbols) else {
            return;
        };

        // Find the parameter_list child of the constructor
        let mut cursor = node.walk();
        let param_list = node
            .children(&mut cursor)
            .find(|c| c.kind() == "parameter_list");
        let Some(param_list) = param_list else { return };

        // Collect all parameter type names with their line numbers
        let mut types = Vec::new();
        let mut param_cursor = param_list.walk();
        for param in param_list
            .children(&mut param_cursor)
            .filter(|c| c.kind() == "parameter")
        {
            if let Some(type_name) = extract_parameter_type_name(base, param) {
                if !type_name.is_empty() {
                    let line_number = param.start_position().row as u32 + 1;
                    let row = param.start_position().row;
                    types.push((type_name, line_number, row));
                }
            }
        }

        (class_symbol.id.clone(), types)
    };
    // Phase 1 done — immutable borrow of extractor is dropped

    // Phase 2: Create relationships, deduplicating across constructor overloads.
    // A class only needs one Uses edge per type, regardless of how many constructors use it.
    let file_path = extractor.get_base().file_path.clone();
    let symbol_map: std::collections::HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    // Collect already-existing Uses targets for this class (from earlier constructors)
    let mut seen: std::collections::HashSet<String> = relationships
        .iter()
        .filter(|r| r.from_symbol_id == class_symbol_id && r.kind == RelationshipKind::Uses)
        .map(|r| r.to_symbol_id.clone())
        .collect();

    for (type_name, line_number, row) in param_types {
        if seen.contains(&type_name) {
            continue; // Already emitted (pending dedup by name)
        }
        match symbol_map.get(&type_name) {
            Some(type_symbol) if !seen.contains(&type_symbol.id) => {
                seen.insert(type_symbol.id.clone());
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol_id,
                        type_symbol.id,
                        RelationshipKind::Uses,
                        row
                    ),
                    from_symbol_id: class_symbol_id.clone(),
                    to_symbol_id: type_symbol.id.clone(),
                    kind: RelationshipKind::Uses,
                    file_path: file_path.clone(),
                    line_number,
                    confidence: 0.9,
                    metadata: None,
                });
            }
            Some(_) => {} // Already seen this resolved type
            None => {
                seen.insert(type_name.clone());
                extractor.add_pending_relationship(PendingRelationship {
                    from_symbol_id: class_symbol_id.clone(),
                    callee_name: type_name,
                    kind: RelationshipKind::Uses,
                    file_path: file_path.clone(),
                    line_number,
                    confidence: 0.8,
                });
            }
        }
    }
}

/// Extract method call relationships
///
/// Creates resolved Relationship when target is a local method.
/// Creates PendingRelationship when target is:
/// - An Import symbol (needs cross-file resolution)
/// - Not found in local symbol_map (e.g., method on imported type)
fn extract_call_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // In C#, method calls can be:
    // 1. Direct identifier call: Method()
    // 2. Member access call: Helper.Process()
    // The node can be either an invocation_expression or a member_access_expression

    let method_name = {
        let base = extractor.get_base();
        match node.kind() {
            "identifier" => base.get_node_text(&node),
            "member_access_expression" => {
                // For something like Helper.Process(), find the method name (last identifier)
                let mut method_cursor = node.walk();
                let children: Vec<_> = node.children(&mut method_cursor).collect();
                children
                    .iter()
                    .rev()
                    .find(|c| c.kind() == "identifier")
                    .map(|n| base.get_node_text(n))
                    .unwrap_or_default()
            }
            _ => {
                // For invocation_expression, get the first child which is the function/method
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                if let Some(first_child) = children.first() {
                    match first_child.kind() {
                        "identifier" => base.get_node_text(first_child),
                        "member_access_expression" => {
                            let mut method_cursor = first_child.walk();
                            let children: Vec<_> =
                                first_child.children(&mut method_cursor).collect();
                            children
                                .iter()
                                .rev()
                                .find(|c| c.kind() == "identifier")
                                .map(|n| base.get_node_text(n))
                                .unwrap_or_default()
                        }
                        _ => String::new(),
                    }
                } else {
                    String::new()
                }
            }
        }
    };

    if !method_name.is_empty() {
        handle_call_target(extractor, node, &method_name, symbols, relationships);
    }
}

/// Handle a call target - create Relationship or PendingRelationship based on target type
fn handle_call_target(
    extractor: &mut CSharpExtractor,
    call_node: tree_sitter::Node,
    callee_name: &str,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base();

    // Build a symbol_map for quick lookup
    let symbol_map: std::collections::HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    // Find the calling method context - look upward in the tree for parent method
    let mut parent = call_node.parent();
    let mut caller_symbol = None;
    while let Some(p) = parent {
        if p.kind() == "method_declaration" || p.kind() == "local_function_statement" {
            // Get the method name
            let mut p_cursor = p.walk();
            if let Some(name_node) = p.children(&mut p_cursor).find(|c| c.kind() == "identifier") {
                let method_name = base.get_node_text(&name_node);
                caller_symbol = symbol_map.get(&method_name).copied();
                break;
            }
        }
        parent = p.parent();
    }

    // No caller context means we can't create a meaningful relationship
    let Some(caller) = caller_symbol else {
        return;
    };

    let line_number = call_node.start_position().row as u32 + 1;
    let file_path = base.file_path.clone();

    // Check if we can resolve the callee locally
    match symbol_map.get(callee_name) {
        Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
            // Target is an Import symbol - need cross-file resolution
            // Don't create relationship pointing to Import (useless for trace_call_path)
            // Instead, create a PendingRelationship with the callee name
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: callee_name.to_string(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.8, // Lower confidence - needs resolution
            });
        }
        Some(called_symbol) => {
            // Target is a local method - create resolved Relationship
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    caller.id,
                    called_symbol.id,
                    RelationshipKind::Calls,
                    call_node.start_position().row
                ),
                from_symbol_id: caller.id.clone(),
                to_symbol_id: called_symbol.id.clone(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        }
        None => {
            // Target not found in local symbols - likely a method on imported type
            // Create PendingRelationship for cross-file resolution
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: callee_name.to_string(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.7, // Lower confidence - unknown target
            });
        }
    }
}
