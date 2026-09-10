//! Symbol resolution over the snapshot graph, shared between navigation tools.
//!
//! The handler-bound workspace-parameter resolver (`resolve_workspace_filter`)
//! lives in `src/handler/workspace_resolution.rs`.

use std::cmp::Ordering;

use julie_core::Symbol;
use julie_extractors::{NormalizedSpan, SymbolKind};
use julie_facts::rows::{Span, SymbolRow};
use julie_index::graph::{Graph, SymbolId};
use serde_json::Value;

pub use julie_core::workspace_errors::{
    WorkspaceResolutionFailure, WorkspaceResolutionFailureKind, workspace_resolution_failure_kind,
};

// `WorkspaceTarget` lives in `julie-context` so that
// `ToolContext::resolve_workspace_target` can name it without a cycle.
pub use julie_context::WorkspaceTarget;

/// Parse a qualified symbol name like "MyClass::method" or "MyClass.method"
/// into (parent_name, child_name), splitting on the LAST separator.
///
/// Returns None if no `::` or `.` separator is found.
pub fn parse_qualified_name(symbol: &str) -> Option<(&str, &str)> {
    if let Some(pos) = symbol.rfind("::") {
        let parent = &symbol[..pos];
        let child = &symbol[pos + 2..];
        if !parent.is_empty() && !child.is_empty() {
            return Some((parent, child));
        }
    }
    if let Some(pos) = symbol.rfind('.') {
        let parent = &symbol[..pos];
        let child = &symbol[pos + 1..];
        if !parent.is_empty() && !child.is_empty() {
            return Some((parent, child));
        }
    }
    None
}

/// Last segment of `a::b::c` or `a.b.c`; the whole name when it has no separator.
pub fn qualified_leaf(name: &str) -> &str {
    parse_qualified_name(name).map_or(name, |(_, leaf)| leaf)
}

/// Priority ordering for symbol definitions by kind
pub fn definition_priority(kind: &SymbolKind) -> u8 {
    match kind {
        SymbolKind::Class | SymbolKind::Interface => 1,
        SymbolKind::Function => 2,
        SymbolKind::Method | SymbolKind::Constructor => 3,
        SymbolKind::Type | SymbolKind::Enum => 4,
        SymbolKind::Variable | SymbolKind::Constant => 5,
        _ => 10,
    }
}

/// Order two graph symbols by definition priority, then by whether they live
/// in `context_file`. Returns `Equal` when neither rule separates them.
pub fn compare_by_priority_and_context(
    graph: &Graph,
    a: SymbolId,
    b: SymbolId,
    context_file: Option<&str>,
) -> Ordering {
    let (a, b) = (graph.symbol(a), graph.symbol(b));
    let priority_cmp = definition_priority(&a.kind).cmp(&definition_priority(&b.kind));
    if priority_cmp != Ordering::Equal {
        return priority_cmp;
    }
    if let Some(context_file) = context_file {
        let suffix = format!("/{context_file}");
        let in_context = |path: &str| path == context_file || path.ends_with(&suffix);
        match (in_context(&a.path), in_context(&b.path)) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
    }
    Ordering::Equal
}

/// True if `path` equals `filter` or has `filter` as a path-separated suffix.
///
/// Prevents false positives from bare `ends_with`: `"handler.rs"` matches
/// `"src/tools/handler.rs"` (preceded by `/`) but NOT `"foohandler.rs"`.
pub fn file_path_matches_suffix(path: &str, filter: &str) -> bool {
    let path = normalize_path_suffix(path);
    let filter = normalize_path_suffix(filter);

    path == filter || path.ends_with(&format!("/{}", filter))
}

fn normalize_path_suffix(value: &str) -> String {
    let mut components = Vec::new();

    for component in value.split(['/', '\\']) {
        match component {
            "" | "." => {}
            ".." if components.last().is_some_and(|last| *last != "..") => {
                components.pop();
            }
            ".." => components.push(component),
            segment => components.push(segment),
        }
    }

    components.join("/")
}

fn normalized(span: Span) -> NormalizedSpan {
    NormalizedSpan {
        start_line: span.start_line,
        start_column: span.start_col,
        end_line: span.end_line,
        end_column: span.end_col,
        start_byte: span.start_byte,
        end_byte: span.end_byte,
    }
}

/// The graph row of `id` shaped as the `Symbol` the formatters take.
pub fn to_symbol(graph: &Graph, id: SymbolId) -> Symbol {
    let row = graph.symbol(id);
    julie_extractors::Symbol {
        id: row.id.clone(),
        name: row.name.clone(),
        kind: row.kind.clone(),
        language: row.language.clone(),
        file_path: row.path.clone(),
        start_line: row.span.start_line,
        start_column: row.span.start_col,
        end_line: row.span.end_line,
        end_column: row.span.end_col,
        start_byte: row.span.start_byte,
        end_byte: row.span.end_byte,
        body_span: row.body_span.map(normalized),
        body_hash: row.body_hash.clone(),
        signature: row.signature.clone(),
        doc_comment: row.doc_comment.clone(),
        visibility: row.visibility.clone(),
        parent_id: row
            .parent_ordinal
            .map(|ordinal| format!("{}:{ordinal}", row.blob_hash)),
        metadata: row.metadata.clone(),
        annotations: row.annotations.clone(),
        semantic_group: row.semantic_group.clone(),
        confidence: row.confidence,
        content_type: row.content_type.clone(),
    }
    .into()
}

fn is_lookup_stub(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Import | SymbolKind::Export)
}

/// Definition kinds preferred when a dotted or `::` name is a single symbol.
fn is_definition_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Module
            | SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Trait
            | SymbolKind::Interface
            | SymbolKind::Enum
            | SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Namespace
            | SymbolKind::Type
            | SymbolKind::Constant
    )
}

fn impl_type_name(row: &SymbolRow) -> Option<&str> {
    row.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("impl_type_name"))
        .and_then(Value::as_str)
}

/// True when `id` is a child of a symbol named `parent` (or its last segment),
/// or its `impl_type_name` metadata names that type.
pub fn has_parent_named(graph: &Graph, id: SymbolId, parent: &str) -> bool {
    let leaf = qualified_leaf(parent);
    let named = |name: &str| name == parent || name == leaf;
    graph
        .parent(id)
        .is_some_and(|parent_id| named(&graph.symbol(parent_id).name))
        || impl_type_name(graph.symbol(id)).is_some_and(named)
}

fn context_file_matches(symbol_path: &str, context_file: &str) -> bool {
    file_path_matches_suffix(symbol_path, context_file)
        || file_path_matches_suffix(context_file, symbol_path)
        || context_file_matches_basename_stem(symbol_path, context_file)
}

fn context_file_matches_basename_stem(symbol_path: &str, context_file: &str) -> bool {
    if context_file.contains(['/', '\\', '.']) {
        return false;
    }

    let basename = symbol_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(symbol_path);
    let stem = basename.rsplit_once('.').map_or(basename, |(stem, _)| stem);
    stem == context_file
}

/// Symbols named `name`: the full name first (flat namespaces such as
/// `Phoenix.Router`), then `Parent::leaf` under a matching parent, then the
/// plain name. Import and export stubs never match. Sorted by definition
/// priority, then `context_file` membership, then graph order.
pub fn find_symbols(graph: &Graph, name: &str, context_file: Option<&str>) -> Vec<SymbolId> {
    let candidates = |name: &str, keep: &dyn Fn(SymbolId) -> bool| -> Vec<SymbolId> {
        graph
            .find_by_name(name)
            .iter()
            .copied()
            .filter(|id| !is_lookup_stub(&graph.symbol(*id).kind))
            .filter(|id| keep(*id))
            .collect()
    };

    let mut found = Vec::new();
    if name.contains('.') || name.contains("::") {
        found = candidates(name, &|id| is_definition_kind(&graph.symbol(id).kind));
    }
    if found.is_empty()
        && let Some((parent, leaf)) = parse_qualified_name(name)
    {
        found = candidates(leaf, &|id| has_parent_named(graph, id, parent));
    }
    if found.is_empty() {
        found = candidates(name, &|_| true);
    }
    if let Some(file) = context_file {
        found.retain(|id| context_file_matches(&graph.symbol(*id).path, file));
    }
    found.sort_by(|a, b| {
        compare_by_priority_and_context(graph, *a, *b, context_file).then(a.cmp(b))
    });
    found
}
