//! Unit tests for import update regex patterns
//!
//! These tests verify the regex patterns in update_imports_in_file() work correctly
//! by testing the logic in isolation (without requiring indexed files).

use anyhow::Result;

/// Direct test of update_imports_in_file logic
/// This bypasses the full rename flow and tests import regex patterns directly
async fn test_import_update_logic(source: &str, old_name: &str, new_name: &str) -> Result<String> {
    use regex::Regex;

    let mut modified_content = source.to_string();

    // This is the EXACT logic from update_imports_in_file (lines 298-329 in rename.rs)
    let patterns = vec![
        // JavaScript/TypeScript: import { getUserData } from 'module'
        Regex::new(&format!(
            r"\bimport\s+\{{\s*{}\s*\}}",
            regex::escape(old_name)
        ))?,
        // JavaScript/TypeScript: import { getUserData, other } (leading position)
        Regex::new(&format!(
            r"\bimport\s+\{{\s*{}\s*,",
            regex::escape(old_name)
        ))?,
        // JavaScript/TypeScript: import { other, getUserData } (trailing position)
        Regex::new(&format!(r",\s*{}\s*\}}", regex::escape(old_name)))?,
        // Python: from module import getUserData (word boundary)
        Regex::new(&format!(
            r"\bfrom\s+\S+\s+import\s+{}\b",
            regex::escape(old_name)
        ))?,
        // Rust: use module::getUserData (word boundary)
        Regex::new(&format!(r"\buse\s+.*::{}\b", regex::escape(old_name)))?,
    ];

    for regex in patterns {
        if regex.is_match(&modified_content) {
            modified_content = regex
                .replace_all(&modified_content, |caps: &regex::Captures| {
                    caps[0].replace(old_name, new_name)
                })
                .to_string();
        }
    }

    Ok(modified_content)
}

#[tokio::test]
async fn test_import_update_named_import_only() -> Result<()> {
    // Named import that should be updated
    let source = r#"import { getUserData } from './api';

const result = getUserData(123);
"#;

    let updated = test_import_update_logic(source, "getUserData", "fetchUserData").await?;

    // CRITICAL: Import should be updated
    assert!(
        updated.contains("import { fetchUserData }"),
        "Import statement should be updated. Got:\n{}",
        updated
    );
    assert!(
        !updated.contains("import { getUserData }"),
        "Old import should be removed"
    );

    // Code usage should NOT be changed by import update (that's done separately)
    assert!(updated.contains("getUserData(123)"));

    Ok(())
}

#[tokio::test]
async fn test_import_update_multiple_named_imports() -> Result<()> {
    // Multiple imports - only one should be renamed
    let source = r#"import { getUserData, saveUserData, deleteUser } from './api';

const result = getUserData(123);
"#;

    let updated = test_import_update_logic(source, "getUserData", "fetchUserData").await?;

    // CRITICAL: Only getUserData import should be renamed, others preserved
    assert!(
        updated.contains("import { fetchUserData"),
        "Renamed import should be present"
    );
    assert!(
        updated.contains("saveUserData"),
        "Other imports should be preserved"
    );
    assert!(
        updated.contains("deleteUser"),
        "Other imports should be preserved"
    );
    assert!(
        !updated.contains("import { getUserData"),
        "Old import should be replaced"
    );

    Ok(())
}

#[tokio::test]
async fn test_import_update_avoids_partial_matches() -> Result<()> {
    // getUserData vs getUserDataFromCache - should NOT match
    let source = r#"import { getUserData, getUserDataFromCache } from './api';

const result = getUserData(123);
const cached = getUserDataFromCache(123);
"#;

    let updated = test_import_update_logic(source, "getUserData", "fetchUserData").await?;

    // CRITICAL: getUserDataFromCache should NOT be affected
    assert!(
        updated.contains("getUserDataFromCache"),
        "Similar named import should not be affected. Got:\n{}",
        updated
    );
    assert!(
        updated.contains("fetchUserData,"),
        "Only exact match should be renamed"
    );
    assert!(
        !updated.contains("getUserData,"),
        "Old exact match should be gone"
    );

    Ok(())
}

#[tokio::test]
async fn test_import_update_python_from_import() -> Result<()> {
    let source = r#"from api import getUserData

result = getUserData(123)
"#;

    let updated = test_import_update_logic(source, "getUserData", "fetch_user_data").await?;

    // Python import should be updated
    assert!(
        updated.contains("from api import fetch_user_data"),
        "Python import should be updated. Got:\n{}",
        updated
    );
    assert!(
        !updated.contains("from api import getUserData"),
        "Old import should be replaced"
    );

    Ok(())
}

#[tokio::test]
async fn test_import_update_rust_use_statement() -> Result<()> {
    let source = r#"use crate::api::getUserData;

fn main() {
    let result = getUserData(123);
}
"#;

    let updated = test_import_update_logic(source, "getUserData", "fetch_user_data").await?;

    // Rust use statement should be updated
    assert!(
        updated.contains("use crate::api::fetch_user_data"),
        "Rust use statement should be updated. Got:\n{}",
        updated
    );
    assert!(
        !updated.contains("use crate::api::getUserData"),
        "Old use statement should be replaced"
    );
    // Note: Code usage (getUserData in function body) is unchanged - that's tested separately

    Ok(())
}

#[tokio::test]
async fn test_import_update_does_not_affect_module_path() -> Result<()> {
    // EDGE CASE: Symbol name appears in module path
    let source = r#"import { getUserData } from './getUserData/api';

const result = getUserData(123);
"#;

    let updated = test_import_update_logic(source, "getUserData", "fetchUserData").await?;

    // CRITICAL: Module path should NOT be affected, only the imported symbol
    assert!(
        updated.contains("from './getUserData/api'"),
        "Module path should not be modified. Got:\n{}",
        updated
    );
    assert!(
        updated.contains("import { fetchUserData }"),
        "Imported symbol should be renamed"
    );

    Ok(())
}
