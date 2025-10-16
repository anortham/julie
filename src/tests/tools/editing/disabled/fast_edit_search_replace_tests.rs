//! Multi-file replacement tests for SafeEditTool multi_file_replace mode
//!
//! These tests verify that SafeEditTool's multi_file_replace mode safely performs
//! find/replace operations across multiple files using:
//! - fast_search: Find files/patterns matching criteria
//! - safe_edit multi_file_replace: Perform replacements on found files
//!
//! Following TDD methodology:
//! 1. RED: Write failing tests that define expected behavior
//! 2. GREEN: Implement SafeEditTool to make tests pass
//! 3. REFACTOR: Improve code while keeping tests green

use anyhow::Result;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Import SafeEditTool for testing
use crate::handler::JulieServerHandler;
use crate::tools::SafeEditTool;

/// Test fixture for SafeEditTool multi_file_replace mode tests
struct SafeEditTestFixture {
    temp_dir: TempDir,
}

impl SafeEditTestFixture {
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

    fn set_search_root(&self) -> EnvVarGuard {
        let joined = env::join_paths([self.temp_dir.path()])
            .expect("failed to join temp dir path for search root");
        let previous = env::var_os("FAST_EDIT_SEARCH_ROOTS");
        env::set_var("FAST_EDIT_SEARCH_ROOTS", &joined);
        EnvVarGuard {
            key: "FAST_EDIT_SEARCH_ROOTS",
            previous,
        }
    }
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(value) = self.previous.take() {
            env::set_var(self.key, value);
        } else {
            env::remove_var(self.key);
        }
    }
}

// SafeEditTool multi_file_replace mode:
// Searches across multiple files and performs safe replacements

#[cfg(test)]
mod backwards_compatibility_tests {
    use super::*;

    #[tokio::test]
    async fn test_original_single_file_mode_still_works() {
        let fixture = SafeEditTestFixture::new().unwrap();
        let _env_guard = fixture.set_search_root();
        let file_path = fixture.create_test_file("test.txt", "Hello world").unwrap();

        // SafeEditTool pattern_replace mode for single file
        let tool = SafeEditTool {
            file_path: file_path.to_string_lossy().to_string(),
            mode: "pattern_replace".to_string(),
            old_text: None,
            new_text: None,
            find_text: Some("Hello".to_string()),
            replace_text: Some("Hi".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: None,
            language: None,
            limit: None,
            dry_run: false,
            validate: true,
            preserve_indentation: true,
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

    #[ignore = "search_and_replace mode requires indexed search output"]
    #[tokio::test]
    async fn test_search_and_replace_across_multiple_files() {
        let fixture = SafeEditTestFixture::new().unwrap();
        let _env_guard = fixture.set_search_root();
        let file1 = fixture
            .create_test_file("src/file1.js", "const userName = 'test';")
            .unwrap();
        let file2 = fixture
            .create_test_file("src/file2.js", "function getUserName() {}")
            .unwrap();
        let file3 = fixture
            .create_test_file("test.py", "user_name = 'test'")
            .unwrap();

        // SafeEditTool multi_file_replace mode
        let tool = SafeEditTool {
            file_path: "".to_string(), // Empty triggers multi-file mode
            mode: "multi_file_replace".to_string(),
            old_text: None,
            new_text: None,
            find_text: Some("userName".to_string()),
            replace_text: Some("accountName".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: Some("src/**".to_string()),
            language: Some("javascript".to_string()),
            limit: Some(50),
            dry_run: false,
            validate: true,
            preserve_indentation: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should replace in JS files matching pattern
        let content1 = fixture.read_file(&file1).unwrap();
        let content2 = fixture.read_file(&file2).unwrap();
        let content3 = fixture.read_file(&file3).unwrap();

        assert!(
            content1.contains("accountName"),
            "content1 after edit: {}",
            content1
        );
        assert!(
            content2.contains("getAccountName"),
            "content2 after edit: {}",
            content2
        );
        assert!(
            content3.contains("user_name"),
            "content3 after edit: {}",
            content3
        ); // Python file unchanged

        // Should report multiple files processed
        let response = format!("{:?}", result);
        assert!(
            response.contains("2") || response.contains("files"),
            "response: {}",
            response
        );
    }

    #[ignore = "search_and_replace mode requires indexed search output"]
    #[tokio::test]
    async fn test_search_and_replace_dry_run() {
        let fixture = SafeEditTestFixture::new().unwrap();
        let _env_guard = fixture.set_search_root();
        let file1 = fixture
            .create_test_file("test1.js", "const hello = 'world';")
            .unwrap();
        let file2 = fixture
            .create_test_file("test2.js", "function hello() {}")
            .unwrap();

        let tool = SafeEditTool {
            file_path: "".to_string(),
            mode: "multi_file_replace".to_string(),
            old_text: None,
            new_text: None,
            find_text: Some("hello".to_string()),
            replace_text: Some("greeting".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: Some("*.js".to_string()),
            language: Some("javascript".to_string()),
            limit: Some(50),
            dry_run: true, // Should not modify files
            validate: true,
            preserve_indentation: true,
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
        assert!(
            response.contains("would replace") || response.contains("dry run"),
            "response: {}",
            response
        );
    }

    #[ignore = "search_and_replace mode requires indexed search output"]
    #[tokio::test]
    async fn test_search_and_replace_with_file_pattern() {
        let fixture = SafeEditTestFixture::new().unwrap();
        let _env_guard = fixture.set_search_root();
        let test_file = fixture
            .create_test_file("component.test.js", "const value = 'test';")
            .unwrap();
        let src_file = fixture
            .create_test_file("component.js", "const value = 'production';")
            .unwrap();

        let tool = SafeEditTool {
            file_path: "".to_string(),
            mode: "multi_file_replace".to_string(),
            old_text: None,
            new_text: None,
            find_text: Some("value".to_string()),
            replace_text: Some("data".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: Some("*.test.js".to_string()), // Only test files
            language: None,
            limit: Some(50),
            dry_run: false,
            validate: true,
            preserve_indentation: true,
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

    #[ignore = "search_and_replace mode requires indexed search output"]
    #[tokio::test]
    async fn test_delegates_to_fast_search_for_file_discovery() {
        let fixture = SafeEditTestFixture::new().unwrap();
        let _env_guard = fixture.set_search_root();
        let _file1 = fixture
            .create_test_file("utils/helper.ts", "export const API_URL = 'old';")
            .unwrap();
        let _file2 = fixture
            .create_test_file("components/Button.tsx", "const API_URL = 'old';")
            .unwrap();
        let _file3 = fixture
            .create_test_file("tests/api.test.js", "const API_URL = 'old';")
            .unwrap();

        let tool = SafeEditTool {
            file_path: "".to_string(),
            mode: "multi_file_replace".to_string(),
            old_text: None,
            new_text: None,
            find_text: Some("API_URL".to_string()),
            replace_text: Some("API_ENDPOINT".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: None,
            language: Some("typescript".to_string()), // Should delegate to fast_search
            limit: Some(50),
            dry_run: true, // Just test discovery
            validate: true,
            preserve_indentation: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should report TypeScript files found via fast_search delegation
        let response = format!("{:?}", result);
        assert!(
            response.contains(".ts") || response.contains(".tsx"),
            "response: {}",
            response
        );
        assert!(
            !response.contains(".js"),
            "response should not mention .js: {}",
            response
        ); // JavaScript files should be filtered out
    }

    #[ignore = "search_and_replace mode requires indexed search output"]
    #[tokio::test]
    async fn test_delegates_to_fast_edit_logic_for_replacements() {
        let fixture = SafeEditTestFixture::new().unwrap();
        let _env_guard = fixture.set_search_root();
        let file_path = fixture
            .create_test_file(
                "test.js",
                "function test() {\n  if (condition) {\n    return 'hello';\n  }\n}",
            )
            .unwrap();

        let tool = SafeEditTool {
            file_path: "".to_string(),
            mode: "multi_file_replace".to_string(),
            old_text: None,
            new_text: None,
            find_text: Some("hello".to_string()),
            replace_text: Some("world".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: Some("*.js".to_string()),
            language: None,
            limit: Some(50),
            dry_run: false,
            validate: true, // Should use safe_edit validation logic
            preserve_indentation: true,
            // Should use safe_edit backup logic
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should use fast_edit logic: backup, validation, etc.
        let backup_path = file_path.with_extension("js.backup");
        assert!(backup_path.exists(), "expected backup at {:?}", backup_path);

        let content = fixture.read_file(&file_path).unwrap();
        assert!(content.contains("world"), "content after edit: {}", content);
        assert!(
            !content.contains("hello"),
            "content should not contain original text: {}",
            content
        );

        // Should report like fast_edit with validation info
        let response = format!("{:?}", result);
        assert!(
            response.contains("replaced") || response.contains("validation"),
            "response: {}",
            response
        );
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_search_mode_requires_empty_file_path() {
        let _fixture = SafeEditTestFixture::new().unwrap();

        let tool = SafeEditTool {
            file_path: "/some/specific/file.js".to_string(), // Should be empty for multi-file mode
            mode: "multi_file_replace".to_string(),
            old_text: None,
            new_text: None,
            find_text: Some("test".to_string()),
            replace_text: Some("demo".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: Some("*.js".to_string()),
            language: None,
            limit: Some(50),
            dry_run: false,
            validate: true,
            preserve_indentation: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should report error about conflicting parameters
        let response = format!("{:?}", result);
        assert!(response.contains("error") || response.contains("empty file_path"));
    }

    #[tokio::test]
    async fn test_single_file_mode_requires_file_path() {
        let _fixture = SafeEditTestFixture::new().unwrap();

        let tool = SafeEditTool {
            file_path: "".to_string(), // Empty triggers multi-file mode
            mode: "pattern_replace".to_string(), // Use pattern_replace for single file behavior
            old_text: None,
            new_text: None,
            find_text: Some("test".to_string()),
            replace_text: Some("demo".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: None,
            language: None,
            limit: None,
            dry_run: false,
            validate: true,
            preserve_indentation: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should report error about missing file_path
        let response = format!("{:?}", result);
        assert!(response.contains("error") || response.contains("file_path"));
    }

    #[tokio::test]
    async fn test_no_files_found_in_search_mode() {
        let fixture = SafeEditTestFixture::new().unwrap();
        let _env_guard = fixture.set_search_root();
        let _file = fixture
            .create_test_file("test.py", "print('hello')")
            .unwrap();

        let tool = SafeEditTool {
            file_path: "".to_string(),
            mode: "multi_file_replace".to_string(),
            old_text: None,
            new_text: None,
            find_text: Some("hello".to_string()),
            replace_text: Some("world".to_string()),
            line_number: None,
            start_line: None,
            end_line: None,
            content: None,
            file_pattern: None,
            language: Some("javascript".to_string()), // No JS files exist
            limit: Some(50),
            dry_run: false,
            validate: true,
            preserve_indentation: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should report no files found
        let response = format!("{:?}", result);
        assert!(response.contains("no files found") || response.contains("0 files"));
    }
}
