//! TOML domain-relationship extraction (Phase 3.3).
//!
//! TOML has no inter-key reference construct in the format itself; the
//! Phase 3.3 contract is *domain-aware*. Two file basenames trigger
//! relationship extraction; everything else emits nothing.
//!
//! - **Cargo.toml**: `[dependencies]`, `[dev-dependencies]`,
//!   `[build-dependencies]`, and target-scoped `[target.<triple>.dependencies]`
//!   tables emit `RelationshipKind::Imports` edges from the table symbol to
//!   each child key (e.g., `serde`, `tokio`).
//! - **pyproject.toml**: `[tool.<x>.*]` tables emit
//!   `RelationshipKind::References` edges, one per unique top-level tool
//!   name. `[tool.pytest]` and `[tool.pytest.ini_options]` collapse to a
//!   single `pytest` edge.
//!
//! Other tables — including dotted tables in non-Cargo / non-pyproject
//! files — produce no relationships. Symbol extraction is unchanged.

use crate::base::{BaseExtractor, Relationship, RelationshipKind, Symbol};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::Node;

pub(super) fn extract_relationships_internal(
    base: &BaseExtractor,
    root: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let basename = file_basename(&base.file_path);
    match basename.as_deref() {
        Some("Cargo.toml") => extract_cargo_relationships(base, root, symbols, relationships),
        Some("pyproject.toml") => {
            extract_pyproject_relationships(base, root, symbols, relationships)
        }
        _ => {}
    }
}

fn file_basename(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .file_name()
        .and_then(|os| os.to_str())
        .map(|s| s.to_string())
}

fn walk_tables<F: FnMut(Node)>(node: Node, mut f: F) {
    fn recurse<F: FnMut(Node)>(node: Node, f: &mut F) {
        if matches!(node.kind(), "table" | "table_array_element") {
            f(node);
        }
        for child in node.children(&mut node.walk()) {
            recurse(child, f);
        }
    }
    recurse(node, &mut f);
}

fn extract_cargo_relationships(
    base: &BaseExtractor,
    root: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    walk_tables(root, |table| {
        let name = match table_header_text(base, table) {
            Some(n) => n,
            None => return,
        };
        if !is_cargo_dependencies_table(&name) {
            return;
        }
        let table_symbol = match symbols
            .iter()
            .find(|s| s.start_byte == table.start_byte() as u32)
        {
            Some(s) => s,
            None => return,
        };
        // Children of a Cargo dependencies table are `pair` nodes (one per
        // dep). Each pair already has a symbol; we emit one Imports edge
        // per pair.
        for child in table.children(&mut table.walk()) {
            if child.kind() != "pair" {
                continue;
            }
            let dep_symbol = match symbols
                .iter()
                .find(|s| s.start_byte == child.start_byte() as u32)
            {
                Some(s) => s,
                None => continue,
            };
            let mut metadata = HashMap::new();
            metadata.insert("dependencyKind".to_string(), Value::String(name.clone()));
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    table_symbol.id,
                    dep_symbol.id,
                    RelationshipKind::Imports,
                    child.start_position().row
                ),
                from_symbol_id: table_symbol.id.clone(),
                to_symbol_id: dep_symbol.id.clone(),
                kind: RelationshipKind::Imports,
                file_path: base.file_path.clone(),
                line_number: child.start_position().row as u32 + 1,
                confidence: 1.0,
                metadata: Some(metadata),
            });
        }
    });
}

fn extract_pyproject_relationships(
    base: &BaseExtractor,
    root: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let mut seen_tools: HashSet<String> = HashSet::new();
    walk_tables(root, |table| {
        let name = match table_header_text(base, table) {
            Some(n) => n,
            None => return,
        };
        let segments: Vec<&str> = name.split('.').collect();
        if segments.len() < 2 || segments[0] != "tool" {
            return;
        }
        let tool_name = segments[1].to_string();
        if !seen_tools.insert(tool_name.clone()) {
            return;
        }
        let table_symbol = match symbols
            .iter()
            .find(|s| s.start_byte == table.start_byte() as u32)
        {
            Some(s) => s,
            None => return,
        };
        // From-side: prefer a top-level `[project]` table symbol if present,
        // otherwise fall back to the table itself (the resolver can still
        // route on metadata.toolName).
        let from_id = symbols
            .iter()
            .find(|s| s.name == "project" && s.parent_id.is_none())
            .map(|s| s.id.clone())
            .unwrap_or_else(|| table_symbol.id.clone());
        let mut metadata = HashMap::new();
        metadata.insert("toolName".to_string(), Value::String(tool_name.clone()));
        relationships.push(Relationship {
            id: format!(
                "{}_{}_{:?}_{}",
                from_id,
                table_symbol.id,
                RelationshipKind::References,
                table.start_position().row
            ),
            from_symbol_id: from_id,
            to_symbol_id: table_symbol.id.clone(),
            kind: RelationshipKind::References,
            file_path: base.file_path.clone(),
            line_number: table.start_position().row as u32 + 1,
            confidence: 1.0,
            metadata: Some(metadata),
        });
    });
}

fn is_cargo_dependencies_table(name: &str) -> bool {
    matches!(
        name,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    ) || (name.starts_with("target.") && name.ends_with(".dependencies"))
}

/// Extract the textual header of a `table` node (the part between `[` and
/// `]`). Returns the dotted form (e.g., `"tool.pytest.ini_options"`,
/// `"target.x86_64-unknown-linux-gnu.dependencies"`).
fn table_header_text(base: &BaseExtractor, table: Node) -> Option<String> {
    for child in table.children(&mut table.walk()) {
        match child.kind() {
            "bare_key" | "quoted_key" => {
                let raw = base.get_node_text(&child);
                return Some(raw.trim_matches('"').trim_matches('\'').to_string());
            }
            "dotted_key" => {
                return Some(base.get_node_text(&child));
            }
            _ => {}
        }
    }
    None
}
