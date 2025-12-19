//! Shared types for refactoring operations

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Structured result from smart refactoring operations
#[derive(Debug, Clone, Serialize)]
pub struct SmartRefactorResult {
    pub tool: String,
    pub operation: String,
    pub dry_run: bool,
    pub success: bool,
    pub files_modified: Vec<String>,
    pub changes_count: usize,
    pub next_actions: Vec<String>,
    /// Operation-specific metadata (flexible JSON for different operation types)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Syntax error detected by tree-sitter
/// (Preserved from abandoned AutoFixSyntax feature - may be useful in future)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SyntaxError {
    /// Line number where error occurs (1-based)
    pub line: u32,
    /// Column number where error occurs (0-based)
    pub column: u32,
    /// Error description
    pub message: String,
    /// Severity: "error" or "warning"
    pub severity: String,
    /// Suggested fix if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
    /// Code snippet showing the error context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Result of auto-fix operation
/// (Preserved from abandoned AutoFixSyntax feature - may be useful in future)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutoFixResult {
    /// Whether any fixes were applied
    pub fixes_applied: bool,
    /// Number of fixes applied
    pub fix_count: u32,
    /// List of fixes that were applied
    pub fixes: Vec<String>,
    /// Errors remaining after fixes
    pub remaining_errors: Vec<SyntaxError>,
    /// Fixed file content (if fixes were applied)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_content: Option<String>,
}

/// Delimiter error detected by tree-sitter (internal use)
#[derive(Debug, Clone)]
pub struct DelimiterError {
    /// Line number where error occurs
    #[allow(dead_code)]
    pub line: usize,
    /// Column number where error occurs (for future use)
    #[allow(dead_code)]
    pub _column: usize,
    /// Missing delimiter character(s)
    pub missing_delimiter: String,
    /// Type of error (unmatched_brace, unclosed_string, etc.) (for future use)
    #[allow(dead_code)]
    pub _error_type: String,
}

/// Available refactoring operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefactorOperation {
    /// Rename a symbol across the codebase
    RenameSymbol,
    /// Extract selected code into a new function
    ExtractFunction,
    /// Replace the entire body/definition of a symbol (Serena-inspired)
    ReplaceSymbolBody,
    /// Insert code before or after a symbol
    InsertRelativeToSymbol,
    /// Extract inline types to named type definitions (TypeScript/Rust)
    ExtractType,
    /// Fix broken import statements after file moves
    UpdateImports,
    /// Inline a variable by replacing all uses with its value
    InlineVariable,
    /// Inline a function by replacing calls with function body
    InlineFunction,
    // ValidateSyntax removed - abandoned AutoFixSyntax feature (Oct 2025)
    // See commit e9ff6e9 - only 2/6 tests passing after days of work
}
