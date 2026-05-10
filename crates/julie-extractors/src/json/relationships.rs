//! JSON Schema `$ref` relationship extraction (Phase 3.2).
//!
//! - **Local `$ref`** (`#/$defs/Address`) → concrete `Relationship` from the
//!   containing object's parent symbol to the resolved target symbol, kind
//!   `References`. If the JSON pointer cannot be resolved, no edge is emitted
//!   (the pointer is malformed, not "deferred to another file").
//! - **External `$ref`** (`<file>#/$defs/Address`) →
//!   `StructuredPendingRelationship` carrying
//!   `target.import_context = Some("<file>")`,
//!   `target.terminal_name` = last fragment segment,
//!   `target.namespace_path` = preceding fragment segments,
//!   `target.display_name` = original `$ref` text,
//!   `caller_scope_symbol_id` = containing object's parent symbol id.
//!
//! AST shape under tree-sitter-json: a `pair` whose first child is a string
//! key `"$ref"` and whose last child is a string value carrying the pointer.

use crate::base::{
    BaseExtractor, Relationship, RelationshipKind, StructuredPendingRelationship, Symbol,
    UnresolvedTarget,
};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) fn extract_relationships_internal(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if node.kind() == "pair" {
        if let Some((value_text, value_node)) = ref_pair_value(base, node) {
            handle_ref_pair(base, node, value_node, &value_text, symbols, relationships);
        }
    }
    for child in node.children(&mut node.walk()) {
        extract_relationships_internal(base, child, symbols, relationships);
    }
}

/// If the pair's key is the literal `"$ref"` and its value is a string,
/// return the unquoted value text plus the value node.
fn ref_pair_value<'a>(base: &BaseExtractor, pair: Node<'a>) -> Option<(String, Node<'a>)> {
    let mut cursor = pair.walk();
    let children: Vec<Node<'a>> = pair.children(&mut cursor).collect();
    if children.len() < 3 {
        return None;
    }
    let key_text = base.get_node_text(&children[0]);
    if key_text.trim_matches('"') != "$ref" {
        return None;
    }
    let value_node = *children.last()?;
    if value_node.kind() != "string" {
        return None;
    }
    let raw = base.get_node_text(&value_node);
    Some((raw.trim_matches('"').to_string(), value_node))
}

fn handle_ref_pair(
    base: &mut BaseExtractor,
    pair: Node,
    value_node: Node,
    value_text: &str,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Locate the symbol that represents the containing object's parent pair
    // (e.g., `billing` for `"billing": { "$ref": ... }`). The $ref pair's
    // immediate parent is an object node; that object's parent is the pair
    // whose symbol we want to be the relationship source / caller scope.
    let containing_pair = match pair
        .parent()
        .filter(|p| p.kind() == "object")
        .and_then(|obj| obj.parent())
        .filter(|p| p.kind() == "pair")
    {
        Some(p) => p,
        None => return,
    };
    let from_symbol = match symbol_for_pair(symbols, containing_pair) {
        Some(s) => s,
        None => return,
    };
    let line_number = value_node.start_position().row as u32 + 1;

    if let Some((file_part, fragment)) = split_external_ref(value_text) {
        emit_external_pending(
            base,
            &from_symbol,
            value_text,
            file_part,
            fragment,
            line_number,
        );
    } else if let Some(fragment) = value_text.strip_prefix("#/") {
        if let Some(target_symbol) = resolve_local_pointer(symbols, fragment) {
            emit_local_relationship(
                base,
                &from_symbol,
                target_symbol,
                value_node,
                line_number,
                relationships,
            );
        }
        // Unresolved local pointer = malformed; emit nothing.
    }
    // Other shapes (e.g., bare relative URIs, missing `#`) are out of scope.
}

fn split_external_ref(value: &str) -> Option<(&str, &str)> {
    let idx = value.find('#')?;
    if idx == 0 {
        return None; // pure local pointer
    }
    let (file, rest) = value.split_at(idx);
    // Skip the `#` itself; tolerate either `#/...` or `#...` shape.
    let fragment = rest.trim_start_matches('#').trim_start_matches('/');
    Some((file, fragment))
}

fn emit_external_pending(
    base: &mut BaseExtractor,
    from_symbol: &Symbol,
    raw_ref: &str,
    file_part: &str,
    fragment: &str,
    line_number: u32,
) {
    let segments: Vec<&str> = if fragment.is_empty() {
        Vec::new()
    } else {
        fragment.split('/').collect()
    };
    let (terminal_name, namespace_path) = match segments.as_slice() {
        [] => return,
        [name] => ((*name).to_string(), Vec::new()),
        _ => (
            segments
                .last()
                .expect("non-empty after match")
                .to_string(),
            segments[..segments.len() - 1]
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        ),
    };
    let target = UnresolvedTarget {
        display_name: raw_ref.to_string(),
        terminal_name,
        receiver: None,
        namespace_path,
        import_context: Some(file_part.to_string()),
    };
    let pending = StructuredPendingRelationship::new(
        from_symbol.id.clone(),
        target,
        Some(from_symbol.id.clone()),
        RelationshipKind::References,
        base.file_path.clone(),
        line_number,
        1.0,
    );
    base.add_structured_pending_relationship(pending);
}

fn emit_local_relationship(
    base: &BaseExtractor,
    from_symbol: &Symbol,
    target_symbol: &Symbol,
    value_node: Node,
    line_number: u32,
    relationships: &mut Vec<Relationship>,
) {
    let mut metadata = HashMap::new();
    metadata.insert(
        "refKind".to_string(),
        serde_json::Value::String("json_schema_ref".to_string()),
    );
    relationships.push(Relationship {
        id: format!(
            "{}_{}_{:?}_{}",
            from_symbol.id,
            target_symbol.id,
            RelationshipKind::References,
            value_node.start_position().row
        ),
        from_symbol_id: from_symbol.id.clone(),
        to_symbol_id: target_symbol.id.clone(),
        kind: RelationshipKind::References,
        file_path: base.file_path.clone(),
        line_number,
        confidence: 1.0,
        metadata: Some(metadata),
    });
}

/// Resolve a JSON-pointer fragment (segments separated by `/`) against the
/// extracted symbol tree. Returns the symbol whose name matches the terminal
/// segment AND whose ancestor chain matches the preceding segments in order.
fn resolve_local_pointer<'a>(symbols: &'a [Symbol], fragment: &str) -> Option<&'a Symbol> {
    let segments: Vec<&str> = fragment.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return None;
    }
    'outer: for symbol in symbols {
        if symbol.name != segments[segments.len() - 1] {
            continue;
        }
        // Walk up the parent chain matching preceding segments in reverse.
        let mut current_parent = symbol.parent_id.as_deref();
        for expected in segments[..segments.len() - 1].iter().rev() {
            let parent = match current_parent
                .and_then(|pid| symbols.iter().find(|s| s.id == pid))
            {
                Some(p) => p,
                None => continue 'outer,
            };
            if parent.name != *expected {
                continue 'outer;
            }
            current_parent = parent.parent_id.as_deref();
        }
        return Some(symbol);
    }
    None
}

fn symbol_for_pair<'a>(symbols: &'a [Symbol], pair: Node) -> Option<&'a Symbol> {
    let start = pair.start_byte() as u32;
    symbols.iter().find(|s| s.start_byte == start)
}
