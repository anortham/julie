//! PowerShell module imports, exports, and dot-sourcing
//! Handles Import-Module, Export-ModuleMember, using statements, and dot sourcing

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use regex::Regex;
use tree_sitter::Node;

/// Extract import statements (Import-Module, using, etc.)
pub(super) fn extract_import(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let node_text = base.get_node_text(&node);
    let is_using = node_text.starts_with("using");
    let is_dot_sourcing = node_text.starts_with(".");

    // Extract module name - this is a simplified approach
    let module_name = if is_using {
        extract_using_name(&node_text)?
    } else if is_dot_sourcing {
        extract_dot_source_name(&node_text)?
    } else {
        extract_import_module_name(&node_text)?
    };

    let doc_comment = if is_using {
        Some("Using statement".to_string())
    } else if is_dot_sourcing {
        Some("Dot sourcing".to_string())
    } else {
        Some("Module import".to_string())
    };

    Some(base.create_symbol(
        &node,
        module_name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(node_text.trim().to_string()),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract import commands (Import-Module, Export-ModuleMember, using)
pub(super) fn extract_import_command(
    base: &mut BaseExtractor,
    node: Node,
    command_name: &str,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let node_text = base.get_node_text(&node);
    let mut module_name = String::new();
    let signature = node_text.trim().to_string();

    if command_name == "Import-Module" {
        // Extract module name from "Import-Module Az.Accounts" or "Import-Module -Name 'Custom.Tools'"
        if let Some(captures) =
            Regex::new(r#"Import-Module\s+(?:-Name\s+["']?([^"'\s]+)["']?|([A-Za-z0-9.-]+))"#)
                .unwrap()
                .captures(&node_text)
        {
            module_name = captures
                .get(1)
                .or_else(|| captures.get(2))
                .map_or("unknown".to_string(), |m| m.as_str().to_string());
        }
    } else if command_name == "using" {
        // Extract from "using namespace System.Collections.Generic" or "using module Az.Storage"
        if let Some(captures) = Regex::new(r"using\s+(?:namespace|module)\s+([A-Za-z0-9.-_]+)")
            .unwrap()
            .captures(&node_text)
        {
            module_name = captures
                .get(1)
                .map_or("unknown".to_string(), |m| m.as_str().to_string());
        }
    } else if command_name == "Export-ModuleMember" {
        // Extract the type being exported (Function, Variable, Alias)
        if let Some(captures) = Regex::new(r"Export-ModuleMember\s+-(\w+)")
            .unwrap()
            .captures(&node_text)
        {
            module_name = captures
                .get(1)
                .map_or("unknown".to_string(), |m| m.as_str().to_string());
        } else {
            // Fallback: try to extract from the full text
            if node_text.contains("-Function") {
                module_name = "Function".to_string();
            } else if node_text.contains("-Variable") {
                module_name = "Variable".to_string();
            } else if node_text.contains("-Alias") {
                module_name = "Alias".to_string();
            } else {
                module_name = "ModuleMember".to_string();
            }
        }
    }

    if module_name.is_empty() || module_name == "unknown" {
        return None;
    }

    let is_using = command_name == "using";
    let is_export = command_name == "Export-ModuleMember";

    let symbol_kind = if is_export {
        SymbolKind::Export
    } else {
        SymbolKind::Import
    };
    let doc_comment = if is_export {
        Some("Module export".to_string())
    } else if is_using {
        Some("Using statement".to_string())
    } else {
        Some("Module import".to_string())
    };

    Some(base.create_symbol(
        &node,
        module_name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract dot sourcing (e.g., '. "$PSScriptRoot\CommonFunctions.ps1"')
pub(super) fn extract_dot_sourcing(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Extract script path from dot sourcing like '. "$PSScriptRoot\CommonFunctions.ps1"'
    let mut cursor = node.walk();
    let command_name_expr_node = node
        .children(&mut cursor)
        .find(|child| child.kind() == "command_name_expr")?;

    let script_path = base.get_node_text(&command_name_expr_node);
    let signature = base.get_node_text(&node).trim().to_string();

    // Extract just the filename for the symbol name
    let mut file_name = script_path.replace("'", "").replace("\"", ""); // Remove quotes
    let last_slash = file_name.rfind('\\').max(file_name.rfind('/'));
    if let Some(pos) = last_slash {
        file_name = file_name[(pos + 1)..].to_string();
    }

    // Remove .ps1 extension for cleaner symbol name
    if file_name.ends_with(".ps1") {
        file_name = file_name[..file_name.len() - 4].to_string();
    }

    Some(base.create_symbol(
        &node,
        file_name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment: Some("Dot sourcing script".to_string()),
        },
    ))
}

/// Extract module name from Import-Module command
fn extract_import_module_name(node_text: &str) -> Option<String> {
    Regex::new(r#"Import-Module\s+(?:-Name\s+["']?([^"'\s]+)["']?|([A-Za-z0-9.-]+))"#)
        .unwrap()
        .captures(node_text)
        .map(|captures| {
            captures
                .get(1)
                .or_else(|| captures.get(2))
                .map_or("unknown".to_string(), |m| m.as_str().to_string())
        })
}

/// Extract module/namespace name from using statement
fn extract_using_name(node_text: &str) -> Option<String> {
    Regex::new(r"using\s+(?:namespace|module)\s+([A-Za-z0-9.-_]+)")
        .unwrap()
        .captures(node_text)
        .map(|captures| {
            captures
                .get(1)
                .map_or("unknown".to_string(), |m| m.as_str().to_string())
        })
}

/// Extract filename from dot source command
fn extract_dot_source_name(node_text: &str) -> Option<String> {
    // Extract path after the dot operator
    let trimmed = node_text.trim_start_matches('.');
    let cleaned = trimmed.replace("'", "").replace("\"", "");

    // Get just the filename
    let last_slash = cleaned.rfind('\\').max(cleaned.rfind('/'));
    let mut file_name = if let Some(pos) = last_slash {
        cleaned[(pos + 1)..].to_string()
    } else {
        cleaned
    };

    // Remove .ps1 extension
    if file_name.ends_with(".ps1") {
        file_name = file_name[..file_name.len() - 4].to_string();
    }

    if file_name.is_empty() {
        None
    } else {
        Some(file_name)
    }
}
