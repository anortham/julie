//! TDD tests for FastEditTool search_and_replace enhancement
//!
//! CRITICAL: These tests define the enhanced FastEditTool behavior BEFORE implementation
//!
//! Enhancement: Add search_and_replace capability that delegates to:
//! - fast_search: Find files/patterns matching criteria
//! - fast_edit: Perform replacements on found files
//!
//! Following TDD methodology:
//! 1. RED: Write failing tests that define expected behavior
//! 2. GREEN: Enhance FastEditTool to make tests pass
//! 3. REFACTOR: Improve code while keeping tests green

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Import enhanced FastEditTool for testing
use crate::handler::JulieServerHandler;
use crate::tools::editing::FastEditTool;

/// Test fixture for enhanced FastEditTool tests
struct FastEditTestFixture {
    temp_dir: TempDir,
}

impl FastEditTestFixture {
    fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    fn create_test_file(&self, name: &str, content: &str) -> Result<PathBuf> {
        let file_path = self.temp_dir.path().join(name);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file_path, content)?;
        Ok(file_path)
    }

    fn read_file(&self, path: &PathBuf) -> Result<String> {
        Ok(fs::read_to_string(path)?)
    }
}

// Enhanced FastEditTool should support both modes:
// 1. Original mode: single file editing (existing behavior)
// 2. New mode: search_and_replace across multiple files

#[cfg(test)]
mod backwards_compatibility_tests {
    use super::*;

    #[tokio::test]
    async fn test_original_single_file_mode_still_works() {
        let fixture = FastEditTestFixture::new().unwrap();
        let file_path = fixture.create_test_file("test.txt", "Hello world").unwrap();

        // Original FastEditTool usage should still work
        let tool = FastEditTool {
            file_path: file_path.to_string_lossy().to_string(),
            find_text: "Hello".to_string(),
            replace_text: "Hi".to_string(),
            // Enhanced fields should have sensible defaults
            mode: None, // None = original single-file mode
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,
            backup: true,
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should work exactly like before
        let content = fixture.read_file(&file_path).unwrap();
        assert_eq!(content, "Hi world");

        let response = format!("{:?}", result);
        assert!(response.contains("replaced"));
    }
}

#[cfg(test)]
mod search_and_replace_mode_tests {
    use super::*;

    #[tokio::test]
    async fn test_search_and_replace_across_multiple_files() {
        let fixture = FastEditTestFixture::new().unwrap();
        let file1 = fixture
            .create_test_file("src/file1.js", "const userName = 'test';")
            .unwrap();
        let file2 = fixture
            .create_test_file("src/file2.js", "function getUserName() {}")
            .unwrap();
        let file3 = fixture
            .create_test_file("test.py", "user_name = 'test'")
            .unwrap();

        // New search_and_replace mode
        let tool = FastEditTool {
            file_path: "".to_string(), // Empty for search mode
            find_text: "userName".to_string(),
            replace_text: "accountName".to_string(),
            mode: Some("search_and_replace".to_string()), // New mode
            language: Some("javascript".to_string()),
            file_pattern: Some("src/**".to_string()),
            limit: Some(50),
            validate: true,
            backup: true,
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should replace in JS files matching pattern
        let content1 = fixture.read_file(&file1).unwrap();
        let content2 = fixture.read_file(&file2).unwrap();
        let content3 = fixture.read_file(&file3).unwrap();

        assert!(content1.contains("accountName"));
        assert!(content2.contains("getAccountName"));
        assert!(content3.contains("user_name")); // Python file unchanged

        // Should report multiple files processed
        let response = format!("{:?}", result);
        assert!(response.contains("2") || response.contains("files"));
    }

    #[tokio::test]
    async fn test_search_and_replace_dry_run() {
        let fixture = FastEditTestFixture::new().unwrap();
        let file1 = fixture
            .create_test_file("test1.js", "const hello = 'world';")
            .unwrap();
        let file2 = fixture
            .create_test_file("test2.js", "function hello() {}")
            .unwrap();

        let tool = FastEditTool {
            file_path: "".to_string(),
            find_text: "hello".to_string(),
            replace_text: "greeting".to_string(),
            mode: Some("search_and_replace".to_string()),
            language: Some("javascript".to_string()),
            file_pattern: Some("*.js".to_string()),
            limit: Some(50),
            validate: true,
            backup: true,
            dry_run: true, // Should not modify files
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Files should be unchanged in dry run
        let content1 = fixture.read_file(&file1).unwrap();
        let content2 = fixture.read_file(&file2).unwrap();

        assert!(content1.contains("hello"));
        assert!(content2.contains("hello"));
        assert!(!content1.contains("greeting"));
        assert!(!content2.contains("greeting"));

        // Should report what would be changed
        let response = format!("{:?}", result);
        assert!(response.contains("would replace") || response.contains("dry run"));
    }

    #[tokio::test]
    async fn test_search_and_replace_with_file_pattern() {
        let fixture = FastEditTestFixture::new().unwrap();
        let test_file = fixture
            .create_test_file("component.test.js", "const value = 'test';")
            .unwrap();
        let src_file = fixture
            .create_test_file("component.js", "const value = 'production';")
            .unwrap();

        let tool = FastEditTool {
            file_path: "".to_string(),
            find_text: "value".to_string(),
            replace_text: "data".to_string(),
            mode: Some("search_and_replace".to_string()),
            language: None,
            file_pattern: Some("*.test.js".to_string()), // Only test files
            limit: Some(50),
            validate: true,
            backup: true,
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let _result = tool.call_tool(&handler).await.unwrap();

        // Should only modify test files
        let test_content = fixture.read_file(&test_file).unwrap();
        let src_content = fixture.read_file(&src_file).unwrap();

        assert!(test_content.contains("data"));
        assert!(!src_content.contains("data"));
        assert!(src_content.contains("value")); // unchanged
    }
}

#[cfg(test)]
mod delegation_behavior_tests {
    use super::*;

    #[tokio::test]
    async fn test_delegates_to_fast_search_for_file_discovery() {
        let fixture = FastEditTestFixture::new().unwrap();
        let _file1 = fixture
            .create_test_file("utils/helper.ts", "export const API_URL = 'old';")
            .unwrap();
        let _file2 = fixture
            .create_test_file("components/Button.tsx", "const API_URL = 'old';")
            .unwrap();
        let _file3 = fixture
            .create_test_file("tests/api.test.js", "const API_URL = 'old';")
            .unwrap();

        let tool = FastEditTool {
            file_path: "".to_string(),
            find_text: "API_URL".to_string(),
            replace_text: "API_ENDPOINT".to_string(),
            mode: Some("search_and_replace".to_string()),
            language: Some("typescript".to_string()), // Should delegate to fast_search
            file_pattern: None,
            limit: Some(50),
            validate: true,
            backup: true,
            dry_run: true, // Just test discovery
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should report TypeScript files found via fast_search delegation
        let response = format!("{:?}", result);
        assert!(response.contains(".ts") || response.contains(".tsx"));
        assert!(!response.contains(".js")); // JavaScript files should be filtered out
    }

    #[tokio::test]
    async fn test_delegates_to_fast_edit_logic_for_replacements() {
        let fixture = FastEditTestFixture::new().unwrap();
        let file_path = fixture
            .create_test_file(
                "test.js",
                "function test() {\n  if (condition) {\n    return 'hello';\n  }\n}",
            )
            .unwrap();

        let tool = FastEditTool {
            file_path: "".to_string(),
            find_text: "hello".to_string(),
            replace_text: "world".to_string(),
            mode: Some("search_and_replace".to_string()),
            language: None,
            file_pattern: Some("*.js".to_string()),
            limit: Some(50),
            validate: true, // Should use fast_edit validation logic
            backup: true,   // Should use fast_edit backup logic
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should use fast_edit logic: backup, validation, etc.
        let backup_path = file_path.with_extension("js.backup");
        assert!(backup_path.exists(), "Should create backup like fast_edit");

        let content = fixture.read_file(&file_path).unwrap();
        assert!(content.contains("world"));
        assert!(!content.contains("hello"));

        // Should report like fast_edit with validation info
        let response = format!("{:?}", result);
        assert!(response.contains("replaced") || response.contains("validation"));
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_search_mode_requires_empty_file_path() {
        let _fixture = FastEditTestFixture::new().unwrap();

        let tool = FastEditTool {
            file_path: "/some/specific/file.js".to_string(), // Should be empty for search mode
            find_text: "test".to_string(),
            replace_text: "demo".to_string(),
            mode: Some("search_and_replace".to_string()),
            language: None,
            file_pattern: Some("*.js".to_string()),
            limit: Some(50),
            validate: true,
            backup: true,
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should report error about conflicting parameters
        let response = format!("{:?}", result);
        assert!(response.contains("error") || response.contains("empty file_path"));
    }

    #[tokio::test]
    async fn test_single_file_mode_requires_file_path() {
        let _fixture = FastEditTestFixture::new().unwrap();

        let tool = FastEditTool {
            file_path: "".to_string(), // Empty for single file mode is invalid
            find_text: "test".to_string(),
            replace_text: "demo".to_string(),
            mode: None, // Single file mode
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,
            backup: true,
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should report error about missing file_path
        let response = format!("{:?}", result);
        assert!(response.contains("error") || response.contains("file_path"));
    }

    #[tokio::test]
    async fn test_no_files_found_in_search_mode() {
        let fixture = FastEditTestFixture::new().unwrap();
        let _file = fixture
            .create_test_file("test.py", "print('hello')")
            .unwrap();

        let tool = FastEditTool {
            file_path: "".to_string(),
            find_text: "hello".to_string(),
            replace_text: "world".to_string(),
            mode: Some("search_and_replace".to_string()),
            language: Some("javascript".to_string()), // No JS files exist
            file_pattern: None,
            limit: Some(50),
            validate: true,
            backup: true,
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should report no files found
        let response = format!("{:?}", result);
        assert!(response.contains("no files found") || response.contains("0 files"));
    }
}
