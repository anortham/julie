//! Type definitions for navigation tools
//!
//! This module contains all structured result types and DTOs used by the
//! fast_goto and fast_refs navigation tools for MCP communication.

use serde::Serialize;

/// Structured result from fast_goto operation
#[derive(Debug, Clone, Serialize)]
pub struct FastGotoResult {
    pub tool: String,
    pub symbol: String,
    pub found: bool,
    pub definitions: Vec<DefinitionResult>,
    pub next_actions: Vec<String>,
}

/// Definition location result
#[derive(Debug, Clone, Serialize)]
pub struct DefinitionResult {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
}

/// Structured result from fast_refs operation
#[derive(Debug, Clone, Serialize)]
pub struct FastRefsResult {
    pub tool: String,
    pub symbol: String,
    pub found: bool,
    pub include_definition: bool,
    pub definition_count: usize,
    pub reference_count: usize,
    pub definitions: Vec<DefinitionResult>,
    pub references: Vec<ReferenceResult>,
    pub next_actions: Vec<String>,
}

/// Reference relationship result
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceResult {
    pub from_symbol_id: String,
    pub to_symbol_id: String,
    pub kind: String,
    pub file_path: String,
    pub line_number: u32,
    pub confidence: f32,
}
