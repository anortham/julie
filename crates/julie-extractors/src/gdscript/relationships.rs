//! Relationship extraction for GDScript
//! Handles function call relationships (including cross-file pending relationships)

use super::super::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex,
    StructuredPendingRelationship, Symbol, SymbolKind, UnresolvedTarget,
};
use super::GDScriptExtractor;
use tree_sitter::{Node, Tree};

/// Extract relationships from GDScript code
pub(super) fn extract_relationships(
    extractor: &mut GDScriptExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let scoped_index = ScopedSymbolIndex::new(symbols);

    extract_metadata_inheritance_relationships(extractor, symbols, &mut relationships);

    // Recursively visit all nodes to extract relationships
    visit_node_for_relationships(
        extractor,
        tree.root_node(),
        symbols,
        &scoped_index,
        &mut relationships,
    );

    relationships
}

fn extract_metadata_inheritance_relationships(
    extractor: &mut GDScriptExtractor,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    for class_symbol in symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Class)
    {
        let Some(base_class) = class_symbol
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("baseClass"))
            .and_then(|value| value.as_str())
            .filter(|base_class| !base_class.is_empty())
        else {
            continue;
        };

        if is_builtin_gdscript_base_class(base_class) {
            continue;
        }

        if let Some(base_symbol) = symbols.iter().find(|symbol| {
            symbol.id != class_symbol.id
                && symbol.name == base_class
                && symbol.kind == SymbolKind::Class
        }) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    class_symbol.id,
                    base_symbol.id,
                    RelationshipKind::Extends,
                    class_symbol.start_line
                ),
                from_symbol_id: class_symbol.id.clone(),
                to_symbol_id: base_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: extractor.base.file_path.clone(),
                line_number: class_symbol.start_line,
                confidence: 0.95,
                metadata: None,
            });
        } else {
            let pending = StructuredPendingRelationship::new(
                class_symbol.id.clone(),
                UnresolvedTarget::simple(base_class.to_string()),
                Some(class_symbol.id.clone()),
                RelationshipKind::Extends,
                extractor.base.file_path.clone(),
                class_symbol.start_line,
                0.8,
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

fn is_builtin_gdscript_base_class(name: &str) -> bool {
    matches!(
        name,
        "Object"
            | "RefCounted"
            | "Resource"
            | "Node"
            | "Node2D"
            | "Node3D"
            | "Control"
            | "CanvasItem"
            | "CanvasLayer"
            | "Area2D"
            | "Area3D"
            | "CharacterBody2D"
            | "CharacterBody3D"
            | "RigidBody2D"
            | "RigidBody3D"
            | "StaticBody2D"
            | "StaticBody3D"
            | "Sprite2D"
            | "Sprite3D"
            | "Camera2D"
            | "Camera3D"
            | "Label"
            | "Button"
            | "Panel"
            | "AnimationPlayer"
            | "AudioStreamPlayer"
            | "Timer"
    )
}

/// Visit a node and extract relationships from it
fn visit_node_for_relationships(
    extractor: &mut GDScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "call" | "call_expression" => {
            extract_call_relationships(extractor, node, symbols, scoped_index, relationships);
        }
        "attribute" => {
            if attribute_has_call_suffix(&node) {
                extract_call_relationships(extractor, node, symbols, scoped_index, relationships);
            }
        }
        _ => {}
    }

    // Recursively visit all children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_node_for_relationships(extractor, child, symbols, scoped_index, relationships);
    }
}

/// Extract call relationships from a function call
fn extract_call_relationships(
    extractor: &mut GDScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    let base = &extractor.base;

    // For GDScript, a call node has the function name as the first child
    // The structure is: call -> (identifier | attribute) + arguments
    let target = extract_target_from_call(base, &node);
    let called_function_name = target.terminal_name.clone();

    if !called_function_name.is_empty() {
        if let Some(caller_symbol) = base
            .find_containing_symbol(&node, symbols)
            .filter(|symbol| matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method))
        {
            let line_number = (node.start_position().row + 1) as u32;
            let file_path = base.file_path.clone();

            // Check if we can resolve the callee locally
            match scoped_index.resolve_call_target(
                &called_function_name,
                Some(caller_symbol),
                target.receiver.as_deref(),
            ) {
                LocalTargetResolution::Resolved(called_symbol) => {
                    // Target is a local function/method - create resolved Relationship
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
                        file_path,
                        line_number,
                        confidence: 0.9,
                        metadata: None,
                    };

                    relationships.push(relationship);
                }
                LocalTargetResolution::Import(_)
                | LocalTargetResolution::Ambiguous
                | LocalTargetResolution::Missing
                | LocalTargetResolution::ReceiverQualified => {
                    // Target not found in local symbols - likely a method on imported type
                    // or a call to an external function
                    // Create PendingRelationship for cross-file resolution
                    let pending = base.create_pending_relationship(
                        caller_symbol.id.clone(),
                        target,
                        RelationshipKind::Calls,
                        &node,
                        Some(caller_symbol.id.clone()),
                        Some(0.7),
                    );
                    extractor.add_structured_pending_relationship(pending);
                }
            }
        }
    }
}

/// Extract unresolved target from a call node
fn extract_target_from_call(base: &crate::base::BaseExtractor, node: &Node) -> UnresolvedTarget {
    // For GDScript, we need to get the function name from the call structure
    // call -> identifier (for simple calls like func_name())
    // call -> attribute (for method calls like obj.method() or self.method())

    if node.kind() == "attribute" {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        if let Some(attribute_call) = children
            .iter()
            .find(|child| child.kind() == "attribute_call")
        {
            let mut call_cursor = attribute_call.walk();
            let call_children: Vec<Node> = attribute_call.children(&mut call_cursor).collect();
            if let Some(name_node) = call_children
                .iter()
                .find(|child| child.kind() == "identifier")
            {
                let terminal_name = base.get_node_text(name_node);
                let display_name = base.get_node_text(node);
                let receiver = display_name
                    .rsplit_once('.')
                    .map(|(receiver, _)| receiver.to_string())
                    .or_else(|| {
                        children
                            .iter()
                            .find(|child| child.is_named() && child.kind() != "attribute_call")
                            .map(|child| base.get_node_text(child))
                    });

                if let Some(receiver) = receiver {
                    return UnresolvedTarget {
                        display_name,
                        terminal_name,
                        receiver: Some(receiver),
                        namespace_path: Vec::new(),
                        import_context: None,
                    };
                }

                return UnresolvedTarget::simple(terminal_name);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // Simple function call: func_name()
                return UnresolvedTarget::simple(base.get_node_text(&child));
            }
            "attribute" => {
                // Method call: obj.method() or self.method()
                // For an attribute node, the rightmost identifier is the member being accessed
                let mut attr_cursor = child.walk();
                let attr_children: Vec<Node> = child.children(&mut attr_cursor).collect();

                if let Some(attribute_call) = attr_children
                    .iter()
                    .find(|attr_child| attr_child.kind() == "attribute_call")
                {
                    let mut call_cursor = attribute_call.walk();
                    if let Some(name_node) = attribute_call
                        .children(&mut call_cursor)
                        .find(|call_child| call_child.kind() == "identifier")
                    {
                        let terminal_name = base.get_node_text(&name_node);
                        let attr_text = base.get_node_text(&child);
                        if let Some(receiver) = attr_text
                            .rsplit_once('.')
                            .map(|(receiver, _)| receiver.to_string())
                        {
                            return UnresolvedTarget {
                                display_name: attr_text,
                                terminal_name,
                                receiver: Some(receiver),
                                namespace_path: Vec::new(),
                                import_context: None,
                            };
                        }
                        return UnresolvedTarget::simple(terminal_name);
                    }
                }

                // The last identifier in the attribute is the method name
                if let Some(last_child) = attr_children.last() {
                    if last_child.kind() == "identifier" {
                        let terminal_name = base.get_node_text(last_child);
                        let attr_text = base.get_node_text(&child);
                        if let Some((receiver, _)) = attr_text.rsplit_once('.') {
                            let receiver = receiver.to_string();
                            return UnresolvedTarget {
                                display_name: attr_text,
                                terminal_name,
                                receiver: Some(receiver),
                                namespace_path: Vec::new(),
                                import_context: None,
                            };
                        }
                        return UnresolvedTarget::simple(terminal_name);
                    }
                }

                // Fallback: try to extract from attribute text
                let attr_text = base.get_node_text(&child);
                if let Some(last_dot) = attr_text.rfind('.') {
                    let terminal_name = attr_text[last_dot + 1..].to_string();
                    return UnresolvedTarget {
                        display_name: attr_text.clone(),
                        terminal_name,
                        receiver: Some(attr_text[..last_dot].to_string()),
                        namespace_path: Vec::new(),
                        import_context: None,
                    };
                }
                return UnresolvedTarget::simple(attr_text);
            }
            _ => {}
        }
    }

    UnresolvedTarget::simple(String::new())
}

fn attribute_has_call_suffix(node: &Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| child.kind() == "attribute_call")
}
