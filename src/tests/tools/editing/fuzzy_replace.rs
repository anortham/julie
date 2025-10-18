//! Unit tests for FuzzyReplaceTool implementation
//!
//! These tests verify the core fuzzy matching logic:
//! - Levenshtein distance similarity calculation
//! - Character-based fuzzy search and replace
//! - Balance calculation for validation
//! - Edge cases (empty strings, UTF-8, long patterns)

use crate::tools::fuzzy_replace::FuzzyReplaceTool;
use anyhow::Result;

#[test]
fn test_calculate_similarity_identical() {
    let tool = create_tool("test", "test");
    let similarity = tool.calculate_similarity("hello", "hello");
    assert_eq!(
        similarity, 1.0,
        "Identical strings should have 1.0 similarity"
    );
}

#[test]
fn test_calculate_similarity_one_char_diff() {
    let tool = create_tool("test", "test");

    // "hello" vs "hallo" - 1 substitution out of 5 chars
    let similarity = tool.calculate_similarity("hello", "hallo");
    assert!(
        (similarity - 0.8).abs() < 0.01,
        "One char diff in 5 should be ~0.8 similarity"
    );
}

#[test]
fn test_calculate_similarity_insertion() {
    let tool = create_tool("test", "test");

    // "getUserData" vs "getUserDat" - 1 char deletion
    let similarity = tool.calculate_similarity("getUserData", "getUserDat");
    assert!(
        similarity > 0.9,
        "One char deletion should be >0.9 similarity, got {}",
        similarity
    );
}

#[test]
fn test_calculate_similarity_different_lengths() {
    let tool = create_tool("test", "test");

    // "hi" vs "hello" - very different
    let similarity = tool.calculate_similarity("hi", "hello");
    assert!(
        similarity < 0.5,
        "Very different strings should have low similarity"
    );
}

#[test]
fn test_calculate_similarity_empty_strings() {
    let tool = create_tool("test", "test");

    assert_eq!(
        tool.calculate_similarity("", ""),
        1.0,
        "Empty strings are identical"
    );
    assert_eq!(
        tool.calculate_similarity("hello", ""),
        0.0,
        "Empty vs non-empty is 0.0"
    );
    assert_eq!(
        tool.calculate_similarity("", "world"),
        0.0,
        "Empty vs non-empty is 0.0"
    );
}

#[test]
fn test_fuzzy_search_replace_exact_match() -> Result<()> {
    let tool = create_tool("getUserData", "fetchUserData");

    let content = "function getUserData() { return data; }";
    let (result, matches) = tool.fuzzy_search_replace(content)?;

    assert_eq!(matches, 1, "Should find 1 exact match");
    assert!(
        result.contains("fetchUserData"),
        "Should replace with new name"
    );
    assert!(!result.contains("getUserData"), "Should remove old name");

    Ok(())
}

#[test]
fn test_fuzzy_search_replace_multiple_matches() -> Result<()> {
    let tool = create_tool("test", "TEST");

    let content = "test test test";
    let (result, matches) = tool.fuzzy_search_replace(content)?;

    assert_eq!(matches, 3, "Should find all 3 matches");
    assert_eq!(result, "TEST TEST TEST", "Should replace all occurrences");

    Ok(())
}

#[test]
fn test_fuzzy_search_replace_with_typo() -> Result<()> {
    let mut tool = create_tool("getUserData", "fetchUserData");
    tool.threshold = 0.85; // Allow 15% difference

    let content = "function getUserDat() {}"; // Missing 'a'
    let (result, matches) = tool.fuzzy_search_replace(content)?;

    assert_eq!(matches, 1, "Should find fuzzy match despite typo");
    assert!(
        result.contains("fetchUserData"),
        "Should replace fuzzy match"
    );

    Ok(())
}

#[test]
fn test_fuzzy_search_replace_no_matches() -> Result<()> {
    let tool = create_tool("getUserData", "fetchUserData");

    let content = "function completely_different() {}";
    let (result, matches) = tool.fuzzy_search_replace(content)?;

    assert_eq!(matches, 0, "Should find no matches");
    assert_eq!(result, content, "Content should be unchanged");

    Ok(())
}

#[test]
fn test_fuzzy_search_replace_threshold_filtering() -> Result<()> {
    let mut tool = create_tool("hello", "HELLO");
    tool.threshold = 0.95; // Very strict

    // "hello" vs "hallo" is ~0.8 similarity, below threshold
    let content = "hallo world";
    let (result, matches) = tool.fuzzy_search_replace(content)?;

    assert_eq!(matches, 0, "Should not match below threshold");
    assert_eq!(result, content, "Content should be unchanged");

    Ok(())
}

#[test]
fn test_fuzzy_search_replace_empty_content() -> Result<()> {
    let tool = create_tool("test", "TEST");

    let (result, matches) = tool.fuzzy_search_replace("")?;

    assert_eq!(matches, 0, "Empty content has no matches");
    assert_eq!(result, "", "Result should be empty");

    Ok(())
}

#[test]
fn test_fuzzy_search_replace_pattern_longer_than_content() -> Result<()> {
    let tool = create_tool("this is a very long pattern", "short");

    let content = "hi";
    let (result, matches) = tool.fuzzy_search_replace(content)?;

    assert_eq!(matches, 0, "Pattern longer than content can't match");
    assert_eq!(result, content, "Content should be unchanged");

    Ok(())
}

#[test]
fn test_fuzzy_search_replace_utf8_safety() -> Result<()> {
    let tool = create_tool("café", "CAFÉ");

    let content = "I love café ☕";
    let (result, matches) = tool.fuzzy_search_replace(content)?;

    assert_eq!(matches, 1, "Should handle UTF-8 characters");
    assert!(result.contains("CAFÉ"), "Should replace UTF-8 text");
    assert!(result.contains("☕"), "Should preserve other UTF-8 chars");

    Ok(())
}

#[test]
fn test_calculate_balance_balanced() {
    let tool = create_tool("test", "test");

    let content = "fn test() { let x = [1, 2]; }";
    let (braces, brackets, parens) = tool.calculate_balance(content);

    assert_eq!(braces, 0, "Braces should be balanced");
    assert_eq!(brackets, 0, "Brackets should be balanced");
    assert_eq!(parens, 0, "Parens should be balanced");
}

#[test]
fn test_calculate_balance_unbalanced_braces() {
    let tool = create_tool("test", "test");

    let content = "fn test() { { missing close";
    let (braces, brackets, parens) = tool.calculate_balance(content);

    assert_eq!(braces, 2, "Should count unclosed braces");
    assert_eq!(brackets, 0, "Brackets should be balanced");
    assert_eq!(parens, 0, "Parens should be balanced");
}

#[test]
fn test_calculate_balance_unbalanced_close() {
    let tool = create_tool("test", "test");

    let content = "} extra close";
    let (braces, _brackets, _parens) = tool.calculate_balance(content);

    assert_eq!(braces, -1, "Should count extra closing brace");
}

#[test]
fn test_calculate_balance_mixed() {
    let tool = create_tool("test", "test");

    let content = "{ [ ( ) ] }";
    let (braces, brackets, parens) = tool.calculate_balance(content);

    assert_eq!(braces, 0, "All balanced");
    assert_eq!(brackets, 0, "All balanced");
    assert_eq!(parens, 0, "All balanced");
}

#[test]
fn test_calculate_balance_in_strings() {
    let tool = create_tool("test", "test");

    // Note: Balance check is simple and doesn't parse strings
    // This is intentional - we only check if replacement changes balance
    let content = r#"let s = "{ [ (";"#;
    let (braces, brackets, parens) = tool.calculate_balance(content);

    // These are in a string, but balance check counts them anyway
    // That's OK because we compare original vs modified balance
    assert_eq!(braces, 1, "Counts braces even in strings");
    assert_eq!(brackets, 1, "Counts brackets even in strings");
    assert_eq!(parens, 1, "Counts parens even in strings");
}

// ===== SECURITY TESTS =====

#[cfg(test)]
mod security_tests {
    use super::*;
    use crate::handler::JulieServerHandler;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_fuzzy_replace_path_traversal_prevention_absolute_path() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        // Try to access /etc/passwd using absolute path
        let edit_tool = FuzzyReplaceTool {
            file_path: "/etc/passwd".to_string(),
            pattern: "root".to_string(),
            replacement: "hacked".to_string(),
            threshold: 1.0,
            distance: 1000,
            dry_run: false,
            validate: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        // Should fail with security error
        assert!(result.is_err(), "Absolute path outside workspace should be blocked");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Security") || error_msg.contains("traversal"),
            "Error should mention security/traversal: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_fuzzy_replace_path_traversal_prevention_relative_traversal() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        // Try to access ../../../../etc/passwd using relative path traversal
        let edit_tool = FuzzyReplaceTool {
            file_path: "../../../../etc/passwd".to_string(),
            pattern: "root".to_string(),
            replacement: "hacked".to_string(),
            threshold: 1.0,
            distance: 1000,
            dry_run: false,
            validate: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        // Should fail with security error or path not found (both secure outcomes)
        assert!(result.is_err(), "Relative path traversal should be blocked");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Security") || error_msg.contains("traversal") || error_msg.contains("does not exist"),
            "Error should indicate security block or non-existent path: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_fuzzy_replace_secure_path_resolution_valid_paths() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "Hello world, this is a test file.")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        // Valid absolute path should work
        let edit_tool = FuzzyReplaceTool {
            file_path: test_file.to_string_lossy().to_string(),
            pattern: "world".to_string(),
            replacement: "universe".to_string(),
            threshold: 1.0,
            distance: 1000,
            dry_run: false,
            validate: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        // Should succeed
        assert!(result.is_ok(), "Valid relative path should work: {:?}", result);

        // Verify the file was actually modified
        let content = fs::read_to_string(&test_file)?;
        assert!(content.contains("universe"), "File should contain replacement text");

        Ok(())
    }
}

// Helper function to create a tool with basic params
fn create_tool(pattern: &str, replacement: &str) -> FuzzyReplaceTool {
    FuzzyReplaceTool {
        file_path: "/tmp/test.txt".to_string(),
        pattern: pattern.to_string(),
        replacement: replacement.to_string(),
        threshold: 1.0, // Default to exact match
        distance: 1000,
        dry_run: false,
        validate: true,
    }
}
