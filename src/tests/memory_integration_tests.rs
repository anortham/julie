//! End-to-end integration tests for the memory system.
//!
//! Verifies full round-trip flows through the public API:
//! - checkpoint -> recall round trip (filesystem + search)
//! - plan workflow (save -> activate -> checkpoint -> recall with planId)
//! - Goldfish backward compatibility (parsing existing format files)
//! - multiple checkpoints ordering and limit
//! - search index rebuild from manually written files
//! - MCP tool-level round trip (CheckpointTool -> RecallTool)

#[cfg(test)]
mod tests {
    use std::path::Path;
    use tempfile::TempDir;

    use crate::memory::checkpoint::save_checkpoint;
    use crate::memory::plan::{activate_plan, complete_plan, get_plan, save_plan};
    use crate::memory::recall::recall;
    use crate::memory::storage::{format_checkpoint, parse_checkpoint};
    use crate::memory::{
        Checkpoint, CheckpointInput, CheckpointType, GitContext, PlanInput, RecallOptions,
    };

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Write a checkpoint file manually to .memories/{date}/{HHMMSS}_{hash}.md.
    fn write_checkpoint_file(root: &Path, checkpoint: &Checkpoint) {
        let date = &checkpoint.timestamp[..10];
        let date_dir = root.join(".memories").join(date);
        std::fs::create_dir_all(&date_dir).unwrap();

        let id = &checkpoint.id;
        let hhmmss = checkpoint.timestamp[11..19].replace(':', "");
        let hash4 = id.strip_prefix("checkpoint_").unwrap_or(id).get(..4).unwrap_or("0000");
        let filename = format!("{}_{}.md", hhmmss, hash4);
        std::fs::write(date_dir.join(&filename), format_checkpoint(checkpoint)).unwrap();
    }

    /// Write raw content to a checkpoint file at a specific path.
    fn write_raw_checkpoint(root: &Path, date: &str, filename: &str, content: &str) {
        let date_dir = root.join(".memories").join(date);
        std::fs::create_dir_all(&date_dir).unwrap();
        std::fs::write(date_dir.join(filename), content).unwrap();
    }

    /// Create a minimal checkpoint struct for testing.
    fn make_checkpoint(ts: &str, desc: &str, tags: Option<Vec<String>>) -> Checkpoint {
        Checkpoint {
            id: crate::memory::storage::generate_checkpoint_id(ts, desc),
            timestamp: ts.to_string(),
            description: desc.to_string(),
            checkpoint_type: None,
            context: None,
            decision: None,
            alternatives: None,
            impact: None,
            evidence: None,
            symbols: None,
            next: None,
            confidence: None,
            unknowns: None,
            tags,
            git: None,
            summary: None,
            plan_id: None,
        }
    }

    // ========================================================================
    // Test 1: Checkpoint -> Recall Round Trip
    // ========================================================================

    #[tokio::test]
    async fn test_checkpoint_recall_round_trip() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = CheckpointInput {
            description: "## Implemented OAuth2 flow\n\nAdded token refresh endpoint.".into(),
            tags: Some(vec!["auth".into(), "oauth".into()]),
            symbols: Some(vec!["OAuthService".into(), "TokenRefresh".into()]),
            decision: Some("Use authorization code flow".into()),
            impact: Some("Users can now authenticate via Google".into()),
            ..Default::default()
        };

        let saved = save_checkpoint(root, input).await.unwrap();

        // Recall with default options
        let result = recall(root, RecallOptions::default()).unwrap();
        assert_eq!(result.checkpoints.len(), 1, "Should find exactly one checkpoint");

        let cp = &result.checkpoints[0];
        assert_eq!(cp.id, saved.id);
        assert_eq!(cp.timestamp, saved.timestamp);
        assert!(cp.description.contains("OAuth2 flow"));
        assert_eq!(cp.tags, Some(vec!["auth".into(), "oauth".into()]));
        assert_eq!(cp.symbols, Some(vec!["OAuthService".into(), "TokenRefresh".into()]));
        assert_eq!(cp.decision, Some("Use authorization code flow".into()));
        assert_eq!(cp.impact, Some("Users can now authenticate via Google".into()));

        // Search for the checkpoint by description keywords
        let search_result = recall(root, RecallOptions {
            search: Some("OAuth2 token refresh".into()),
            limit: Some(10),
            ..Default::default()
        }).unwrap();
        assert_eq!(search_result.checkpoints.len(), 1, "Search should find the checkpoint");
        assert_eq!(search_result.checkpoints[0].id, saved.id);
    }

    #[tokio::test]
    async fn test_checkpoint_recall_with_full_flag() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let mut cp = make_checkpoint("2026-03-07T14:30:00.000Z", "Testing full flag", None);
        cp.git = Some(GitContext {
            branch: Some("feature/test".into()),
            commit: Some("abc1234".into()),
            files: Some(vec!["src/main.rs".into()]),
        });
        write_checkpoint_file(root, &cp);

        // Default recall (full=false) strips git context
        let result = recall(root, RecallOptions::default()).unwrap();
        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].git.is_none(), "Git should be stripped when full=false");

        // full=true preserves git context
        let result = recall(root, RecallOptions { full: Some(true), ..Default::default() }).unwrap();
        let git = result.checkpoints[0].git.as_ref().expect("Git should be present when full=true");
        assert_eq!(git.branch.as_deref(), Some("feature/test"));
        assert_eq!(git.commit.as_deref(), Some("abc1234"));
    }

    // ========================================================================
    // Test 2: Plan Workflow
    // ========================================================================

    #[tokio::test]
    async fn test_plan_workflow_save_activate_checkpoint_recall() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // 1. Save a plan
        let plan = save_plan(root, PlanInput {
            id: None,
            title: "Database Migration".into(),
            content: "Migrate from MySQL to PostgreSQL".into(),
            tags: Some(vec!["database".into(), "migration".into()]),
            activate: None,
        }).unwrap();
        assert_eq!(plan.id, "database-migration");
        assert_eq!(plan.status, "active");

        // 2. Activate the plan
        activate_plan(root, &plan.id).unwrap();

        // 3. Save a checkpoint — auto-attaches active plan ID
        let saved = save_checkpoint(root, CheckpointInput {
            description: "## Schema migration complete\n\nAll tables converted.".into(),
            tags: Some(vec!["database".into()]),
            ..Default::default()
        }).await.unwrap();
        assert_eq!(saved.plan_id, Some("database-migration".into()));

        // 4. Recall with plan_id filter
        let result = recall(root, RecallOptions {
            plan_id: Some("database-migration".into()),
            ..Default::default()
        }).unwrap();
        assert_eq!(result.checkpoints.len(), 1, "Should find checkpoint linked to the plan");
        assert_eq!(result.checkpoints[0].id, saved.id);
        assert_eq!(result.checkpoints[0].plan_id, Some("database-migration".into()));

        // 5. Active plan should be in recall result
        let active = result.active_plan.as_ref().expect("Active plan should be present");
        assert_eq!(active.id, "database-migration");
        assert_eq!(active.title, "Database Migration");
        assert_eq!(active.status, "active");

        // 6. Complete the plan and verify persistence
        let completed = complete_plan(root, &plan.id).unwrap();
        assert_eq!(completed.status, "completed");
        let loaded = get_plan(root, &plan.id).unwrap().unwrap();
        assert_eq!(loaded.status, "completed");
    }

    #[tokio::test]
    async fn test_plan_checkpoint_linkage_survives_file_round_trip() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(root, PlanInput {
            id: Some("feature-x".into()),
            title: "Feature X".into(),
            content: "Build feature X".into(),
            tags: None,
            activate: Some(true),
        }).unwrap();

        let saved = save_checkpoint(root, CheckpointInput {
            description: "Working on feature X".into(),
            ..Default::default()
        }).await.unwrap();

        // Read back the checkpoint FILE and parse it
        let date = &saved.timestamp[..10];
        let entry = std::fs::read_dir(root.join(".memories").join(date))
            .unwrap().filter_map(|e| e.ok()).next().unwrap();
        let parsed = parse_checkpoint(&std::fs::read_to_string(entry.path()).unwrap()).unwrap();
        assert_eq!(parsed.plan_id, Some("feature-x".into()), "planId must survive file round-trip");
    }

    // ========================================================================
    // Test 3: Goldfish Backward Compatibility
    // ========================================================================

    #[test]
    fn test_goldfish_compatibility_full_format() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let goldfish_content = r##"---
id: checkpoint_7bb3fd6e
timestamp: "2026-03-07T17:44:14.659Z"
tags:
  - vision
  - architecture
git:
  branch: main
  commit: 62017a0
  files:
    - docs/plans/example.md
summary: "# Test Checkpoint"
planId: test-plan
type: decision
decision: Test decision statement
alternatives:
  - Alternative 1
  - Alternative 2
impact: Test impact statement
symbols:
  - TestSymbol
  - AnotherSymbol
confidence: 4
---

# Test Checkpoint

This is a test checkpoint in Goldfish format.
"##;

        write_raw_checkpoint(root, "2026-03-07", "174414_7bb3.md", goldfish_content);

        let result = recall(root, RecallOptions { full: Some(true), ..Default::default() }).unwrap();
        assert_eq!(result.checkpoints.len(), 1);
        let cp = &result.checkpoints[0];

        assert_eq!(cp.id, "checkpoint_7bb3fd6e");
        assert_eq!(cp.timestamp, "2026-03-07T17:44:14.659Z");
        assert_eq!(cp.tags, Some(vec!["vision".into(), "architecture".into()]));
        assert_eq!(cp.checkpoint_type, Some(CheckpointType::Decision));
        assert_eq!(cp.decision, Some("Test decision statement".into()));
        assert_eq!(cp.alternatives, Some(vec!["Alternative 1".into(), "Alternative 2".into()]));
        assert_eq!(cp.impact, Some("Test impact statement".into()));
        assert_eq!(cp.symbols, Some(vec!["TestSymbol".into(), "AnotherSymbol".into()]));
        assert_eq!(cp.plan_id, Some("test-plan".into()));
        assert_eq!(cp.summary, Some("# Test Checkpoint".into()));
        assert_eq!(cp.confidence, Some(4));

        let git = cp.git.as_ref().expect("Git context should be parsed");
        assert_eq!(git.branch.as_deref(), Some("main"));
        assert_eq!(git.commit.as_deref(), Some("62017a0"));
        assert_eq!(git.files, Some(vec!["docs/plans/example.md".into()]));

        assert!(cp.description.contains("This is a test checkpoint in Goldfish format."));
    }

    #[test]
    fn test_goldfish_compatibility_minimal_format() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let minimal = r#"---
id: checkpoint_abcd1234
timestamp: "2026-03-06T10:00:00.000Z"
---

Simple checkpoint with no metadata.
"#;
        write_raw_checkpoint(root, "2026-03-06", "100000_abcd.md", minimal);

        let result = recall(root, RecallOptions::default()).unwrap();
        assert_eq!(result.checkpoints.len(), 1);
        let cp = &result.checkpoints[0];
        assert_eq!(cp.id, "checkpoint_abcd1234");
        assert_eq!(cp.timestamp, "2026-03-06T10:00:00.000Z");
        assert!(cp.description.contains("Simple checkpoint with no metadata."));
        assert!(cp.tags.is_none());
        assert!(cp.checkpoint_type.is_none());
        assert!(cp.decision.is_none());
        assert!(cp.alternatives.is_none());
        assert!(cp.impact.is_none());
        assert!(cp.symbols.is_none());
        assert!(cp.plan_id.is_none());
        assert!(cp.summary.is_none());
    }

    #[test]
    fn test_goldfish_compatibility_learning_type() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let content = r#"---
id: checkpoint_learn001
timestamp: "2026-03-07T12:00:00.000Z"
type: learning
tags:
  - rust
  - performance
context: Investigating slow search queries
impact: Found that BM25 scoring was O(n^2) due to nested loops
evidence:
  - flamegraph shows 80% time in scoring loop
  - reduced from 200ms to 15ms after fix
unknowns:
  - May affect ranking quality for rare terms
next: Run search quality regression suite
confidence: 3
---

## Learned: BM25 scoring performance trap

Nested loop in BM25 scoring caused quadratic blowup on large result sets.
"#;
        write_raw_checkpoint(root, "2026-03-07", "120000_lear.md", content);

        let result = recall(root, RecallOptions { full: Some(true), ..Default::default() }).unwrap();
        assert_eq!(result.checkpoints.len(), 1);
        let cp = &result.checkpoints[0];
        assert_eq!(cp.checkpoint_type, Some(CheckpointType::Learning));
        assert_eq!(cp.context, Some("Investigating slow search queries".into()));
        assert_eq!(cp.confidence, Some(3));
        assert_eq!(cp.next, Some("Run search quality regression suite".into()));
        assert_eq!(cp.evidence, Some(vec![
            "flamegraph shows 80% time in scoring loop".into(),
            "reduced from 200ms to 15ms after fix".into(),
        ]));
        assert_eq!(cp.unknowns, Some(vec!["May affect ranking quality for rare terms".into()]));
    }

    // ========================================================================
    // Test 4: Multiple Checkpoints + Ordering
    // ========================================================================

    #[test]
    fn test_multiple_checkpoints_ordering_newest_first() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp1 = make_checkpoint("2026-03-07T10:00:00.000Z", "First checkpoint (oldest)", None);
        let cp2 = make_checkpoint("2026-03-07T12:00:00.000Z", "Second checkpoint (middle)", None);
        let cp3 = make_checkpoint("2026-03-07T14:00:00.000Z", "Third checkpoint (newest)", None);
        write_checkpoint_file(root, &cp1);
        write_checkpoint_file(root, &cp2);
        write_checkpoint_file(root, &cp3);

        let result = recall(root, RecallOptions { limit: Some(10), ..Default::default() }).unwrap();
        assert_eq!(result.checkpoints.len(), 3);
        assert!(result.checkpoints[0].description.contains("newest"), "got: {}", result.checkpoints[0].description);
        assert!(result.checkpoints[1].description.contains("middle"), "got: {}", result.checkpoints[1].description);
        assert!(result.checkpoints[2].description.contains("oldest"), "got: {}", result.checkpoints[2].description);
    }

    #[test]
    fn test_multiple_checkpoints_limit_one() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        for (i, ts) in ["2026-03-07T10:00:00.000Z", "2026-03-07T12:00:00.000Z", "2026-03-07T14:00:00.000Z"]
            .iter().enumerate()
        {
            write_checkpoint_file(root, &make_checkpoint(ts, &format!("Checkpoint {}", i + 1), None));
        }

        let result = recall(root, RecallOptions { limit: Some(1), ..Default::default() }).unwrap();
        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].description.contains("Checkpoint 3"), "limit=1 should return newest");
    }

    #[test]
    fn test_limit_zero_returns_active_plan_only() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        write_checkpoint_file(root, &make_checkpoint("2026-03-07T10:00:00.000Z", "Should not appear", None));
        save_plan(root, PlanInput {
            id: Some("my-plan".into()),
            title: "My Plan".into(),
            content: "Plan content".into(),
            tags: None,
            activate: Some(true),
        }).unwrap();

        let result = recall(root, RecallOptions { limit: Some(0), ..Default::default() }).unwrap();
        assert_eq!(result.checkpoints.len(), 0, "limit=0 should return no checkpoints");
        assert!(result.active_plan.is_some(), "Active plan should still be returned");
        assert_eq!(result.active_plan.as_ref().unwrap().id, "my-plan");
    }

    #[test]
    fn test_checkpoints_across_multiple_dates() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        write_checkpoint_file(root, &make_checkpoint("2026-03-05T10:00:00.000Z", "Day one work", None));
        write_checkpoint_file(root, &make_checkpoint("2026-03-06T15:00:00.000Z", "Day two work", None));
        write_checkpoint_file(root, &make_checkpoint("2026-03-07T09:00:00.000Z", "Day three work", None));

        let result = recall(root, RecallOptions { limit: Some(10), ..Default::default() }).unwrap();
        assert_eq!(result.checkpoints.len(), 3, "Should find checkpoints across date dirs");
        assert!(result.checkpoints[0].description.contains("Day three"));
        assert!(result.checkpoints[1].description.contains("Day two"));
        assert!(result.checkpoints[2].description.contains("Day one"));
    }

    // ========================================================================
    // Test 5: Search Index Rebuild
    // ========================================================================

    #[test]
    fn test_search_index_rebuild_from_manually_written_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Write files manually (not indexed in Tantivy)
        write_checkpoint_file(root, &make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Implemented Redis caching for sessions",
            Some(vec!["redis".into(), "caching".into()]),
        ));
        write_checkpoint_file(root, &make_checkpoint(
            "2026-03-07T11:00:00.000Z",
            "Fixed PostgreSQL connection pool leak",
            Some(vec!["postgresql".into(), "bugfix".into()]),
        ));

        // Search triggers lazy rebuild_from_files()
        let r1 = recall(root, RecallOptions {
            search: Some("Redis caching".into()), limit: Some(10), ..Default::default()
        }).unwrap();
        assert_eq!(r1.checkpoints.len(), 1, "Should find Redis checkpoint after rebuild");
        assert!(r1.checkpoints[0].description.contains("Redis caching"));

        let r2 = recall(root, RecallOptions {
            search: Some("PostgreSQL connection pool".into()), limit: Some(10), ..Default::default()
        }).unwrap();
        assert_eq!(r2.checkpoints.len(), 1, "Should find PostgreSQL checkpoint after rebuild");
        assert!(r2.checkpoints[0].description.contains("PostgreSQL connection pool"));
    }

    #[test]
    fn test_search_mode_with_plan_id_filter() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let mut cp_linked = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Database schema migration for Project Alpha",
            Some(vec!["database".into()]),
        );
        cp_linked.plan_id = Some("project-alpha".into());
        write_checkpoint_file(root, &cp_linked);
        write_checkpoint_file(root, &make_checkpoint(
            "2026-03-07T11:00:00.000Z",
            "Database performance tuning for general use",
            Some(vec!["database".into()]),
        ));

        let result = recall(root, RecallOptions {
            search: Some("database".into()),
            plan_id: Some("project-alpha".into()),
            limit: Some(10),
            ..Default::default()
        }).unwrap();
        assert_eq!(result.checkpoints.len(), 1, "planId filter should narrow results");
        assert_eq!(result.checkpoints[0].plan_id, Some("project-alpha".into()));
    }

    // ========================================================================
    // Test 6: MCP Tool-Level Round Trip
    // ========================================================================

    /// Helper: extract text from MCP CallToolResult.
    fn extract_tool_text(result: &crate::mcp_compat::CallToolResult) -> String {
        result.content.iter().filter_map(|block| {
            serde_json::to_value(block).ok().and_then(|json| {
                json.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
            })
        }).collect::<Vec<_>>().join("\n")
    }

    /// Helper: create a minimal CheckpointTool with just description and optional tags.
    fn make_cp_tool(desc: &str, tags: Option<Vec<String>>) -> crate::tools::memory::checkpoint::CheckpointTool {
        crate::tools::memory::checkpoint::CheckpointTool {
            description: desc.into(),
            checkpoint_type: None,
            tags,
            symbols: None,
            decision: None,
            alternatives: None,
            impact: None,
            context: None,
            evidence: None,
            unknowns: None,
            next: None,
            confidence: None,
        }
    }

    /// Helper: create a RecallTool with all defaults.
    fn make_recall_tool() -> crate::tools::memory::recall::RecallTool {
        crate::tools::memory::recall::RecallTool {
            limit: None, since: None, days: None, from: None, to: None,
            search: None, full: None, workspace: None, plan_id: None,
        }
    }

    #[tokio::test]
    async fn test_mcp_tool_checkpoint_then_recall() {
        use crate::handler::JulieServerHandler;
        use crate::tools::memory::checkpoint::CheckpointTool;

        let dir = TempDir::new().unwrap();
        let handler = JulieServerHandler::new_sync(dir.path().to_path_buf()).unwrap();

        let cp_tool = CheckpointTool {
            description: "## Refactored search pipeline\n\nSplit monolith into stages.".into(),
            checkpoint_type: Some("decision".into()),
            tags: Some(vec!["refactoring".into(), "search".into()]),
            symbols: Some(vec!["SearchPipeline".into()]),
            decision: Some("Use staged pipeline pattern".into()),
            alternatives: Some(vec!["Keep monolith".into()]),
            impact: Some("Easier testing and maintenance".into()),
            context: None, evidence: None, unknowns: None, next: None,
            confidence: Some(4),
        };
        let cp_text = extract_tool_text(&cp_tool.call_tool(&handler).await.unwrap());
        assert!(cp_text.contains("Checkpoint saved"), "output: {}", cp_text);
        assert!(cp_text.contains("checkpoint_"), "output: {}", cp_text);
        assert!(cp_text.contains("Refactored search pipeline"), "output: {}", cp_text);

        let recall_text = extract_tool_text(&make_recall_tool().call_tool(&handler).await.unwrap());
        assert!(recall_text.contains("Checkpoints (1 found)"), "output: {}", recall_text);
        assert!(recall_text.contains("Refactored search pipeline"), "output: {}", recall_text);
        assert!(recall_text.contains("**Tags:** refactoring, search"), "output: {}", recall_text);
    }

    #[tokio::test]
    async fn test_mcp_tool_plan_then_checkpoint_then_recall() {
        use crate::handler::JulieServerHandler;
        use crate::tools::memory::plan::PlanTool;

        let dir = TempDir::new().unwrap();
        let handler = JulieServerHandler::new_sync(dir.path().to_path_buf()).unwrap();

        // 1. Save and activate a plan
        let plan_tool = PlanTool {
            action: "save".into(), id: None,
            title: Some("API Redesign".into()),
            content: Some("Redesign the REST API for v2".into()),
            tags: Some(vec!["api".into()]),
            activate: Some(true), status: None,
        };
        let plan_text = extract_tool_text(&plan_tool.call_tool(&handler).await.unwrap());
        assert!(plan_text.contains("Plan saved"), "output: {}", plan_text);
        assert!(plan_text.contains("(activated)"), "output: {}", plan_text);

        // 2. Save checkpoint (auto-links to active plan)
        make_cp_tool("Designed new endpoint structure", None).call_tool(&handler).await.unwrap();

        // 3. Recall with planId filter
        let mut recall_tool = make_recall_tool();
        recall_tool.plan_id = Some("api-redesign".into());
        let recall_text = extract_tool_text(&recall_tool.call_tool(&handler).await.unwrap());
        assert!(recall_text.contains("Checkpoints (1 found)"), "output: {}", recall_text);
        assert!(recall_text.contains("**Plan:** api-redesign"), "output: {}", recall_text);
        assert!(recall_text.contains("Active Plan: API Redesign"), "output: {}", recall_text);

        // 4. Complete the plan
        let complete_tool = PlanTool {
            action: "complete".into(), id: Some("api-redesign".into()),
            title: None, content: None, tags: None, activate: None, status: None,
        };
        let complete_text = extract_tool_text(&complete_tool.call_tool(&handler).await.unwrap());
        assert!(complete_text.contains("Plan completed"), "output: {}", complete_text);
        assert!(complete_text.contains("**Status:** completed"), "output: {}", complete_text);
    }

    #[tokio::test]
    async fn test_mcp_tool_search_recall() {
        use crate::handler::JulieServerHandler;

        let dir = TempDir::new().unwrap();
        let handler = JulieServerHandler::new_sync(dir.path().to_path_buf()).unwrap();

        // Save two checkpoints with different topics
        make_cp_tool("Implemented WebSocket real-time notifications", Some(vec!["websocket".into()]))
            .call_tool(&handler).await.unwrap();
        make_cp_tool("Fixed GraphQL schema validation error", Some(vec!["graphql".into()]))
            .call_tool(&handler).await.unwrap();

        // Search for "WebSocket" — should find only the first
        let mut recall_tool = make_recall_tool();
        recall_tool.search = Some("WebSocket notifications".into());
        recall_tool.limit = Some(10);
        let text = extract_tool_text(&recall_tool.call_tool(&handler).await.unwrap());
        assert!(text.contains("Checkpoints (1 found)"), "output: {}", text);
        assert!(text.contains("WebSocket"), "output: {}", text);
    }
}
