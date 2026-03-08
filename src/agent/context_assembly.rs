//! Context assembly for agent dispatch.
//!
//! Assembles a structured prompt from workspace search results and memories,
//! formatted for consumption by an AI agent CLI.
//!
//! The assembly approach:
//! 1. Search the workspace's SearchIndex for symbols relevant to the task
//! 2. Get symbol signatures and doc comments from search results
//! 3. Recall relevant memories from the checkpoint system
//! 4. Format everything into a structured prompt

use std::path::Path;

use anyhow::Result;

use crate::memory::{self, RecallOptions};
use crate::search::{LanguageConfigs, SearchFilter, SearchIndex};

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
/// If `workspace_root` is `Some`, searches the workspace's Tantivy index
/// for relevant symbols and recalls relevant memories. If `None`, produces
/// a minimal prompt with just the task and any provided hints.
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
/// ## Recent Memories
/// [Relevant checkpoint summaries from recall]
///
/// ## Additional Context
/// [Any extra hints provided by the caller]
///
/// # Task
/// [User's task description]
/// ```
pub async fn assemble_context(
    workspace_root: Option<&Path>,
    task: &str,
    hints: Option<ContextHints>,
) -> Result<String> {
    let mut sections = Vec::new();

    sections.push("# Context (assembled by Julie)".to_string());
    sections.push(String::new());

    // 1. Search for relevant code symbols
    if let Some(root) = workspace_root {
        let code_section = assemble_code_context(root, task).await;
        if !code_section.is_empty() {
            sections.push("## Relevant Code".to_string());
            sections.push(String::new());
            sections.push(code_section);
            sections.push(String::new());
        }
    }

    // 2. Recall relevant memories
    if let Some(root) = workspace_root {
        let memory_section = assemble_memory_context(root, task);
        if !memory_section.is_empty() {
            sections.push("## Recent Memories".to_string());
            sections.push(String::new());
            sections.push(memory_section);
            sections.push(String::new());
        }
    }

    // 3. Include hints
    if let Some(ref hints) = hints {
        let hints_section = assemble_hints_context(hints);
        if !hints_section.is_empty() {
            sections.push("## Additional Context".to_string());
            sections.push(String::new());
            sections.push(hints_section);
            sections.push(String::new());
        }
    }

    // 4. Task section (always present)
    sections.push("# Task".to_string());
    sections.push(String::new());
    sections.push(task.to_string());
    sections.push(String::new());

    Ok(sections.join("\n"))
}

/// Search the workspace's Tantivy index for symbols relevant to the task.
///
/// Returns a formatted string with top search results (signatures + doc comments).
async fn assemble_code_context(workspace_root: &Path, task: &str) -> String {
    let tantivy_dir = workspace_root
        .join(".julie")
        .join("indexes")
        .join("tantivy");

    // Try to open an existing Tantivy index
    let index = if tantivy_dir.exists() {
        let configs = LanguageConfigs::load_embedded();
        match SearchIndex::open_with_language_configs(&tantivy_dir, &configs) {
            Ok(idx) => Some(idx),
            Err(_) => None,
        }
    } else {
        None
    };

    let Some(index) = index else {
        return String::new();
    };

    let filter = SearchFilter::default();
    let results = match index.search_symbols(task, &filter, 10) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

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

/// Recall relevant memories from the checkpoint system.
///
/// Uses the task description as a search query to find relevant checkpoints.
fn assemble_memory_context(workspace_root: &Path, task: &str) -> String {
    let options = RecallOptions {
        search: Some(task.to_string()),
        limit: Some(5),
        full: Some(false),
        ..Default::default()
    };

    let result = match memory::recall::recall(workspace_root, options) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

    if result.checkpoints.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    for checkpoint in &result.checkpoints {
        let summary = checkpoint
            .summary
            .as_deref()
            .unwrap_or(&checkpoint.description);
        let truncated = if summary.chars().count() > 200 {
            let s: String = summary.chars().take(200).collect();
            format!("{}...", s)
        } else {
            summary.to_string()
        };
        lines.push(format!(
            "- **{}** ({}): {}",
            checkpoint.id, checkpoint.timestamp, truncated
        ));
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
