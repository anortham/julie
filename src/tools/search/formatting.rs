//! Result formatting for search responses
//!
//! Provides formatting utilities for search tool responses.

use crate::extractors::Symbol;
use serde::Serialize;

/// Simplified Symbol for TOON encoding (primitives only for compact CSV-style)
#[derive(Debug, Clone, Serialize)]
pub struct ToonSymbol {
    id: String,
    name: String,
    kind: String, // Enum formatted as string
    language: String,
    file_path: String,
    start_line: u32,
    end_line: u32,
    signature: Option<String>,
    doc_comment: Option<String>,
    visibility: Option<String>, // Enum formatted as string
    confidence: Option<f32>,
    code_context: Option<String>,
}

impl From<&Symbol> for ToonSymbol {
    fn from(s: &Symbol) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            kind: format!("{:?}", s.kind), // Convert enum to string
            language: s.language.clone(),
            file_path: s.file_path.clone(),
            start_line: s.start_line,
            end_line: s.end_line,
            signature: s.signature.clone(),
            doc_comment: s.doc_comment.clone(),
            visibility: s.visibility.as_ref().map(|v| format!("{:?}", v)),
            confidence: s.confidence,
            code_context: s.code_context.clone(),
        }
    }
}

/// Simplified OptimizedResponse for TOON encoding
#[derive(Debug, Clone, Serialize)]
pub struct ToonResponse {
    pub tool: String,
    pub results: Vec<ToonSymbol>,
    pub confidence: f32,
    pub total_found: usize,
    pub insights: Option<String>,
    pub next_actions: Vec<String>,
}

/// Truncate code_context field to save massive tokens in search results
///
/// Formula: max_lines = context_lines * 2 + 1
/// - context_lines=0 → 1 line total (just match)
/// - context_lines=1 → 3 lines total (1 before + match + 1 after) [DEFAULT]
/// - context_lines=3 → 7 lines total (grep default)
pub fn truncate_code_context(symbols: Vec<Symbol>, context_lines: Option<u32>) -> Vec<Symbol> {
    let context_lines = context_lines.unwrap_or(1) as usize;
    let max_lines = context_lines * 2 + 1; // before + match + after

    symbols
        .into_iter()
        .map(|mut symbol| {
            if let Some(code_context) = symbol.code_context.take() {
                let lines: Vec<&str> = code_context.lines().collect();

                if lines.len() > max_lines {
                    // Truncate to max_lines and add indicator
                    let truncated: Vec<&str> = lines.into_iter().take(max_lines).collect();
                    symbol.code_context = Some(format!("{}...", truncated.join("\n")));
                } else {
                    // Keep as-is (within limit)
                    symbol.code_context = Some(code_context);
                }
            }
            symbol
        })
        .collect()
}
