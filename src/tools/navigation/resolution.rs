//! Symbol resolution logic shared between navigation tools
//!
//! This module provides common utilities for resolving symbols across
//! different workspaces and using multiple search strategies.

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::workspace::registry_service::WorkspaceRegistryService;
use anyhow::Result;

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

/// Workspace targeting for tool operations.
///
/// Replaces the previous `Option<String>` / `Option<Vec<String>>` return types
/// from workspace resolution with an explicit enum that all tool callers match on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    /// Use the primary workspace (handler.get_workspace().db)
    Primary,
    /// Use a specific reference workspace by ID
    Reference(String),
}

/// Given an invalid workspace ID and a list of known workspace IDs,
/// return an error with a fuzzy match suggestion (if one is close enough)
/// or a generic "not found" error.
fn suggest_closest_workspace(workspace_id: &str, known_ids: &[&str]) -> Result<WorkspaceTarget> {
    if let Some((best_match, distance)) =
        crate::utils::string_similarity::find_closest_match(workspace_id, known_ids)
    {
        // Only suggest if the distance is reasonable (< 50% of query length)
        if distance < workspace_id.len() / 2 {
            return Err(anyhow::anyhow!(
                "Workspace '{}' not found. Did you mean '{}'?",
                workspace_id,
                best_match
            ));
        }
    }

    // No close match found
    Err(anyhow::anyhow!(
        "Workspace '{}' not found. Use 'primary' or a valid workspace ID",
        workspace_id
    ))
}

/// Resolve workspace parameter to a WorkspaceTarget.
///
/// - `None` or `"primary"` → `WorkspaceTarget::Primary`
/// - Any other string → validated as a reference workspace ID → `WorkspaceTarget::Reference(id)`
///
/// Workspace IDs are validated against `WorkspaceRegistryService`.
pub async fn resolve_workspace_filter(
    workspace_param: Option<&str>,
    handler: &JulieServerHandler,
) -> Result<WorkspaceTarget> {
    let workspace_param = workspace_param.unwrap_or("primary");

    match workspace_param {
        "primary" => Ok(WorkspaceTarget::Primary),
        workspace_id => {
            if let Some(primary_workspace) = handler.get_workspace().await? {
                let registry_service =
                    WorkspaceRegistryService::new(primary_workspace.root.clone());

                match registry_service.get_workspace(workspace_id).await? {
                    Some(_) => Ok(WorkspaceTarget::Reference(workspace_id.to_string())),
                    None => {
                        let all_workspaces = registry_service.get_all_workspaces().await?;
                        let workspace_ids: Vec<&str> =
                            all_workspaces.iter().map(|w| w.id.as_str()).collect();
                        suggest_closest_workspace(workspace_id, &workspace_ids)
                    }
                }
            } else {
                Err(anyhow::anyhow!(
                    "No primary workspace found. Initialize workspace first."
                ))
            }
        }
    }
}

/// Priority ordering for symbol definitions by kind
pub fn definition_priority(kind: &crate::extractors::SymbolKind) -> u8 {
    use crate::extractors::SymbolKind;
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
