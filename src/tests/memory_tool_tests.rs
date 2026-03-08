//! Tests for MCP tool wrappers (`src/tools/memory/`).
//!
//! Covers: `CheckpointTool`, `RecallTool`, and `PlanTool` — the thin MCP
//! wrappers that delegate to `src/memory/` business logic. Tests verify
//! JSON deserialization, call_tool output formatting, and error cases.

#[cfg(test)]
mod tests {
    use std::path::Path;
    use tempfile::TempDir;

    use crate::handler::JulieServerHandler;
    use crate::tools::memory::checkpoint::CheckpointTool;
    use crate::tools::memory::plan::PlanTool;
    use crate::tools::memory::recall::RecallTool;

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Create a JulieServerHandler pointing at a temp directory.
    fn create_test_handler(root: &Path) -> JulieServerHandler {
        JulieServerHandler::new_sync(root.to_path_buf()).expect("handler creation should succeed")
    }

    /// Create a temp directory that looks like a git repo (for checkpoint git context).
    async fn create_temp_git_repo() -> TempDir {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path();

        run_git(path, &["init"]).await;
        run_git(path, &["config", "user.email", "test@test.com"]).await;
        run_git(path, &["config", "user.name", "Test"]).await;

        let file = path.join("README.md");
        std::fs::write(&file, "# Test\n").unwrap();
        run_git(path, &["add", "README.md"]).await;
        run_git(path, &["commit", "-m", "initial commit"]).await;

        dir
    }

    async fn run_git(dir: &Path, args: &[&str]) {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .expect("failed to run git");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Extract text from a CallToolResult.
    fn extract_text(result: &crate::mcp_compat::CallToolResult) -> String {
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

    // ========================================================================
    // CheckpointTool — JSON deserialization
    // ========================================================================

    #[test]
    fn test_checkpoint_tool_deserializes_minimal() {
        let json = r#"{"description": "Fixed the auth bug"}"#;
        let tool: CheckpointTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.description, "Fixed the auth bug");
        assert!(tool.checkpoint_type.is_none());
        assert!(tool.tags.is_none());
        assert!(tool.symbols.is_none());
    }

    #[test]
    fn test_checkpoint_tool_deserializes_full() {
        let json = r#"{
            "description": "Decided on approach X",
            "type": "decision",
            "tags": ["architecture", "auth"],
            "symbols": ["AuthService", "login"],
            "decision": "Use JWT tokens",
            "alternatives": ["Session cookies", "OAuth only"],
            "impact": "Simpler auth flow",
            "context": "Auth was too complex",
            "evidence": ["auth_test.rs passes"],
            "unknowns": ["Token rotation strategy"],
            "next": "Implement refresh tokens",
            "confidence": 4
        }"#;
        let tool: CheckpointTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.description, "Decided on approach X");
        assert_eq!(tool.checkpoint_type.as_deref(), Some("decision"));
        assert_eq!(tool.tags.as_ref().unwrap().len(), 2);
        assert_eq!(tool.symbols.as_ref().unwrap().len(), 2);
        assert_eq!(tool.decision.as_deref(), Some("Use JWT tokens"));
        assert_eq!(tool.alternatives.as_ref().unwrap().len(), 2);
        assert_eq!(tool.confidence, Some(4));
    }

    // ========================================================================
    // CheckpointTool — call_tool
    // ========================================================================

    #[tokio::test]
    async fn test_checkpoint_tool_saves_and_returns_confirmation() {
        let repo = create_temp_git_repo().await;
        let handler = create_test_handler(repo.path());

        let tool = CheckpointTool {
            description: "## Fixed auth bug\n\nResolved the login timeout issue.".to_string(),
            checkpoint_type: None,
            tags: Some(vec!["bugfix".to_string()]),
            symbols: None,
            decision: None,
            alternatives: None,
            impact: None,
            context: None,
            evidence: None,
            unknowns: None,
            next: None,
            confidence: None,
        };

        let result = tool.call_tool(&handler).await.expect("call_tool should succeed");
        let text = extract_text(&result);

        // Verify confirmation output format
        assert!(text.contains("Checkpoint saved"), "output: {}", text);
        assert!(text.contains("**ID:** checkpoint_"), "output: {}", text);
        assert!(text.contains("**File:** .memories/"), "output: {}", text);
        assert!(text.contains("**Summary:** Fixed auth bug"), "output: {}", text);
        assert!(text.contains("**Branch:**"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_checkpoint_tool_with_decision_type() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = CheckpointTool {
            description: "Chose approach A over B".to_string(),
            checkpoint_type: Some("decision".to_string()),
            tags: None,
            symbols: None,
            decision: Some("Use approach A".to_string()),
            alternatives: Some(vec!["Approach B".to_string()]),
            impact: Some("Better performance".to_string()),
            context: None,
            evidence: None,
            unknowns: None,
            next: None,
            confidence: Some(3),
        };

        let result = tool.call_tool(&handler).await.expect("call_tool should succeed");
        let text = extract_text(&result);
        assert!(text.contains("Checkpoint saved"), "output: {}", text);

        // Verify the checkpoint file was actually written
        let memories_dir = dir.path().join(".memories");
        assert!(memories_dir.exists(), ".memories directory should be created");
    }

    // ========================================================================
    // RecallTool — JSON deserialization
    // ========================================================================

    #[test]
    fn test_recall_tool_deserializes_minimal() {
        let json = r#"{}"#;
        let tool: RecallTool = serde_json::from_str(json).unwrap();
        assert!(tool.limit.is_none());
        assert!(tool.since.is_none());
        assert!(tool.search.is_none());
    }

    #[test]
    fn test_recall_tool_deserializes_full() {
        let json = r#"{
            "limit": 10,
            "since": "2h",
            "days": 3,
            "from": "2026-03-01",
            "to": "2026-03-07",
            "search": "auth bug",
            "full": true,
            "workspace": "current",
            "planId": "my-plan"
        }"#;
        let tool: RecallTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.limit, Some(10));
        assert_eq!(tool.since.as_deref(), Some("2h"));
        assert_eq!(tool.days, Some(3));
        assert_eq!(tool.search.as_deref(), Some("auth bug"));
        assert_eq!(tool.full, Some(true));
        assert_eq!(tool.plan_id.as_deref(), Some("my-plan"));
    }

    // ========================================================================
    // RecallTool — call_tool
    // ========================================================================

    #[tokio::test]
    async fn test_recall_tool_returns_empty_when_no_memories() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = RecallTool {
            limit: None,
            since: None,
            days: None,
            from: None,
            to: None,
            search: None,
            full: None,
            workspace: None,
            plan_id: None,
        };

        let result = tool.call_tool(&handler).await.expect("call_tool should succeed");
        let text = extract_text(&result);

        assert!(text.contains("Checkpoints (0 found)"), "output: {}", text);
        assert!(text.contains("No checkpoints found"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_recall_tool_returns_checkpoints_after_save() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        // Save a checkpoint first
        let cp_tool = CheckpointTool {
            description: "## Implemented feature X\n\nAdded the new feature.".to_string(),
            checkpoint_type: None,
            tags: Some(vec!["feature".to_string()]),
            symbols: None,
            decision: None,
            alternatives: None,
            impact: None,
            context: None,
            evidence: None,
            unknowns: None,
            next: None,
            confidence: None,
        };
        cp_tool.call_tool(&handler).await.expect("checkpoint save should succeed");

        // Recall
        let recall_tool = RecallTool {
            limit: None,
            since: None,
            days: None,
            from: None,
            to: None,
            search: None,
            full: None,
            workspace: None,
            plan_id: None,
        };

        let result = recall_tool.call_tool(&handler).await.expect("recall should succeed");
        let text = extract_text(&result);

        assert!(text.contains("Checkpoints (1 found)"), "output: {}", text);
        assert!(text.contains("checkpoint_"), "output: {}", text);
        assert!(text.contains("**Summary:** Implemented feature X"), "output: {}", text);
        assert!(text.contains("**Tags:** feature"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_recall_tool_limit_zero_returns_plan_only() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        // Save a checkpoint (should NOT appear with limit=0)
        let cp_tool = CheckpointTool {
            description: "Some milestone".to_string(),
            checkpoint_type: None,
            tags: None,
            symbols: None,
            decision: None,
            alternatives: None,
            impact: None,
            context: None,
            evidence: None,
            unknowns: None,
            next: None,
            confidence: None,
        };
        cp_tool.call_tool(&handler).await.unwrap();

        let recall_tool = RecallTool {
            limit: Some(0),
            since: None,
            days: None,
            from: None,
            to: None,
            search: None,
            full: None,
            workspace: None,
            plan_id: None,
        };

        let result = recall_tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);

        assert!(text.contains("Checkpoints (0 found)"), "limit=0 should return no checkpoints, output: {}", text);
    }

    // ========================================================================
    // RecallTool — format_recall_result
    // ========================================================================

    #[test]
    fn test_format_recall_result_with_active_plan() {
        use crate::memory::{Plan, RecallResult};
        use crate::tools::memory::recall::format_recall_result;

        let result = RecallResult {
            checkpoints: vec![],
            active_plan: Some(Plan {
                id: "my-plan".to_string(),
                title: "My Plan".to_string(),
                content: "Plan content here".to_string(),
                status: "active".to_string(),
                created: "2026-03-07T10:00:00.000Z".to_string(),
                updated: "2026-03-07T10:00:00.000Z".to_string(),
                tags: vec!["tag1".to_string()],
            }),
            workspaces: None,
        };

        let output = format_recall_result(&result);
        assert!(output.contains("## Active Plan: My Plan"), "output: {}", output);
        assert!(output.contains("**ID:** my-plan"), "output: {}", output);
        assert!(output.contains("**Tags:** tag1"), "output: {}", output);
        assert!(output.contains("Plan content here"), "output: {}", output);
    }

    // ========================================================================
    // PlanTool — JSON deserialization
    // ========================================================================

    #[test]
    fn test_plan_tool_deserializes_save() {
        let json = r#"{
            "action": "save",
            "title": "My Feature Plan",
            "content": "Build feature X",
            "tags": ["feature"],
            "activate": true
        }"#;
        let tool: PlanTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.action, "save");
        assert_eq!(tool.title.as_deref(), Some("My Feature Plan"));
        assert_eq!(tool.content.as_deref(), Some("Build feature X"));
        assert_eq!(tool.activate, Some(true));
    }

    #[test]
    fn test_plan_tool_deserializes_get() {
        let json = r#"{"action": "get", "id": "my-plan"}"#;
        let tool: PlanTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.action, "get");
        assert_eq!(tool.id.as_deref(), Some("my-plan"));
    }

    // ========================================================================
    // PlanTool — call_tool save/get/list/activate/update/complete
    // ========================================================================

    #[tokio::test]
    async fn test_plan_tool_save() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = PlanTool {
            action: "save".to_string(),
            id: None,
            title: Some("Auth Refactor".to_string()),
            content: Some("Refactor the auth module".to_string()),
            tags: Some(vec!["auth".to_string()]),
            activate: Some(true),
            status: None,
        };

        let result = tool.call_tool(&handler).await.expect("plan save should succeed");
        let text = extract_text(&result);

        assert!(text.contains("Plan saved"), "output: {}", text);
        assert!(text.contains("(activated)"), "output: {}", text);
        assert!(text.contains("**ID:** auth-refactor"), "output: {}", text);
        assert!(text.contains("**Title:** Auth Refactor"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_plan_tool_get() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        // Save a plan first
        let save_tool = PlanTool {
            action: "save".to_string(),
            id: None,
            title: Some("Test Plan".to_string()),
            content: Some("Test content".to_string()),
            tags: None,
            activate: None,
            status: None,
        };
        save_tool.call_tool(&handler).await.unwrap();

        // Get it
        let get_tool = PlanTool {
            action: "get".to_string(),
            id: Some("test-plan".to_string()),
            title: None,
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = get_tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);

        assert!(text.contains("## Test Plan"), "output: {}", text);
        assert!(text.contains("Test content"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_plan_tool_get_not_found() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = PlanTool {
            action: "get".to_string(),
            id: Some("nonexistent".to_string()),
            title: None,
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("not found"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_plan_tool_list() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        // Save two plans
        for title in &["Plan A", "Plan B"] {
            let tool = PlanTool {
                action: "save".to_string(),
                id: None,
                title: Some(title.to_string()),
                content: Some(format!("{} content", title)),
                tags: None,
                activate: None,
                status: None,
            };
            tool.call_tool(&handler).await.unwrap();
        }

        let list_tool = PlanTool {
            action: "list".to_string(),
            id: None,
            title: None,
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = list_tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);

        assert!(text.contains("Plans (2 found)"), "output: {}", text);
        assert!(text.contains("Plan A"), "output: {}", text);
        assert!(text.contains("Plan B"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_plan_tool_list_empty() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = PlanTool {
            action: "list".to_string(),
            id: None,
            title: None,
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("No plans found"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_plan_tool_activate() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        // Save a plan
        let save_tool = PlanTool {
            action: "save".to_string(),
            id: None,
            title: Some("Activate Me".to_string()),
            content: Some("Content".to_string()),
            tags: None,
            activate: None,
            status: None,
        };
        save_tool.call_tool(&handler).await.unwrap();

        // Activate it
        let activate_tool = PlanTool {
            action: "activate".to_string(),
            id: Some("activate-me".to_string()),
            title: None,
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = activate_tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("'activate-me' activated"), "output: {}", text);

        // Verify recall shows the active plan
        let recall_tool = RecallTool {
            limit: Some(0),
            since: None,
            days: None,
            from: None,
            to: None,
            search: None,
            full: None,
            workspace: None,
            plan_id: None,
        };

        let result = recall_tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("Active Plan: Activate Me"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_plan_tool_update() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        // Save a plan
        let save_tool = PlanTool {
            action: "save".to_string(),
            id: None,
            title: Some("Original Title".to_string()),
            content: Some("Original content".to_string()),
            tags: None,
            activate: None,
            status: None,
        };
        save_tool.call_tool(&handler).await.unwrap();

        // Update it
        let update_tool = PlanTool {
            action: "update".to_string(),
            id: Some("original-title".to_string()),
            title: Some("Updated Title".to_string()),
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = update_tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("Plan updated"), "output: {}", text);
        assert!(text.contains("**Title:** Updated Title"), "output: {}", text);
    }

    #[tokio::test]
    async fn test_plan_tool_complete() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        // Save a plan
        let save_tool = PlanTool {
            action: "save".to_string(),
            id: None,
            title: Some("Complete Me".to_string()),
            content: Some("Content".to_string()),
            tags: None,
            activate: None,
            status: None,
        };
        save_tool.call_tool(&handler).await.unwrap();

        // Complete it
        let complete_tool = PlanTool {
            action: "complete".to_string(),
            id: Some("complete-me".to_string()),
            title: None,
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = complete_tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("Plan completed"), "output: {}", text);
        assert!(text.contains("**Status:** completed"), "output: {}", text);
    }

    // ========================================================================
    // PlanTool — error cases
    // ========================================================================

    #[tokio::test]
    async fn test_plan_tool_unknown_action() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = PlanTool {
            action: "destroy".to_string(),
            id: None,
            title: None,
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown plan action"), "error: {}", err);
    }

    #[tokio::test]
    async fn test_plan_tool_save_missing_title() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = PlanTool {
            action: "save".to_string(),
            id: None,
            title: None,
            content: Some("Content without title".to_string()),
            tags: None,
            activate: None,
            status: None,
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("title"));
    }

    #[tokio::test]
    async fn test_plan_tool_save_missing_content() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = PlanTool {
            action: "save".to_string(),
            id: None,
            title: Some("Has Title".to_string()),
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("content"));
    }

    #[tokio::test]
    async fn test_plan_tool_activate_missing_id() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        let tool = PlanTool {
            action: "activate".to_string(),
            id: None,
            title: None,
            content: None,
            tags: None,
            activate: None,
            status: None,
        };

        let result = tool.call_tool(&handler).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("id"));
    }

    // ========================================================================
    // End-to-end: checkpoint linked to plan
    // ========================================================================

    #[tokio::test]
    async fn test_checkpoint_linked_to_active_plan() {
        let dir = TempDir::new().unwrap();
        let handler = create_test_handler(dir.path());

        // Save and activate a plan
        let plan_tool = PlanTool {
            action: "save".to_string(),
            id: None,
            title: Some("Feature Work".to_string()),
            content: Some("Building the feature".to_string()),
            tags: None,
            activate: Some(true),
            status: None,
        };
        plan_tool.call_tool(&handler).await.unwrap();

        // Save a checkpoint (should automatically get the active plan ID)
        let cp_tool = CheckpointTool {
            description: "Progress on feature".to_string(),
            checkpoint_type: None,
            tags: None,
            symbols: None,
            decision: None,
            alternatives: None,
            impact: None,
            context: None,
            evidence: None,
            unknowns: None,
            next: None,
            confidence: None,
        };
        cp_tool.call_tool(&handler).await.unwrap();

        // Recall with planId filter
        let recall_tool = RecallTool {
            limit: None,
            since: None,
            days: None,
            from: None,
            to: None,
            search: None,
            full: None,
            workspace: None,
            plan_id: Some("feature-work".to_string()),
        };

        let result = recall_tool.call_tool(&handler).await.unwrap();
        let text = extract_text(&result);

        assert!(text.contains("Checkpoints (1 found)"), "checkpoint should be linked to plan, output: {}", text);
        assert!(text.contains("**Plan:** feature-work"), "output: {}", text);
    }
}
