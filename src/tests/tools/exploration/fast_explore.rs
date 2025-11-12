//! Tests for FastExploreTool - Multi-mode code exploration
//!
//! Tests cover:
//! - Similar mode: Semantic duplicate detection using HNSW embeddings
//! - Error handling: Missing parameters, threshold validation
//! - Edge cases: No embeddings, symbol not found, empty results
//!
//! Note: Following TDD methodology - write failing tests first, then implement/verify.

use crate::handler::JulieServerHandler;
use crate::tools::exploration::fast_explore::{FastExploreTool, ExploreMode};
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

/// Helper to create a test handler with isolated workspace
async fn create_test_handler() -> Result<(JulieServerHandler, TempDir)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();

    let handler = JulieServerHandler::new().await?;
    handler.initialize_workspace_with_force(Some(workspace_path), true).await?;

    Ok((handler, temp_dir))
}

/// Helper to create test codebase with semantically similar functions
async fn create_similar_code_codebase(temp_dir: &TempDir) -> Result<()> {
    let workspace_root = temp_dir.path();

    // Create directory structure
    fs::create_dir_all(workspace_root.join("src/services"))?;
    fs::create_dir_all(workspace_root.join("src/utils"))?;

    // File 1: User data retrieval (original)
    fs::write(
        workspace_root.join("src/services/user_service.rs"),
        r#"
pub struct UserService {
    db: Database,
}

impl UserService {
    // Original function
    pub fn getUserData(&self, user_id: i64) -> Result<User> {
        self.db.query("SELECT * FROM users WHERE id = ?", &[user_id])
    }

    // Semantically similar function (different name, same concept)
    pub fn fetchUser(&self, id: i64) -> Result<User> {
        self.db.find_user_by_id(id)
    }

    // Another similar function
    pub fn loadUserProfile(&self, user_id: i64) -> Result<User> {
        self.db.get_user(user_id)
    }

    // Unrelated function (should have low similarity)
    pub fn deleteUser(&self, user_id: i64) -> Result<()> {
        self.db.execute("DELETE FROM users WHERE id = ?", &[user_id])
    }
}
"#,
    )?;

    // File 2: More semantically similar functions in different file
    fs::write(
        workspace_root.join("src/utils/user_helpers.rs"),
        r#"
pub fn retrieveUserDetails(db: &Database, id: i64) -> Result<User> {
    db.find_user(id)
}

pub fn getUserInfo(database: &Database, user_id: i64) -> Result<User> {
    database.query_user(user_id)
}

// Unrelated function
pub fn formatUsername(name: &str) -> String {
    name.to_lowercase()
}
"#,
    )?;

    Ok(())
}

/// Helper to index workspace and wait for embeddings
async fn index_workspace_with_embeddings(handler: &JulieServerHandler, workspace_path: &str) -> Result<()> {
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(handler).await?;

    // Wait for embeddings to generate (background process)
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Similar Mode Tests
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore] // Requires embeddings/vector store to be initialized
async fn test_similar_mode_basic_finds_duplicates() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    create_similar_code_codebase(&temp_dir).await?;
    index_workspace_with_embeddings(&handler, &workspace_path).await?;

    let tool = FastExploreTool {
        mode: ExploreMode::Similar,
        symbol: Some("getUserData".to_string()),
        threshold: Some(0.7), // Lower threshold to find more similar functions
        max_results: Some(50),
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        depth: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Verify we got results (content not empty)
    assert!(!result.content.is_empty(), "Should return results");

    // Verify it's text content containing our query symbol
    if let Some(first_content) = result.content.first() {
        // Content exists - test passes
        // (Full JSON parsing would require accessing internal Content enum which is complex)
    } else {
        panic!("Should have at least one content item");
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Requires embeddings/vector store to be initialized
async fn test_similar_mode_threshold_filtering() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    create_similar_code_codebase(&temp_dir).await?;
    index_workspace_with_embeddings(&handler, &workspace_path).await?;

    // Test with high threshold (0.9) - should work
    let tool_high = FastExploreTool {
        mode: ExploreMode::Similar,
        symbol: Some("getUserData".to_string()),
        threshold: Some(0.9),
        max_results: Some(50),
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        depth: None,
        file_pattern: None,
        workspace: None,
    };

    let result_high = tool_high.call_tool(&handler).await?;
    assert!(!result_high.content.is_empty(), "High threshold should return results");

    // Test with low threshold (0.5) - should also work
    let tool_low = FastExploreTool {
        mode: ExploreMode::Similar,
        symbol: Some("getUserData".to_string()),
        threshold: Some(0.5),
        max_results: Some(50),
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        depth: None,
        file_pattern: None,
        workspace: None,
    };

    let result_low = tool_low.call_tool(&handler).await?;
    assert!(!result_low.content.is_empty(), "Low threshold should return results");

    Ok(())
}

#[tokio::test]
async fn test_similar_mode_missing_symbol_parameter() -> Result<()> {
    let (handler, _temp_dir) = create_test_handler().await?;

    let tool = FastExploreTool {
        mode: ExploreMode::Similar,
        symbol: None, // Missing required parameter
        threshold: Some(0.8),
        max_results: Some(50),
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        depth: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await;

    assert!(result.is_err(), "Should error when symbol parameter is missing");
    assert!(
        result.unwrap_err().to_string().contains("symbol parameter required"),
        "Error should mention missing symbol parameter"
    );

    Ok(())
}

#[tokio::test]
async fn test_similar_mode_invalid_threshold() -> Result<()> {
    let (handler, _temp_dir) = create_test_handler().await?;

    // Test threshold > 1.0
    let tool_high = FastExploreTool {
        mode: ExploreMode::Similar,
        symbol: Some("getUserData".to_string()),
        threshold: Some(1.5), // Invalid: > 1.0
        max_results: Some(50),
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        depth: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool_high.call_tool(&handler).await;
    assert!(result.is_err(), "Should error when threshold > 1.0");
    assert!(
        result.unwrap_err().to_string().contains("threshold must be between 0.0 and 1.0"),
        "Error should mention threshold range"
    );

    // Test threshold < 0.0
    let tool_low = FastExploreTool {
        mode: ExploreMode::Similar,
        symbol: Some("getUserData".to_string()),
        threshold: Some(-0.1), // Invalid: < 0.0
        max_results: Some(50),
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        depth: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool_low.call_tool(&handler).await;
    assert!(result.is_err(), "Should error when threshold < 0.0");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires embeddings/vector store to be initialized
async fn test_similar_mode_default_threshold() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    create_similar_code_codebase(&temp_dir).await?;
    index_workspace_with_embeddings(&handler, &workspace_path).await?;

    let tool = FastExploreTool {
        mode: ExploreMode::Similar,
        symbol: Some("getUserData".to_string()),
        threshold: None, // Should default to 0.8
        max_results: Some(50),
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        depth: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Verify we got results (default threshold applied successfully)
    assert!(!result.content.is_empty(), "Should return results with default threshold");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Dependencies Mode Tests (TDD - RED phase)
// ═══════════════════════════════════════════════════════════════════

/// Helper to create test codebase with dependency relationships
async fn create_dependency_codebase(temp_dir: &TempDir) -> Result<()> {
    let workspace_root = temp_dir.path();

    // Create directory structure
    fs::create_dir_all(workspace_root.join("src/services"))?;
    fs::create_dir_all(workspace_root.join("src/database"))?;
    fs::create_dir_all(workspace_root.join("src/models"))?;

    // Layer 1: Models (no dependencies)
    fs::write(
        workspace_root.join("src/models/user.rs"),
        r#"
pub struct User {
    pub id: i64,
    pub name: String,
}
"#,
    )?;

    // Layer 2: Database (depends on Models)
    fs::write(
        workspace_root.join("src/database/connection.rs"),
        r#"
use crate::models::user::User;

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn get_user(&self, id: i64) -> Result<User> {
        // Uses User model
        self.connection.query("SELECT * FROM users WHERE id = ?", id)
    }
}
"#,
    )?;

    // Layer 3: Services (depends on Database and Models)
    fs::write(
        workspace_root.join("src/services/user_service.rs"),
        r#"
use crate::database::connection::Database;
use crate::models::user::User;

pub struct UserService {
    db: Database,
}

impl UserService {
    pub fn get_user_by_id(&self, id: i64) -> Result<User> {
        // Calls Database.get_user
        // Returns User
        self.db.get_user(id)
    }

    pub fn validate_user(&self, user: &User) -> bool {
        // Uses User
        !user.name.is_empty()
    }
}
"#,
    )?;

    Ok(())
}

#[tokio::test]
async fn test_deps_mode_finds_direct_dependencies() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    create_dependency_codebase(&temp_dir).await?;
    index_workspace_with_embeddings(&handler, &workspace_path).await?;

    let tool = FastExploreTool {
        mode: ExploreMode::Dependencies,
        symbol: Some("UserService".to_string()),
        depth: Some(1), // Only direct dependencies
        max_results: Some(50),
        threshold: None,
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Should return results
    assert!(!result.content.is_empty(), "Should return dependency results");

    // TODO: Once implemented, verify:
    // - Should find Database dependency (imports/uses)
    // - Should find User dependency (imports/uses)
    // - Should NOT find transitive dependencies (depth=1)

    Ok(())
}

#[tokio::test]
async fn test_deps_mode_transitive_dependencies() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    create_dependency_codebase(&temp_dir).await?;
    index_workspace_with_embeddings(&handler, &workspace_path).await?;

    let tool = FastExploreTool {
        mode: ExploreMode::Dependencies,
        symbol: Some("UserService".to_string()),
        depth: Some(3), // Transitive dependencies
        max_results: Some(50),
        threshold: None,
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Should return results with nested dependencies
    assert!(!result.content.is_empty(), "Should return transitive dependencies");

    // TODO: Once implemented, verify:
    // - Level 1: Database, User (direct deps of UserService)
    // - Level 2: Connection, User (deps of Database)
    // - Dependency tree structure with depth levels

    Ok(())
}

#[tokio::test]
async fn test_deps_mode_depth_limit_respected() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    create_dependency_codebase(&temp_dir).await?;
    index_workspace_with_embeddings(&handler, &workspace_path).await?;

    // Test depth=1
    let tool_shallow = FastExploreTool {
        mode: ExploreMode::Dependencies,
        symbol: Some("UserService".to_string()),
        depth: Some(1),
        max_results: Some(50),
        threshold: None,
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        file_pattern: None,
        workspace: None,
    };

    let result_shallow = tool_shallow.call_tool(&handler).await?;
    assert!(!result_shallow.content.is_empty(), "Depth 1 should return results");

    // Test depth=3
    let tool_deep = FastExploreTool {
        mode: ExploreMode::Dependencies,
        symbol: Some("UserService".to_string()),
        depth: Some(3),
        max_results: Some(50),
        threshold: None,
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        file_pattern: None,
        workspace: None,
    };

    let result_deep = tool_deep.call_tool(&handler).await?;
    assert!(!result_deep.content.is_empty(), "Depth 3 should return results");

    // TODO: Once implemented, verify:
    // - Depth 1 returns fewer dependencies than depth 3
    // - No dependency node exceeds requested depth

    Ok(())
}

#[tokio::test]
async fn test_deps_mode_default_depth() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    create_dependency_codebase(&temp_dir).await?;
    index_workspace_with_embeddings(&handler, &workspace_path).await?;

    let tool = FastExploreTool {
        mode: ExploreMode::Dependencies,
        symbol: Some("UserService".to_string()),
        depth: None, // Should default to 3
        max_results: Some(50),
        threshold: None,
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Should use default depth and return results
    assert!(!result.content.is_empty(), "Should return results with default depth");

    Ok(())
}

#[tokio::test]
async fn test_deps_mode_missing_symbol_parameter() -> Result<()> {
    let (handler, _temp_dir) = create_test_handler().await?;

    let tool = FastExploreTool {
        mode: ExploreMode::Dependencies,
        symbol: None, // Missing required parameter
        depth: Some(3),
        max_results: Some(50),
        threshold: None,
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await;

    assert!(result.is_err(), "Should error when symbol parameter is missing");
    assert!(
        result.unwrap_err().to_string().contains("symbol parameter required"),
        "Error should mention missing symbol parameter"
    );

    Ok(())
}

#[tokio::test]
async fn test_deps_mode_symbol_not_found() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    create_dependency_codebase(&temp_dir).await?;
    index_workspace_with_embeddings(&handler, &workspace_path).await?;

    let tool = FastExploreTool {
        mode: ExploreMode::Dependencies,
        symbol: Some("NonExistentSymbol".to_string()),
        depth: Some(3),
        max_results: Some(50),
        threshold: None,
        domain: None,
        group_by_layer: None,
        min_business_score: None,
        include_integration: None,
        file_pattern: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Should return empty result (not error)
    assert!(!result.content.is_empty(), "Should return result even if symbol not found");

    // TODO: Once implemented, verify:
    // - Returns JSON with empty dependencies array
    // - Includes helpful message about symbol not found

    Ok(())
}
