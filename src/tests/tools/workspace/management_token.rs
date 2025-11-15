//! Token optimization integration tests for ManageWorkspaceTool
//! Following TDD methodology: RED -> GREEN -> REFACTOR
//!
//! Tests verify that list and recent commands use ProgressiveReducer
//! to gracefully handle large numbers of workspaces and files.

#[cfg(test)]
mod workspace_management_token_tests {
    #![allow(unused_variables)]

    use crate::utils::progressive_reduction::ProgressiveReducer;
    use crate::utils::token_estimation::TokenEstimator;
    use crate::workspace::registry::{
        EmbeddingStatus, WorkspaceEntry, WorkspaceStatus, WorkspaceType,
    };

    /// Test that workspace list formatting respects token limits
    #[test]
    fn test_workspace_list_with_many_workspaces_applies_reduction() {
        let reducer = ProgressiveReducer::new();
        let token_estimator = TokenEstimator::new();

        // Create 100 workspaces to trigger token optimization
        let mut workspaces = Vec::new();
        for i in 1..=100 {
            let workspace = WorkspaceEntry {
                id: format!("workspace_{}", i),
                original_path: format!(
                    "/very/long/path/to/workspace/with/many/nested/directories/project_{}",
                    i
                ),
                directory_name: format!("workspace_{}", i),
                display_name: format!("Project {} - Comprehensive Development Environment", i),
                workspace_type: WorkspaceType::Reference,
                file_count: 1000 + i,
                symbol_count: 50000 + i * 100,
                index_size_bytes: 5000000 + (i as u64) * 10000,
                created_at: 1700000000 + i as u64,
                last_accessed: 1700000000 + i as u64,
                expires_at: Some(1800000000 + i as u64),
                status: WorkspaceStatus::Active,
                embedding_status: EmbeddingStatus::Ready,
            };
            workspaces.push(workspace);
        }

        // Token estimation function that formats workspace entries
        let estimate_workspaces = |ws_subset: &[WorkspaceEntry]| {
            let mut test_output = String::from("📋 Registered Workspaces:\n\n");
            for workspace in ws_subset {
                test_output.push_str(&format!(
                    "🏷️ **{}** ({})\n\
                    📁 Path: {}\n\
                    🔍 Type: {:?}\n\
                    📊 Files: {} | Symbols: {} | Size: {:.1} KB\n\
                    ⏰ Expires: in 30 days\n\
                    📅 Status: ✅ ACTIVE\n\n",
                    workspace.display_name,
                    workspace.id,
                    workspace.original_path,
                    workspace.workspace_type,
                    workspace.file_count,
                    workspace.symbol_count,
                    workspace.index_size_bytes as f64 / 1024.0,
                ));
            }
            token_estimator.estimate_string(&test_output)
        };

        // Apply progressive reduction with 5000 token target (more aggressive to force reduction)
        let optimized = reducer.reduce(&workspaces, 5000, estimate_workspaces);

        // Should reduce from 100 to fit within token limits
        assert!(
            optimized.len() < 100,
            "Should reduce workspace count from 100"
        );
        assert!(
            optimized.len() >= 5,
            "Should keep at least 5% of workspaces"
        );

        // Verify token estimation is within limits
        let final_tokens = estimate_workspaces(&optimized);
        assert!(
            final_tokens <= 5000,
            "Final output should be within 5000 token limit"
        );

        // Should preserve first workspace (most recently accessed)
        assert_eq!(optimized[0].id, "workspace_1");
    }

    /// Test that workspace list with few workspaces doesn't apply reduction
    #[test]
    fn test_workspace_list_with_few_workspaces_unchanged() {
        let reducer = ProgressiveReducer::new();
        let token_estimator = TokenEstimator::new();

        // Create only 3 workspaces
        let mut workspaces = Vec::new();
        for i in 1..=3 {
            let workspace = WorkspaceEntry {
                id: format!("workspace_{}", i),
                original_path: format!("/path/to/project_{}", i),
                directory_name: format!("workspace_{}", i),
                display_name: format!("Project {}", i),
                workspace_type: WorkspaceType::Reference,
                file_count: 100,
                symbol_count: 5000,
                index_size_bytes: 500000,
                created_at: 1700000000,
                last_accessed: 1700000000,
                expires_at: None,
                status: WorkspaceStatus::Active,
                embedding_status: EmbeddingStatus::Ready,
            };
            workspaces.push(workspace);
        }

        let estimate_workspaces = |ws_subset: &[WorkspaceEntry]| {
            let mut test_output = String::from("📋 Registered Workspaces:\n\n");
            for workspace in ws_subset {
                test_output.push_str(&format!(
                    "🏷️ **{}** ({})\n📁 Path: {}\n\n",
                    workspace.display_name, workspace.id, workspace.original_path
                ));
            }
            token_estimator.estimate_string(&test_output)
        };

        // Apply progressive reduction with 10000 token target
        let optimized = reducer.reduce(&workspaces, 10000, estimate_workspaces);

        // Should NOT reduce - all 3 workspaces should be included
        assert_eq!(
            optimized.len(),
            3,
            "Should include all 3 workspaces without reduction"
        );
        assert_eq!(optimized[0].id, "workspace_1");
        assert_eq!(optimized[1].id, "workspace_2");
        assert_eq!(optimized[2].id, "workspace_3");
    }

    /// Test that recent files list respects token limits
    #[test]
    fn test_recent_files_with_many_files_applies_reduction() {
        use crate::database::FileInfo;

        let reducer = ProgressiveReducer::new();
        let token_estimator = TokenEstimator::new();

        // Create 500 recently modified files
        let mut files = Vec::new();
        for i in 1..=500 {
            let file = FileInfo {
                path: format!(
                    "/very/long/path/to/project/src/modules/submodule_{}/components/detailed_implementation_file_with_long_name_{}.rs",
                    i % 20,
                    i
                ),
                language: "rust".to_string(),
                hash: format!("hash_{}", i),
                size: 10000 + i as i64,
                last_modified: 1700000000 + i as i64,
                last_indexed: 1700000000 + i as i64,
                symbol_count: 50 + (i % 20) as i32,
                content: None,
            };
            files.push(file);
        }

        let estimate_files = |file_subset: &[FileInfo]| {
            let mut test_output = String::from("📅 Files modified in the last 7 days:\n\n");
            for file in file_subset {
                test_output.push_str(&format!(
                    "📄 **{}**\n\
                    🕐 Modified: 2 hours ago\n\
                    📊 {} symbols | {} bytes\n\
                    🔤 Language: {}\n\n",
                    file.path, file.symbol_count, file.size, file.language
                ));
            }
            token_estimator.estimate_string(&test_output)
        };

        // Apply progressive reduction with 12000 token target
        let optimized = reducer.reduce(&files, 12000, estimate_files);

        // Should reduce from 500 to fit within token limits
        assert!(optimized.len() < 500, "Should reduce file count from 500");
        assert!(optimized.len() >= 25, "Should keep at least 5% of files");

        // Verify token estimation is within limits
        let final_tokens = estimate_files(&optimized);
        assert!(
            final_tokens <= 12000,
            "Final output should be within 12000 token limit"
        );

        // Should preserve first file (most recently modified)
        assert!(optimized[0].path.contains("file_with_long_name_1"));
    }

    /// Test that recent files with few files doesn't apply reduction
    #[test]
    fn test_recent_files_with_few_files_unchanged() {
        use crate::database::FileInfo;

        let reducer = ProgressiveReducer::new();
        let token_estimator = TokenEstimator::new();

        // Create only 5 recently modified files
        let mut files = Vec::new();
        for i in 1..=5 {
            let file = FileInfo {
                path: format!("/path/to/file_{}.rs", i),
                language: "rust".to_string(),
                hash: format!("hash_{}", i),
                size: 5000,
                last_modified: 1700000000,
                last_indexed: 1700000000,
                symbol_count: 25,
                content: None,
            };
            files.push(file);
        }

        let estimate_files = |file_subset: &[FileInfo]| {
            let mut test_output = String::from("📅 Files modified in the last 7 days:\n\n");
            for file in file_subset {
                test_output.push_str(&format!(
                    "📄 **{}**\n🕐 Modified: 1 hour ago\n\n",
                    file.path
                ));
            }
            token_estimator.estimate_string(&test_output)
        };

        // Apply progressive reduction with 12000 token target
        let optimized = reducer.reduce(&files, 12000, estimate_files);

        // Should NOT reduce - all 5 files should be included
        assert_eq!(
            optimized.len(),
            5,
            "Should include all 5 files without reduction"
        );
        assert!(optimized[0].path.contains("file_1.rs"));
        assert!(optimized[4].path.contains("file_5.rs"));
    }

    /// Test progressive reduction steps work correctly
    #[test]
    fn test_progressive_reduction_steps_applied_correctly() {
        let reducer = ProgressiveReducer::new();
        let token_estimator = TokenEstimator::new();

        // Create workspaces where each one is ~200 tokens
        // At 100 workspaces, that's ~20000 tokens total
        let mut workspaces = Vec::new();
        for i in 1..=100 {
            let workspace = WorkspaceEntry {
                id: format!("ws_{}", i),
                original_path: format!("/path/to/workspace_{}", i),
                directory_name: format!("ws_{}", i),
                display_name: format!("Workspace {}", i),
                workspace_type: WorkspaceType::Reference,
                file_count: 1000,
                symbol_count: 50000,
                index_size_bytes: 5000000,
                created_at: 1700000000,
                last_accessed: 1700000000,
                expires_at: Some(1800000000),
                status: WorkspaceStatus::Active,
                embedding_status: EmbeddingStatus::Ready,
            };
            workspaces.push(workspace);
        }

        let estimate_workspaces = |ws_subset: &[WorkspaceEntry]| {
            // Each workspace entry is approximately 200 tokens
            ws_subset.len() * 200
        };

        // Test with 10000 token target
        // 10000 tokens / 200 tokens per workspace = 50 workspaces max
        let optimized = reducer.reduce(&workspaces, 10000, estimate_workspaces);

        // Should reduce to ~50 workspaces (50% reduction step)
        assert!(optimized.len() <= 50, "Should apply 50% reduction");
        assert!(
            optimized.len() >= 45,
            "Should be close to 50% (allowing for rounding)"
        );

        let final_tokens = estimate_workspaces(&optimized);
        assert!(final_tokens <= 10000, "Should be within token limit");
    }
}
