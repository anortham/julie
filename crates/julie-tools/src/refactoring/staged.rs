//! Staged rename data structures and helper utilities.

use super::RenameChange;
use crate::editing::ast_validation::TextEditSpan;
use anyhow::Result;
use julie_core::Symbol;
use julie_extractors::Relationship;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Represents a single file staged for rename refactoring in memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedFileRename {
    pub file_path: String,
    pub original_content: String,
    pub modified_content: String,
    pub changes: Vec<RenameChange>,
    pub edit_spans: Vec<TextEditSpan>,
}

/// Represents the complete batch of staged renames across the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedRename {
    pub old_name: String,
    pub new_name: String,
    pub files: Vec<StagedFileRename>,
    pub total_changes: usize,
    pub import_warning: Option<String>,
}

impl PreparedRename {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Build file-to-line mapping for definitions and references.
pub fn build_file_locations(
    definitions: &[Symbol],
    references: &[Relationship],
) -> HashMap<String, Vec<u32>> {
    let mut file_locations: HashMap<String, Vec<u32>> = HashMap::new();
    for def in definitions {
        file_locations
            .entry(def.file_path.clone())
            .or_default()
            .push(def.start_line);
    }
    for rel in references {
        file_locations
            .entry(rel.file_path.clone())
            .or_default()
            .push(rel.line_number);
    }
    file_locations
}

/// Normalize a `file:` scope argument to a workspace-relative Unix-style path.
pub fn normalize_scope_file_path(file_path: &str, workspace_root: &Path) -> Result<String> {
    let resolution = julie_core::paths::resolve_workspace_file_input(file_path, workspace_root)?;
    Ok(resolution.relative_query_path)
}

/// Returns true if the file extension is supported for automatic import rewriting.
pub fn is_import_update_supported(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "js" | "ts" | "tsx" | "jsx" | "mjs" | "cjs" | "py" | "rs"
    )
}

/// Update import statements purely in-memory on the given content string.
pub fn update_imports_in_content(
    content: &str,
    old_name: &str,
    new_name: &str,
) -> Result<(String, usize)> {
    let patterns = [
        Regex::new(&format!(
            r"\bimport\s+\{{\s*{}\s*\}}",
            regex::escape(old_name)
        ))?,
        Regex::new(&format!(
            r"\bimport\s+\{{\s*{}\s*,",
            regex::escape(old_name)
        ))?,
        Regex::new(&format!(r",\s*{}\s*\}}", regex::escape(old_name)))?,
        Regex::new(&format!(
            r"\bfrom\s+\S+\s+import\s+{}\b",
            regex::escape(old_name)
        ))?,
        Regex::new(&format!(r"\buse\s+.*::{}\b", regex::escape(old_name)))?,
    ];

    let mut modified_content = content.to_string();
    let mut changes = 0;

    for regex in patterns {
        if regex.is_match(&modified_content) {
            let before = modified_content.clone();
            modified_content = regex
                .replace_all(&modified_content, |caps: &regex::Captures| {
                    caps[0].replace(old_name, new_name)
                })
                .to_string();

            if modified_content != before {
                changes += 1;
            }
        }
    }

    Ok((modified_content, changes))
}
