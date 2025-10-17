//! Symbol resolution logic shared between navigation tools
//!
//! This module provides common utilities for resolving symbols across
//! different workspaces and using multiple search strategies.

use anyhow::Result;
use crate::handler::JulieServerHandler;
use crate::extractors::Symbol;
use crate::workspace::registry_service::WorkspaceRegistryService;

/// Resolve workspace parameter to specific workspace ID
///
/// Returns None for primary workspace (use handler.get_workspace().db)
/// Returns Some(workspace_id) for reference workspaces (need to open separate DB)
pub async fn resolve_workspace_filter(
    workspace_param: Option<&str>,
    handler: &JulieServerHandler,
) -> Result<Option<String>> {
    let workspace_param = workspace_param.unwrap_or("primary");

    match workspace_param {
        "primary" => {
            // Primary workspace - use handler.get_workspace().db (already loaded)
            Ok(None)
        }
        workspace_id => {
            // Reference workspace ID - validate it exists in registry
            if let Some(primary_workspace) = handler.get_workspace().await? {
                let registry_service =
                    WorkspaceRegistryService::new(primary_workspace.root.clone());

                // Check if it's a valid workspace ID
                match registry_service.get_workspace(workspace_id).await? {
                    Some(_) => Ok(Some(workspace_id.to_string())),
                    None => {
                        // Invalid workspace ID
                        Err(anyhow::anyhow!(
                            "Workspace '{}' not found. Use 'primary' or a valid workspace ID",
                            workspace_id
                        ))
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
    // CORRECTNESS FIX: Use exact path comparison instead of contains()
    // contains() is fragile - "test.rs" would match "contest.rs" (false positive)
    if let Some(context_file) = context_file {
        let a_in_context = a.file_path == context_file || a.file_path.ends_with(context_file);
        let b_in_context = b.file_path == context_file || b.file_path.ends_with(context_file);
        match (a_in_context, b_in_context) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
    }

    // Return Equal to allow caller to add final tiebreaker
    std::cmp::Ordering::Equal
}
