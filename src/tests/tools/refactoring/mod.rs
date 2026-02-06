// Tests extracted from src/tools/refactoring.rs
// These were previously inline tests that have been moved to follow project standards

mod extract_to_file_tests;
mod import_update_tests;
mod insert_relative_tests;
mod replace_symbol_body_tests;
mod scope_test;

// New focused tools (Phase 2 - Tool Adoption Improvements)
mod edit_symbol;
mod rename_symbol;

// AST-aware refactoring tests (CRITICAL - verifies tree-sitter is actually used)
mod ast_aware;

use crate::tools::refactoring::*;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};

#[test]
fn parse_refs_result_handles_confidence_suffix() {
    let tool = SmartRefactorTool {
        operation: "rename_symbol".to_string(),
        params: "{}".to_string(),
        dry_run: true,
    };

    let content = "🔗 Reference: OldSymbol - src/lib.rs:42 (confidence: 0.95)";
    let result = CallToolResult::text_content(vec![Content::text(content)]);

    let parsed = tool
        .parse_refs_result(&result)
        .expect("parse should succeed");
    let lines = parsed.get("src/lib.rs").expect("file should be captured");
    assert_eq!(lines, &vec![42]);
}
