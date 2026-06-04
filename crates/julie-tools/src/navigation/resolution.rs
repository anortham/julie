//! Symbol resolution logic shared between navigation tools.
//!
//! Handler-free utilities for resolving symbols across workspaces.
//! The handler-bound workspace-parameter resolver (`resolve_workspace_filter`)
//! has moved to `src/handler/workspace_resolution.rs`.

use julie_extractors::Symbol;

pub use julie_core::workspace_errors::{
    WorkspaceResolutionFailure, WorkspaceResolutionFailureKind, workspace_resolution_failure_kind,
};

// `WorkspaceTarget` has been relocated to `julie-context` so that
// `ToolContext::resolve_workspace_target` can name it without a cycle.
// All importers of `crate::navigation::resolution::WorkspaceTarget`
// continue to resolve unchanged via this re-export.
pub use julie_context::WorkspaceTarget;

/// Parse a qualified symbol name like "MyClass::method" or "MyClass.method"
/// into (parent_name, child_name), splitting on the LAST separator.
///
/// Returns None if no `::` or `.` separator is found.
pub fn parse_qualified_name(symbol: &str) -> Option<(&str, &str)> {
    // Check for :: first (more specific), then .
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

/// Priority ordering for symbol definitions by kind
pub fn definition_priority(kind: &julie_extractors::SymbolKind) -> u8 {
    use julie_extractors::SymbolKind;
    match kind {
        SymbolKind::Class | SymbolKind::Interface => 1,
        SymbolKind::Function => 2,
        SymbolKind::Method | SymbolKind::Constructor => 3,
        SymbolKind::Type | SymbolKind::Enum => 4,
        SymbolKind::Variable | SymbolKind::Constant => 5,
        _ => 10,
    }
}

/// Compare two symbols by priority and context for sorting
///
/// Returns std::cmp::Ordering::Equal if both symbols have equal priority/context,
/// allowing caller to add additional tiebreaker criteria
pub fn compare_symbols_by_priority_and_context(
    a: &Symbol,
    b: &Symbol,
    context_file: Option<&str>,
) -> std::cmp::Ordering {
    // First by definition priority (classes > functions > variables)
    let priority_cmp = definition_priority(&a.kind).cmp(&definition_priority(&b.kind));
    if priority_cmp != std::cmp::Ordering::Equal {
        return priority_cmp;
    }

    // Then by context file preference if provided
    // Use path-separator-aware matching to avoid false positives
    // (e.g. bare ends_with("test.rs") would incorrectly match "contest.rs")
    if let Some(context_file) = context_file {
        let suffix = format!("/{}", context_file);
        let a_in_context = a.file_path == context_file || a.file_path.ends_with(&suffix);
        let b_in_context = b.file_path == context_file || b.file_path.ends_with(&suffix);
        match (a_in_context, b_in_context) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
    }

    // Return Equal to allow caller to add final tiebreaker
    std::cmp::Ordering::Equal
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
