// PHP Extractor - Relationship extraction (inheritance, implementation, function calls)

use super::{PhpExtractor, find_child};
use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind, UnresolvedTarget};
use std::collections::HashMap;
use tree_sitter::Node;

/// Strip PHP namespace prefix from a qualified name, returning the last component.
/// e.g. `\App\Http\Controller` -> `Controller`, `Controller` -> `Controller`
fn strip_php_namespace(name: &str) -> &str {
    name.trim_start_matches('\\')
        .rsplit('\\')
        .next()
        .unwrap_or(name)
}

/// Extract class inheritance and implementation relationships
pub(super) fn extract_class_relationships(
    extractor: &mut PhpExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let class_symbol = find_class_symbol(extractor, node, symbols);
    if class_symbol.is_none() {
        return;
    }
    let class_symbol = class_symbol.unwrap();

    // Inheritance relationships
    if let Some(extends_node) = find_child(extractor, &node, "base_clause") {
        let raw_name = extractor
            .get_base()
            .get_node_text(&extends_node)
            .replace("extends", "")
            .trim()
            .to_string();
        // Bug 4: strip namespace qualifier so `\App\Http\BaseController` -> `BaseController`
        let base_class_name = strip_php_namespace(&raw_name).to_string();

        // Try same-file resolution first
        if let Some(base_class_symbol) = symbols
            .iter()
            .find(|s| s.name == base_class_name && s.kind == SymbolKind::Class)
        {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    class_symbol.id,
                    base_class_symbol.id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: class_symbol.id.clone(),
                to_symbol_id: base_class_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: extractor.get_base().file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: 1.0,
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "baseClass".to_string(),
                        serde_json::Value::String(base_class_name),
                    );
                    metadata
                }),
            });
        } else {
            // Bug 2: base class not found in same file — emit PendingRelationship for cross-file resolution
            let pending = extractor.get_base().create_pending_relationship(
                class_symbol.id.clone(),
                UnresolvedTarget::simple(base_class_name),
                RelationshipKind::Extends,
                &node,
                Some(class_symbol.id.clone()),
                Some(0.9),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }

    // Implementation relationships
    if let Some(implements_node) = find_child(extractor, &node, "class_interface_clause") {
        let interface_names: Vec<String> = extractor
            .get_base()
            .get_node_text(&implements_node)
            .replace("implements", "")
            .split(',')
            // Bug 4: strip namespace qualifiers from interface names
            .map(|name| strip_php_namespace(name.trim()).to_string())
            .filter(|name| !name.is_empty())
            .collect();

        for interface_name in interface_names {
            // Bug 3: removed same-file filter — search all in-scope symbols
            let interface_symbol = symbols
                .iter()
                .find(|s| s.name == interface_name && s.kind == SymbolKind::Interface);

            if let Some(iface) = interface_symbol {
                // Same-file interface found: create a resolved Relationship
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol.id,
                        iface.id,
                        RelationshipKind::Implements,
                        node.start_position().row
                    ),
                    from_symbol_id: class_symbol.id.clone(),
                    to_symbol_id: iface.id.clone(),
                    kind: RelationshipKind::Implements,
                    file_path: extractor.get_base().file_path.clone(),
                    line_number: node.start_position().row as u32 + 1,
                    confidence: 1.0,
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "interface".to_string(),
                            serde_json::Value::String(interface_name),
                        );
                        metadata
                    }),
                });
            } else {
                // Bug 3: interface not found in symbols — emit PendingRelationship instead of fabricating an ID
                let pending = extractor.get_base().create_pending_relationship(
                    class_symbol.id.clone(),
                    UnresolvedTarget::simple(interface_name),
                    RelationshipKind::Implements,
                    &node,
                    Some(class_symbol.id.clone()),
                    Some(0.9),
                );
                extractor.add_structured_pending_relationship(pending);
            }
        }
    }
}

/// Extract interface inheritance relationships
pub(super) fn extract_interface_relationships(
    extractor: &mut PhpExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let interface_symbol = find_interface_symbol(extractor, node, symbols);
    if interface_symbol.is_none() {
        return;
    }
    let interface_symbol = interface_symbol.unwrap();

    // Interface inheritance
    if let Some(extends_node) = find_child(extractor, &node, "base_clause") {
        let base_interface_names: Vec<String> = extractor
            .get_base()
            .get_node_text(&extends_node)
            .replace("extends", "")
            .split(',')
            // Bug 4: strip namespace qualifiers
            .map(|name| strip_php_namespace(name.trim()).to_string())
            .filter(|name| !name.is_empty())
            .collect();

        for base_interface_name in base_interface_names {
            // Try to find the base interface in current symbols
            let base_symbol = symbols
                .iter()
                .find(|s| s.name == base_interface_name && s.kind == SymbolKind::Interface);

            if let Some(base) = base_symbol {
                // Same-file interface found: create resolved Relationship
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        interface_symbol.id,
                        base.id,
                        RelationshipKind::Extends,
                        node.start_position().row
                    ),
                    from_symbol_id: interface_symbol.id.clone(),
                    to_symbol_id: base.id.clone(),
                    kind: RelationshipKind::Extends,
                    file_path: extractor.get_base().file_path.clone(),
                    line_number: node.start_position().row as u32 + 1,
                    confidence: 1.0,
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "baseInterface".to_string(),
                            serde_json::Value::String(base_interface_name),
                        );
                        metadata
                    }),
                });
            } else {
                // Cross-file: emit PendingRelationship instead of fabricating an ID
                let pending = extractor.get_base().create_pending_relationship(
                    interface_symbol.id.clone(),
                    UnresolvedTarget::simple(base_interface_name),
                    RelationshipKind::Extends,
                    &node,
                    Some(interface_symbol.id.clone()),
                    Some(0.9),
                );
                extractor.add_structured_pending_relationship(pending);
            }
        }
    }
}

/// Find class symbol by node
pub(super) fn find_class_symbol<'a>(
    extractor: &PhpExtractor,
    node: Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = extractor.get_base().get_node_text(&name_node);

    symbols.iter().find(|s| {
        s.name == name
            && s.kind == SymbolKind::Class
            && s.file_path == extractor.get_base().file_path
    })
}

/// Find interface symbol by node
pub(super) fn find_interface_symbol<'a>(
    extractor: &PhpExtractor,
    node: Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = extractor.get_base().get_node_text(&name_node);

    symbols.iter().find(|s| {
        s.name == name
            && s.kind == SymbolKind::Interface
            && s.file_path == extractor.get_base().file_path
    })
}

/// Extract function and method call relationships
pub(super) fn extract_call_relationships(
    extractor: &mut PhpExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base();

    // For function calls and method calls, extract the function/method being called
    let called_function_name = match node.kind() {
        "function_call_expression" => {
            // Function call: foo()
            if let Some(name_node) = node.child_by_field_name("function") {
                base.get_node_text(&name_node)
            } else {
                return;
            }
        }
        "member_call_expression" => {
            // Method call: $obj->method() - uses "name" field not "member"
            if let Some(name_node) = node.child_by_field_name("name") {
                base.get_node_text(&name_node)
            } else {
                return;
            }
        }
        "scoped_call_expression" => {
            // Static method call: Class::method()
            if let Some(name_node) = node.child_by_field_name("name") {
                base.get_node_text(&name_node)
            } else {
                return;
            }
        }
        "object_creation_expression" => {
            // Bug 1: new ClassName() — extract the class name being instantiated.
            // Tree structure: (object_creation_expression (name) ...) or (qualified_name)
            // The class name is the first named child.
            if let Some(class_name_node) = node.named_child(0) {
                // Bug 4: strip namespace qualifier from the class name
                let raw = base.get_node_text(&class_name_node);
                strip_php_namespace(raw.trim()).to_string()
            } else {
                return;
            }
        }
        _ => return,
    };

    if called_function_name.is_empty() {
        return;
    }

    // Determine relationship kind: new Foo() is Instantiates, everything else is Calls
    let rel_kind = if node.kind() == "object_creation_expression" {
        RelationshipKind::Instantiates
    } else {
        RelationshipKind::Calls
    };

    // Find the enclosing function/method that contains this call
    if let Some(caller_symbol) = find_containing_function(extractor, node, symbols) {
        let line_number = (node.start_position().row + 1) as u32;
        let file_path = base.file_path.clone();

        // Create a symbol map for fast lookups
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.name.clone(), s)).collect();

        // Check if we can resolve the callee locally
        match symbol_map.get(&called_function_name) {
            // For Instantiates, reject non-type targets (a function or constant
            // sharing the same name is not a valid instantiation target)
            Some(called_symbol)
                if rel_kind == RelationshipKind::Instantiates
                    && !matches!(
                        called_symbol.kind,
                        SymbolKind::Class
                            | SymbolKind::Interface
                            | SymbolKind::Struct
                            | SymbolKind::Enum
                    ) =>
            {
                let pending = extractor.get_base().create_pending_relationship(
                    caller_symbol.id.clone(),
                    unresolved_call_target(extractor, node, &called_function_name),
                    rel_kind,
                    &node,
                    Some(caller_symbol.id.clone()),
                    Some(0.7),
                );
                extractor.add_structured_pending_relationship(pending);
            }
            Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
                // Target is an Import symbol - need cross-file resolution
                // Don't create relationship pointing to Import (useless for trace_call_path)
                // Instead, create a PendingRelationship with the callee name
                let pending = extractor.get_base().create_pending_relationship(
                    caller_symbol.id.clone(),
                    unresolved_call_target(extractor, node, &called_function_name),
                    rel_kind,
                    &node,
                    Some(caller_symbol.id.clone()),
                    Some(0.8),
                );
                extractor.add_structured_pending_relationship(pending);
            }
            Some(called_symbol) => {
                // Target is a local function/method - create resolved Relationship
                let relationship = Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        caller_symbol.id,
                        called_symbol.id,
                        rel_kind,
                        node.start_position().row
                    ),
                    from_symbol_id: caller_symbol.id.clone(),
                    to_symbol_id: called_symbol.id.clone(),
                    kind: rel_kind,
                    file_path,
                    line_number,
                    confidence: 0.9,
                    metadata: None,
                };

                relationships.push(relationship);
            }
            None => {
                // Target not found in local symbols - likely a method on imported type
                // Create PendingRelationship for cross-file resolution
                let pending = extractor.get_base().create_pending_relationship(
                    caller_symbol.id.clone(),
                    unresolved_call_target(extractor, node, &called_function_name),
                    rel_kind,
                    &node,
                    Some(caller_symbol.id.clone()),
                    Some(0.7),
                );
                extractor.add_structured_pending_relationship(pending);
            }
        }
    }
}

fn unresolved_call_target(
    extractor: &PhpExtractor,
    node: Node,
    fallback_name: &str,
) -> UnresolvedTarget {
    match node.kind() {
        "member_call_expression" => {
            let receiver = node
                .child_by_field_name("object")
                .map(|object| extractor.get_base().get_node_text(&object))
                .map(|name| name.trim_start_matches('$').to_string());
            let terminal_name = node
                .child_by_field_name("name")
                .map(|name| extractor.get_base().get_node_text(&name))
                .unwrap_or_else(|| fallback_name.to_string());
            let display_name = receiver
                .as_ref()
                .map(|receiver| format!("{receiver}.{terminal_name}"))
                .unwrap_or_else(|| terminal_name.clone());
            UnresolvedTarget {
                display_name,
                terminal_name,
                receiver,
                namespace_path: Vec::new(),
                import_context: None,
            }
        }
        _ => UnresolvedTarget::simple(fallback_name.to_string()),
    }
}

/// Find the containing function of a node
fn find_containing_function<'a>(
    extractor: &PhpExtractor,
    node: Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let base = extractor.get_base();

    // Walk up the tree to find the containing function or method
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "function_definition" || parent.kind() == "method_declaration" {
            // Found a function, extract its name
            if let Some(name_node) = parent.child_by_field_name("name") {
                let function_name = base.get_node_text(&name_node);
                let symbol_map: HashMap<String, &Symbol> =
                    symbols.iter().map(|s| (s.name.clone(), s)).collect();
                return symbol_map.get(&function_name).copied();
            }
        }
        current = parent;
    }
    None
}
