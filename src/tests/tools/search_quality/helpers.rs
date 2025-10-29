//! Test Helpers - Shared utilities for search quality tests

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use anyhow::{anyhow, Result};
use rust_mcp_sdk::schema::CallToolResult;

/// Search Julie's codebase (file content search)
pub async fn search_content(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    let tool = FastSearchTool {
        query: query.to_string(),
        search_method: "text".to_string(),
        limit,
        language: None,
        file_pattern: None,
        workspace: Some("primary".to_string()),
        output: None,  // Use default (symbols mode)
        context_lines: None,
        search_target: "content".to_string(),
    };

    let result = tool.call_tool(handler).await?;
    parse_search_results(&result)
}

/// Search Julie's codebase (symbol definitions search)
pub async fn search_definitions(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    let tool = FastSearchTool {
        query: query.to_string(),
        search_method: "text".to_string(),
        limit,
        language: None,
        file_pattern: None,
        workspace: Some("primary".to_string()),
        output: None,
        context_lines: None,
        search_target: "definitions".to_string(),
    };

    let result = tool.call_tool(handler).await?;
    parse_search_results(&result)
}

/// Parse search results from MCP CallToolResult
fn parse_search_results(result: &CallToolResult) -> Result<Vec<Symbol>> {
    // Extract structured_content from CallToolResult
    // The search tool returns OptimizedResponse in structured_content with format:
    // { "tool": "fast_search", "results": [Symbol, ...], "confidence": 0.85, ... }

    if let Some(structured) = &result.structured_content {
        if let Some(results_value) = structured.get("results") {
            // Parse the results array as Vec<Symbol>
            let symbols: Vec<Symbol> = serde_json::from_value(results_value.clone())
                .map_err(|e| anyhow!("Failed to parse symbols from results: {}", e))?;
            return Ok(symbols);
        }
    }

    // Fallback: no structured content
    Ok(Vec::new())
}

/// Assert that results contain a file path matching the pattern
pub fn assert_contains_path(results: &[Symbol], path_pattern: &str) {
    let found = results.iter().any(|r| r.file_path.contains(path_pattern));
    assert!(
        found,
        "Expected results to contain path '{}', but found:\n{}",
        path_pattern,
        format_results(results)
    );
}

/// Assert that results do NOT contain a file path matching the pattern
pub fn assert_not_contains_path(results: &[Symbol], path_pattern: &str) {
    let found = results.iter().any(|r| r.file_path.contains(path_pattern));
    assert!(
        !found,
        "Expected results to NOT contain path '{}', but it was found in:\n{}",
        path_pattern,
        format_results(results)
    );
}

/// Assert minimum number of results
pub fn assert_min_results(results: &[Symbol], min: usize) {
    assert!(
        results.len() >= min,
        "Expected at least {} results, but got {}:\n{}",
        min,
        results.len(),
        format_results(results)
    );
}

/// Assert maximum number of results
pub fn assert_max_results(results: &[Symbol], max: usize) {
    assert!(
        results.len() <= max,
        "Expected at most {} results, but got {}:\n{}",
        max,
        results.len(),
        format_results(results)
    );
}

/// Assert exact number of results
pub fn assert_exact_count(results: &[Symbol], expected: usize) {
    assert_eq!(
        results.len(),
        expected,
        "Expected exactly {} results, but got {}:\n{}",
        expected,
        results.len(),
        format_results(results)
    );
}

/// Assert that a specific symbol kind is present
pub fn assert_contains_symbol_kind(results: &[Symbol], kind: &str) {
    let found = results.iter().any(|r| r.kind.to_string() == kind);
    assert!(
        found,
        "Expected results to contain symbol kind '{}', but found:\n{}",
        kind,
        format_results(results)
    );
}

/// Assert that first result matches criteria (for ranking tests)
pub fn assert_first_result(
    results: &[Symbol],
    path_pattern: &str,
    name_pattern: Option<&str>,
) {
    assert!(
        !results.is_empty(),
        "Expected at least one result, but got none"
    );

    let first = &results[0];
    assert!(
        first.file_path.contains(path_pattern),
        "Expected first result to be in '{}', but got '{}'\nAll results:\n{}",
        path_pattern,
        first.file_path,
        format_results(results)
    );

    if let Some(name) = name_pattern {
        assert!(
            first.name.contains(name),
            "Expected first result name to contain '{}', but got '{}'\nAll results:\n{}",
            name,
            first.name,
            format_results(results)
        );
    }
}

/// Format results for error messages
fn format_results(results: &[Symbol]) -> String {
    if results.is_empty() {
        return "  (no results)".to_string();
    }

    results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "  [{}] {} ({}:{})",
                i + 1,
                r.name,
                r.file_path,
                r.start_line
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
