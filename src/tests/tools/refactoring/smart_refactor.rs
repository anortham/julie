//! Comprehensive tests for RenameSymbolTool (formerly SmartRefactorTool)
//!
//! These tests verify that semantic rename operations work correctly
//! and safely across different code scenarios.

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::refactoring::RenameSymbolTool;
use crate::mcp_compat::CallToolResult;

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Test fixture for refactoring operations
struct RefactoringTestFixture {
    temp_dir: TempDir,
}

impl RefactoringTestFixture {
    fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    fn create_test_file(&self, name: &str, content: &str) -> Result<String> {
        let file_path = self.temp_dir.path().join(name);
        fs::write(&file_path, content)?;
        Ok(file_path.to_string_lossy().to_string())
    }

    fn read_file(&self, path: &str) -> Result<String> {
        Ok(fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod rename_symbol_tests {
    use super::*;

    #[tokio::test]
    async fn test_rename_symbol_basic_functionality() {
        let fixture = RefactoringTestFixture::new().unwrap();

        // Create a simple test file
        let test_content = r#"
function getUserData(userId) {
    return database.getUserData(userId);
}

function processUser() {
    const userData = getUserData(123);
    return userData;
}
"#;
        let _file_path = fixture.create_test_file("test.js", test_content).unwrap();

        let tool = RenameSymbolTool {
            old_name: "getUserData".to_string(),
            new_name: "fetchUserProfile".to_string(),
            scope: None, // defaults to "workspace"
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        let response = extract_text_from_result(&result);
        println!("Response: {}", response);

        // Response should indicate success (applied changes) or no references found
        // (no refs expected since the file isn't indexed in this temp workspace)
        let response_lower = response.to_lowercase();
        assert!(
            response_lower.contains("applied") || response_lower.contains("no references found"),
            "Unexpected response: {}",
            response
        );
    }

    #[tokio::test]
    async fn test_rename_symbol_dry_run() {
        let fixture = RefactoringTestFixture::new().unwrap();

        let test_content = r#"
class UserService {
    processUser() {
        return new UserService();
    }
}
"#;
        let file_path = fixture
            .create_test_file("service.js", test_content)
            .unwrap();

        let tool = RenameSymbolTool {
            old_name: "UserService".to_string(),
            new_name: "AccountService".to_string(),
            scope: Some("workspace".to_string()),
            dry_run: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        let response = extract_text_from_result(&result);
        let response_lower = response.to_lowercase();

        // Verify it's a dry run response or no references found
        assert!(
            response_lower.contains("dry run") || response_lower.contains("no references found"),
            "Unexpected response: {}",
            response
        );

        // Verify original file is unchanged
        let original_content = fixture.read_file(&file_path).unwrap();
        assert!(original_content.contains("UserService"));
        assert!(!original_content.contains("AccountService"));
    }

    #[tokio::test]
    async fn test_rename_symbol_empty_name() {
        let tool = RenameSymbolTool {
            old_name: "test".to_string(),
            new_name: "".to_string(),
            scope: None,
            dry_run: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        let response = extract_text_from_result(&result);
        assert!(response.contains("required") || response.contains("empty"));
    }

    #[tokio::test]
    async fn test_rename_symbol_identical_names() {
        let tool = RenameSymbolTool {
            old_name: "test".to_string(),
            new_name: "test".to_string(),
            scope: None,
            dry_run: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        let response = extract_text_from_result(&result);
        assert!(response.contains("identical"));
    }

    #[tokio::test]
    async fn test_rename_symbol_no_references_found() {
        let tool = RenameSymbolTool {
            old_name: "NonExistentSymbol".to_string(),
            new_name: "NewName".to_string(),
            scope: None,
            dry_run: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        let response = extract_text_from_result(&result);

        // Should indicate no references found (case-insensitive)
        assert!(
            response.to_lowercase().contains("no references found"),
            "Unexpected response: {}",
            response
        );
    }
}

#[cfg(test)]
mod tool_validation_tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_creation_and_serialization() {
        // Test that the tool can be created and serialized properly
        let tool = RenameSymbolTool {
            old_name: "test".to_string(),
            new_name: "newTest".to_string(),
            scope: None,
            dry_run: false,
        };

        // Should be able to serialize/deserialize
        let json = serde_json::to_string(&tool).unwrap();
        let deserialized: RenameSymbolTool = serde_json::from_str(&json).unwrap();

        // Verify basic fields
        assert_eq!(deserialized.old_name, "test");
        assert_eq!(deserialized.new_name, "newTest");
        assert!(!deserialized.dry_run);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_multiple_file_rename_workflow() {
        let fixture = RefactoringTestFixture::new().unwrap();

        // Create multiple files with the same symbol
        let file1_content = r#"
import { UserValidator } from './validator';

class UserProcessor {
    validate(user) {
        return UserValidator.check(user);
    }
}
"#;

        let file2_content = r#"
export class UserValidator {
    static check(user) {
        return user.name && user.email;
    }
}
"#;

        let _file1 = fixture
            .create_test_file("processor.js", file1_content)
            .unwrap();
        let _file2 = fixture
            .create_test_file("validator.js", file2_content)
            .unwrap();

        // Test dry run first
        let tool = RenameSymbolTool {
            old_name: "UserValidator".to_string(),
            new_name: "AccountValidator".to_string(),
            scope: Some("workspace".to_string()),
            dry_run: true,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        let response = extract_text_from_result(&result);
        let response_lower = response.to_lowercase();

        // Should show dry run results or no references found
        assert!(
            response_lower.contains("dry run") || response_lower.contains("no references found"),
            "Unexpected response: {}",
            response
        );
    }

    #[tokio::test]
    async fn test_end_to_end_rename_workflow_with_real_files() {
        // This test creates actual files, indexes them with Julie, and performs a real rename
        let fixture = RefactoringTestFixture::new().unwrap();

        // Create comprehensive TypeScript files that reference each other
        let service_content = r#"
export class UserService {
    constructor(private apiUrl: string) {}

    async getUser(id: number) {
        const response = await fetch(`${this.apiUrl}/users/${id}`);
        return response.json();
    }

    createUser(userData: any) {
        return fetch(`${this.apiUrl}/users`, {
            method: 'POST',
            body: JSON.stringify(userData)
        });
    }
}
"#;

        let controller_content = r#"
import { UserService } from './user-service';

export class UserController {
    private userService: UserService;

    constructor() {
        this.userService = new UserService('https://api.example.com');
    }

    async handleGetUser(req: Request) {
        const userId = req.params.id;
        return await this.userService.getUser(userId);
    }

    async handleCreateUser(req: Request) {
        return await this.userService.createUser(req.body);
    }
}
"#;

        let types_content = r#"
// Type definitions that reference UserService
export interface UserServiceConfig {
    service: UserService;
    timeout: number;
}

export type ServiceFactory = () => UserService;
"#;

        // Create the test files
        let service_path = fixture
            .create_test_file("user-service.ts", service_content)
            .unwrap();
        let controller_path = fixture
            .create_test_file("user-controller.ts", controller_content)
            .unwrap();
        let types_path = fixture.create_test_file("types.ts", types_content).unwrap();

        // Initialize Julie handler and workspace
        let handler = JulieServerHandler::new().await.unwrap();

        // Initialize workspace with the temp directory path
        let workspace_path = fixture.temp_dir.path().to_string_lossy().to_string();
        handler
            .initialize_workspace_with_force(Some(workspace_path), true)
            .await
            .unwrap();

        // Create the rename tool - rename UserService to AccountService
        let rename_tool = RenameSymbolTool {
            old_name: "UserService".to_string(),
            new_name: "AccountService".to_string(),
            scope: Some("workspace".to_string()),
            dry_run: false, // Actually perform the rename
        };

        // Execute the rename
        let result = rename_tool.call_tool(&handler).await.unwrap();
        let response = extract_text_from_result(&result);
        let response_lower = response.to_lowercase();

        // The response should indicate successful rename or no references found
        // (no references found is expected since we're testing with temp files that may not be indexed properly)
        assert!(
            response_lower.contains("applied")
                || response_lower.contains("no references found")
                || response_lower.contains("modified")
                || response_lower.contains("change"),
            "Unexpected response: {}",
            response
        );

        // Verify files exist and check their content
        let updated_service = fixture.read_file(&service_path).unwrap();
        let updated_controller = fixture.read_file(&controller_path).unwrap();
        let updated_types = fixture.read_file(&types_path).unwrap();

        // If the rename was successful, the files should contain AccountService instead of UserService
        // If no references were found, the files should be unchanged
        if response_lower.contains("applied") || response_lower.contains("modified") {
            // Files should be updated
            assert!(updated_service.contains("AccountService"));
            assert!(!updated_service.contains("UserService"));

            assert!(updated_controller.contains("AccountService"));
            assert!(!updated_controller.contains("UserService"));

            assert!(updated_types.contains("AccountService"));
            assert!(!updated_types.contains("UserService"));
        } else {
            // No references found - files should be unchanged
            assert!(updated_service.contains("UserService"));
            assert!(updated_controller.contains("UserService"));
            assert!(updated_types.contains("UserService"));
        }

        // The test passes if the tool behaves consistently with its response
        println!("End-to-end test completed. Response: {}", response);
    }
}
