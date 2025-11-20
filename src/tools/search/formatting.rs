//! Result formatting for search responses
//!
//! Provides formatting utilities for converting search results into human and AI-friendly formats.

use crate::extractors::Symbol;
use crate::tools::shared::OptimizedResponse;
use serde::Serialize;
use tracing::{debug, error, warn};

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

/// Format minimal summary for AI agents (structured_content has all data)
///
/// AI agents parse structured_content (JSON), not text output.
/// Keep text minimal to save massive context tokens.
pub fn format_optimized_results(query: &str, optimized: &OptimizedResponse<Symbol>) -> String {
    // Line 1: Summary with count and confidence
    let summary = format!(
        "Found {} results for '{}' (confidence: {:.1})",
        optimized.total_found, query, optimized.confidence
    );

    // Line 2: Top 5 result names (quick scan)
    let top_names: Vec<String> = optimized
        .results
        .iter()
        .take(5)
        .map(|s| s.name.clone())
        .collect();

    if top_names.is_empty() {
        summary
    } else {
        format!("{}\nTop results: {}", summary, top_names.join(", "))
    }
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

/// Encode OptimizedResponse to TOON format with automatic JSON fallback
///
/// TOON (Token-Oriented Object Notation) provides ~35-67% token reduction vs JSON
/// by using compact CSV-style encoding for uniform arrays.
///
/// This function converts Symbol to ToonSymbol (primitives only) so TOON can
/// use compact tabular format instead of verbose YAML-style.
///
/// ## Returns
/// TOON-encoded string on success, JSON on fallback
pub fn encode_to_toon_with_fallback(
    data: &OptimizedResponse<Symbol>,
    format_name: &str,
) -> String {
    // Convert to simplified ToonResponse (primitives only for compact encoding)
    let toon_response = ToonResponse {
        tool: data.tool.clone(),
        results: data.results.iter().map(ToonSymbol::from).collect(),
        confidence: data.confidence,
        total_found: data.total_found,
        insights: data.insights.clone(),
        next_actions: data.next_actions.clone(),
    };

    match toon_format::encode_default(&toon_response) {
        Ok(toon) => {
            debug!("✅ Encoded {} to TOON ({} chars)", format_name, toon.len());
            toon
        }
        Err(e) => {
            warn!("❌ TOON encoding failed for {}: {}", format_name, e);
            warn!("   Falling back to JSON format");

            // Fallback to pretty JSON of original data
            match serde_json::to_string_pretty(data) {
                Ok(json) => json,
                Err(e2) => {
                    error!("💥 Both TOON and JSON serialization failed: {}", e2);
                    format!("Error: Unable to serialize {}", format_name)
                }
            }
        }
    }
}
