// Tests extracted from src/tools/refactoring.rs
// These were previously inline tests that have been moved to follow project standards

use crate::tools::refactoring::*;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde_json::json;

#[test]
fn parse_refs_result_handles_confidence_suffix() {
    let tool = SmartRefactorTool {
        operation: "rename_symbol".to_string(),
        params: "{}".to_string(),
        dry_run: true,
    };

    let content = "🔗 Reference: OldSymbol - src/lib.rs:42 (confidence: 0.95)";
    let result = CallToolResult::text_content(vec![TextContent::from(content)]);

    let parsed = tool
        .parse_refs_result(&result)
        .expect("parse should succeed");
    let lines = parsed.get("src/lib.rs").expect("file should be captured");
    assert_eq!(lines, &vec![42]);
}

#[test]
fn parse_refs_result_prefers_structured_content() {
    let tool = SmartRefactorTool {
        operation: "rename_symbol".to_string(),
        params: "{}".to_string(),
        dry_run: true,
    };

    let structured = json!({
        "references": [
            {
                "file_path": "src/main.rs",
                "line_number": 128
            }
        ],
        "definitions": [
            {
                "file_path": "src/lib.rs",
                "start_line": 12
            }
        ]
    });

    let result = if let serde_json::Value::Object(map) = structured {
        CallToolResult::text_content(vec![]).with_structured_content(map)
    } else {
        panic!("expected structured object");
    };

    let parsed = tool
        .parse_refs_result(&result)
        .expect("parse should succeed");
    assert_eq!(parsed.get("src/main.rs"), Some(&vec![128]));
    assert_eq!(parsed.get("src/lib.rs"), Some(&vec![12]));
}
