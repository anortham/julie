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

    let result = tool.smart_text_replace(
        source,
        "UserService",
        "AccountService",
        "test.ts",
        false,
    )?;

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
