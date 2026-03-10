//! Context assembly for agent dispatch.
//!
//! Assembles a structured prompt from workspace search results,
//! formatted for consumption by an AI agent CLI.
//!
//! The assembly approach:
//! 1. Search the workspace's SearchIndex for symbols relevant to the task
//! 2. Get symbol signatures and doc comments from search results
//! 3. Format everything into a structured prompt

use std::sync::Arc;

use anyhow::Result;

use crate::search::{SearchFilter, SearchIndex};

/// Hints to guide context assembly.
///
/// These are optional signals from the user about what context is relevant.
#[derive(Debug, Clone, Default)]
pub struct ContextHints {
    /// Specific files to include context from.
    pub files: Option<Vec<String>>,
    /// Specific symbol names to look up.
    pub symbols: Option<Vec<String>>,
    /// Additional free-form context to include verbatim.
    pub extra_context: Option<String>,
}

/// Assemble context from a workspace into a structured prompt string.
///
/// Code search uses the caller-provided `SearchIndex` (via `search_index`),
/// which avoids the wrong-path bug of trying to reconstruct the Tantivy path.
/// The caller (API layer) has access to the workspace's `SearchIndex` via
/// `LoadedWorkspace.workspace.search_index`.
///
/// # Sections
///
/// The output is structured as:
/// ```text
/// # Context (assembled by Julie)
///
/// ## Relevant Code
/// [Top N symbols with signatures and doc comments]
///
/// ## Additional Context
/// [Any extra hints provided by the caller]
///
/// # Task
/// [User's task description]
/// ```
pub async fn assemble_context(
    _workspace_root: Option<&std::path::Path>,
    search_index: Option<&Arc<std::sync::Mutex<SearchIndex>>>,
    task: &str,
    hints: Option<ContextHints>,
) -> Result<String> {
    let mut sections = Vec::new();

    sections.push("# Context (assembled by Julie)".to_string());
    sections.push(String::new());

    // 1. Search for relevant code symbols (using caller-provided SearchIndex)
    if let Some(index) = search_index {
        let code_section = assemble_code_context(index, task);
        if !code_section.is_empty() {
            sections.push("## Relevant Code".to_string());
            sections.push(String::new());
            sections.push(code_section);
            sections.push(String::new());
        }
    }

    // 2. Include hints
    if let Some(ref hints) = hints {
        let hints_section = assemble_hints_context(hints);
        if !hints_section.is_empty() {
            sections.push("## Additional Context".to_string());
            sections.push(String::new());
            sections.push(hints_section);
            sections.push(String::new());
        }
    }

    // 3. Task section (always present)
    sections.push("# Task".to_string());
    sections.push(String::new());
    sections.push(task.to_string());
    sections.push(String::new());

    Ok(sections.join("\n"))
}

/// Search the workspace's Tantivy index for symbols relevant to the task.
///
/// Uses the caller-provided `SearchIndex` (already correctly routed to the
/// workspace's `{workspace_id}/tantivy/` directory).
fn assemble_code_context(
    search_index: &Arc<std::sync::Mutex<SearchIndex>>,
    task: &str,
) -> String {
    let index = match search_index.lock() {
        Ok(idx) => idx,
        Err(_) => return String::new(),
    };

    let filter = SearchFilter::default();
    let results = match index.search_symbols(task, &filter, 10) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

    drop(index); // release lock early

    if results.results.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    for result in &results.results {
        lines.push(format!(
            "### {} (`{}`, {})",
            result.name, result.kind, result.file_path
        ));
        if !result.signature.is_empty() {
            lines.push(format!("```\n{}\n```", result.signature));
        }
        if !result.doc_comment.is_empty() {
            lines.push(result.doc_comment.clone());
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Format hints into a context section.
fn assemble_hints_context(hints: &ContextHints) -> String {
    let mut lines = Vec::new();

    if let Some(ref files) = hints.files {
        lines.push("**Relevant files:**".to_string());
        for file in files {
            lines.push(format!("- `{}`", file));
        }
        lines.push(String::new());
    }

    if let Some(ref symbols) = hints.symbols {
        lines.push("**Key symbols:**".to_string());
        for symbol in symbols {
            lines.push(format!("- `{}`", symbol));
        }
        lines.push(String::new());
    }

    if let Some(ref extra) = hints.extra_context {
        lines.push(extra.clone());
        lines.push(String::new());
    }

    lines.join("\n")
}
