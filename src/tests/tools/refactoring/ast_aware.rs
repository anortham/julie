//! Test that tree-sitter AST is actually used for smart refactoring
//!
//! CRITICAL: These tests verify Julie uses AST-aware refactoring, not regex.
//! If these tests pass with simple text replacement, we've broken the core value prop!

use crate::tools::refactoring::SmartRefactorTool;
use anyhow::Result;

#[test]
fn test_ast_aware_rename_skips_string_literals() -> Result<()> {
    let source = r#"
class UserService {
    constructor() {
        this.name = "UserService"; // Should NOT be renamed - it's a string!
        console.log("Initializing UserService instance"); // Should NOT be renamed
    }
}
"#;

    let tool = SmartRefactorTool {
        operation: "rename_symbol".to_string(),
        params: r#"{"old_name": "UserService", "new_name": "AccountService"}"#.to_string(),
        dry_run: false,
    };

    let result = tool.smart_text_replace(
        source,
        "UserService",
        "AccountService",
        "test.ts",
        false, // Don't update comments
    )?;

    // Verify class name was renamed
    assert!(
        result.contains("class AccountService"),
        "Class name should be renamed"
    );

    // CRITICAL: String literals should NOT be changed
    assert!(
        result.contains(r#"this.name = "UserService""#),
        "String literal should NOT be renamed - AST should skip strings!\nGot: {}",
        result
    );
    assert!(
        result.contains(r#""Initializing UserService instance""#),
        "Console.log string should NOT be renamed!\nGot: {}",
        result
    );

    Ok(())
}

#[test]
fn test_ast_aware_rename_skips_comments() -> Result<()> {
    let source = r#"
// UserService handles user management
class UserService {
    /* The UserService class provides... */
    run() {}
}
"#;

    let tool = SmartRefactorTool {
        operation: "rename_symbol".to_string(),
        params: r#"{"old_name": "UserService", "new_name": "AccountService"}"#.to_string(),
        dry_run: false,
    };

    let result = tool.smart_text_replace(
        source,
        "UserService",
        "AccountService",
        "test.ts",
        false, // Don't update comments
    )?;

    // Verify class name was renamed
    assert!(
        result.contains("class AccountService"),
        "Class name should be renamed"
    );

    // CRITICAL: Comments should NOT be changed when update_comments=false
    assert!(
        result.contains("// UserService handles user management"),
        "Single-line comment should NOT be renamed when update_comments=false!\nGot: {}",
        result
    );
    assert!(
        result.contains("/* The UserService class provides... */"),
        "Multi-line comment should NOT be renamed when update_comments=false!\nGot: {}",
        result
    );

    Ok(())
}

#[test]
fn test_ast_aware_rename_changes_only_identifiers() -> Result<()> {
    let source = r#"
import { UserService } from './user';

const service = new UserService();
const result = service.getUserData();
"#;

    let tool = SmartRefactorTool {
        operation: "rename_symbol".to_string(),
        params: r#"{"old_name": "UserService", "new_name": "AccountService"}"#.to_string(),
        dry_run: false,
    };

    let result =
        tool.smart_text_replace(source, "UserService", "AccountService", "test.ts", false)?;

    // All identifier references should be renamed
    assert!(
        result.contains("import { AccountService }"),
        "Import should be renamed"
    );
    assert!(
        result.contains("new AccountService()"),
        "Constructor call should be renamed"
    );

    // But method names should NOT be affected
    assert!(
        result.contains("getUserData"),
        "Method names should not be affected by UserService rename"
    );

    Ok(())
}

// Regression: files with unknown/unsupported extension (e.g. .env, .cfg, .ini) have no
// tree-sitter parser. smart_text_replace should fall back to plain text replacement
// rather than returning an error.
#[test]
fn test_smart_text_replace_unknown_language_falls_back_to_plain_text() -> Result<()> {
    // .env files have no tree-sitter parser -- detect_language returns "unknown"
    let content = "DATABASE_HOST=old_host\nREPLICA_HOST=old_host_replica\n";

    let tool = SmartRefactorTool {
        operation: "rename_symbol".to_string(),
        params: r#"{"old_name": "old_host", "new_name": "new_host"}"#.to_string(),
        dry_run: false,
    };

    let result = tool.smart_text_replace(content, "old_host", "new_host", ".env", false);

    assert!(
        result.is_ok(),
        "unknown language should fall back to plain text, not error: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("new_host"),
        "plain-text replacement should have applied to .env file: {}",
        output
    );

    Ok(())
}
