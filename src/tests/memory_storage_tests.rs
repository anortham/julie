//! Tests for the memory storage layer (YAML frontmatter serialization).
//!
//! Covers: Checkpoint/Plan/GitContext struct definitions, format_checkpoint(),
//! parse_checkpoint(), generate_checkpoint_id(), get_checkpoint_filename(),
//! and backward compatibility with existing Goldfish .memories/ files.

#[cfg(test)]
mod tests {
    use crate::memory::{
        Checkpoint, CheckpointType, GitContext, Plan, RecallOptions, RecallResult,
        WorkspaceSummary,
    };
    use crate::memory::storage::{
        format_checkpoint, generate_checkpoint_id, get_checkpoint_filename, parse_checkpoint,
    };

    // ========================================================================
    // Type definition tests
    // ========================================================================

    #[test]
    fn test_checkpoint_type_enum_values() {
        // Verify all four checkpoint types match Goldfish's TypeScript union
        assert_eq!(
            serde_json::to_string(&CheckpointType::Checkpoint).unwrap(),
            "\"checkpoint\""
        );
        assert_eq!(
            serde_json::to_string(&CheckpointType::Decision).unwrap(),
            "\"decision\""
        );
        assert_eq!(
            serde_json::to_string(&CheckpointType::Incident).unwrap(),
            "\"incident\""
        );
        assert_eq!(
            serde_json::to_string(&CheckpointType::Learning).unwrap(),
            "\"learning\""
        );
    }

    #[test]
    fn test_checkpoint_type_deserialize_from_string() {
        let decision: CheckpointType = serde_json::from_str("\"decision\"").unwrap();
        assert_eq!(decision, CheckpointType::Decision);
    }

    #[test]
    fn test_git_context_all_fields() {
        let git = GitContext {
            branch: Some("main".to_string()),
            commit: Some("abc1234".to_string()),
            files: Some(vec!["src/main.rs".to_string(), "Cargo.toml".to_string()]),
        };
        assert_eq!(git.branch.as_deref(), Some("main"));
        assert_eq!(git.commit.as_deref(), Some("abc1234"));
        assert_eq!(git.files.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_git_context_empty() {
        let git = GitContext {
            branch: None,
            commit: None,
            files: None,
        };
        assert!(git.branch.is_none());
        assert!(git.commit.is_none());
        assert!(git.files.is_none());
    }

    #[test]
    fn test_checkpoint_minimal() {
        let cp = Checkpoint {
            id: "checkpoint_abcd1234".to_string(),
            timestamp: "2026-03-07T17:44:14.659Z".to_string(),
            description: "Test checkpoint".to_string(),
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
            tags: None,
            git: None,
            summary: None,
            plan_id: None,
        };
        assert_eq!(cp.id, "checkpoint_abcd1234");
        assert_eq!(cp.description, "Test checkpoint");
    }

    #[test]
    fn test_checkpoint_full() {
        let cp = Checkpoint {
            id: "checkpoint_7bb3fd6e".to_string(),
            timestamp: "2026-03-07T17:44:14.659Z".to_string(),
            description: "# Full test".to_string(),
            checkpoint_type: Some(CheckpointType::Decision),
            context: Some("Testing context".to_string()),
            decision: Some("Use Rust".to_string()),
            alternatives: Some(vec!["Use TypeScript".to_string(), "Use Go".to_string()]),
            impact: Some("Major change".to_string()),
            evidence: Some(vec!["bench results".to_string()]),
            symbols: Some(vec!["SearchIndex".to_string(), "Checkpoint".to_string()]),
            next: Some("Build it".to_string()),
            confidence: Some(4),
            unknowns: Some(vec!["perf impact".to_string()]),
            tags: Some(vec!["architecture".to_string(), "decision".to_string()]),
            git: Some(GitContext {
                branch: Some("main".to_string()),
                commit: Some("62017a0".to_string()),
                files: Some(vec!["src/memory/mod.rs".to_string()]),
            }),
            summary: Some("Full test summary".to_string()),
            plan_id: Some("my-plan".to_string()),
        };
        assert_eq!(cp.checkpoint_type, Some(CheckpointType::Decision));
        assert_eq!(cp.confidence, Some(4));
        assert_eq!(cp.alternatives.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_plan_struct() {
        let plan = Plan {
            id: "my-plan".to_string(),
            title: "Test Plan".to_string(),
            content: "Plan content here".to_string(),
            status: "active".to_string(),
            created: "2026-03-07T17:44:14.659Z".to_string(),
            updated: "2026-03-07T17:44:14.659Z".to_string(),
            tags: vec!["test".to_string()],
        };
        assert_eq!(plan.id, "my-plan");
        assert_eq!(plan.status, "active");
    }

    #[test]
    fn test_recall_options_defaults() {
        let opts = RecallOptions::default();
        assert!(opts.limit.is_none());
        assert!(opts.since.is_none());
        assert!(opts.search.is_none());
        assert!(opts.full.is_none());
    }

    #[test]
    fn test_recall_result_struct() {
        let result = RecallResult {
            checkpoints: vec![],
            active_plan: None,
            workspaces: None,
        };
        assert!(result.checkpoints.is_empty());
        assert!(result.active_plan.is_none());
    }

    #[test]
    fn test_workspace_summary_struct() {
        let ws = WorkspaceSummary {
            name: "julie".to_string(),
            path: "/Users/murphy/Source/julie".to_string(),
            checkpoint_count: 42,
            last_activity: Some("2026-03-07T17:44:14.659Z".to_string()),
        };
        assert_eq!(ws.checkpoint_count, 42);
    }

    // ========================================================================
    // Checkpoint ID generation tests
    // ========================================================================

    #[test]
    fn test_generate_checkpoint_id_deterministic() {
        let id1 = generate_checkpoint_id("2026-03-07T17:44:14.659Z", "test description");
        let id2 = generate_checkpoint_id("2026-03-07T17:44:14.659Z", "test description");
        assert_eq!(id1, id2, "Same inputs must produce same ID");
    }

    #[test]
    fn test_generate_checkpoint_id_format() {
        let id = generate_checkpoint_id("2026-03-07T17:44:14.659Z", "test description");
        assert!(id.starts_with("checkpoint_"), "ID must start with 'checkpoint_'");
        // checkpoint_ = 11 chars, hash = 8 hex chars = 19 total
        assert_eq!(id.len(), 19, "ID must be exactly 19 characters");
        // The hash portion must be valid hex
        let hash_part = &id[11..];
        assert!(
            hash_part.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash portion must be hex: {}",
            hash_part
        );
    }

    #[test]
    fn test_generate_checkpoint_id_matches_goldfish() {
        // This is the actual checkpoint from the repo:
        // timestamp: 2026-03-07T17:44:14.659Z
        // description starts with "# Julie Platform Vision..."
        // id: checkpoint_7bb3fd6e
        //
        // In Goldfish, the hash input is `${timestamp}:${description}`
        // where description is the FULL markdown body.
        let timestamp = "2026-03-07T17:44:14.659Z";
        let description = "# Julie Platform Vision - Brainstorming Complete\n\n## What\nBrainstormed and designed a vision for transforming Julie from a per-session MCP code intelligence server into a **persistent personal developer intelligence platform**.\n\n## Why\nJulie as a per-session tool has reached its ceiling. The daemon model unlocks cross-project intelligence, integrated developer memory, a management web UI, and agent dispatch — capabilities that aren\'t possible with the current per-session architecture.\n\n## Key Decisions\n- **Daemon + HTTP from the start** — this is the foundational pivot; without it, Julie stays as-is\n- **Federated per-project indexes** (not shared) — 95% of queries are single-project, cross-project via parallel query + RRF merge\n- **MCP over Streamable HTTP** — same MCP protocol, daemon-compatible transport, same port as web UI\n- **Dual-level memory** — per-project `.memories/` (git-tracked, shareable) + user-level `~/.julie/memories/` (personal)\n- **Single embedding model** for all content types (code, memories, docs) — shared embedding space enables cross-content queries\n- **Agent dispatch via CLI backends** — `claude -p`, `codex -q`, API keys, Ollama — no credential management needed\n- **Cross-platform** — no platform-specific code in core daemon logic\n- **Dynamic embedding dimensions** — learned from Miller\'s LanceDB approach\n\n## Roadmap (5 phases)\n1. Daemon + HTTP Foundation (the pivot)\n2. Cross-Project Intelligence (federated search, auto-reference discovery)\n3. Memory Integration (Goldfish reunion, Rust implementation)\n4. Management UI + Agent Dispatch (command center, feedback loop)\n5. Search Enhancement (dynamic embeddings, weighted RRF per tool)";

        let id = generate_checkpoint_id(timestamp, description);
        assert_eq!(
            id, "checkpoint_7bb3fd6e",
            "Must match the real Goldfish checkpoint ID"
        );
    }

    #[test]
    fn test_generate_checkpoint_id_different_inputs() {
        let id1 = generate_checkpoint_id("2026-03-07T17:44:14.659Z", "description A");
        let id2 = generate_checkpoint_id("2026-03-07T17:44:14.659Z", "description B");
        assert_ne!(id1, id2, "Different descriptions must produce different IDs");

        let id3 = generate_checkpoint_id("2026-03-07T17:44:14.659Z", "same desc");
        let id4 = generate_checkpoint_id("2026-03-08T17:44:14.659Z", "same desc");
        assert_ne!(id3, id4, "Different timestamps must produce different IDs");
    }

    // ========================================================================
    // Checkpoint filename generation tests
    // ========================================================================

    #[test]
    fn test_get_checkpoint_filename_format() {
        let filename = get_checkpoint_filename(
            "2026-03-07T17:44:14.659Z",
            "checkpoint_7bb3fd6e",
        );
        assert_eq!(filename, "174414_7bb3.md");
    }

    #[test]
    fn test_get_checkpoint_filename_midnight() {
        let filename = get_checkpoint_filename(
            "2026-03-07T00:00:00.000Z",
            "checkpoint_abcd1234",
        );
        assert_eq!(filename, "000000_abcd.md");
    }

    #[test]
    fn test_get_checkpoint_filename_end_of_day() {
        let filename = get_checkpoint_filename(
            "2026-03-07T23:59:59.999Z",
            "checkpoint_efgh5678",
        );
        assert_eq!(filename, "235959_efgh.md");
    }

    // ========================================================================
    // format_checkpoint tests
    // ========================================================================

    #[test]
    fn test_format_checkpoint_minimal() {
        let cp = Checkpoint {
            id: "checkpoint_abcd1234".to_string(),
            timestamp: "2026-03-07T17:44:14.659Z".to_string(),
            description: "Simple test checkpoint".to_string(),
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
            tags: None,
            git: None,
            summary: None,
            plan_id: None,
        };

        let formatted = format_checkpoint(&cp);

        // Must have frontmatter delimiters
        assert!(formatted.starts_with("---\n"), "Must start with ---");
        assert!(
            formatted.contains("\n---\n"),
            "Must have closing frontmatter delimiter"
        );

        // Must contain required fields
        assert!(formatted.contains("id: checkpoint_abcd1234"));
        assert!(formatted.contains("timestamp: \"2026-03-07T17:44:14.659Z\"")
            || formatted.contains("timestamp: 2026-03-07T17:44:14.659Z"));

        // Must end with body
        assert!(formatted.ends_with("Simple test checkpoint\n"));

        // Must NOT contain optional fields that are None
        assert!(!formatted.contains("tags:"));
        assert!(!formatted.contains("git:"));
        assert!(!formatted.contains("summary:"));
        assert!(!formatted.contains("type:"));
        assert!(!formatted.contains("planId:"));
    }

    #[test]
    fn test_format_checkpoint_with_all_fields() {
        let cp = Checkpoint {
            id: "checkpoint_7bb3fd6e".to_string(),
            timestamp: "2026-03-07T17:44:14.659Z".to_string(),
            description: "# Full checkpoint\n\nWith body content.".to_string(),
            checkpoint_type: Some(CheckpointType::Decision),
            context: Some("Testing context".to_string()),
            decision: Some("Use Rust for everything".to_string()),
            alternatives: Some(vec!["TypeScript".to_string(), "Go".to_string()]),
            impact: Some("Major architecture change".to_string()),
            evidence: Some(vec!["benchmarks".to_string()]),
            symbols: Some(vec!["SearchIndex".to_string()]),
            next: Some("Implement it".to_string()),
            confidence: Some(4),
            unknowns: Some(vec!["performance".to_string()]),
            tags: Some(vec!["architecture".to_string(), "decision".to_string()]),
            git: Some(GitContext {
                branch: Some("main".to_string()),
                commit: Some("62017a0".to_string()),
                files: Some(vec!["src/memory/mod.rs".to_string()]),
            }),
            summary: Some("Full checkpoint summary".to_string()),
            plan_id: Some("my-plan".to_string()),
        };

        let formatted = format_checkpoint(&cp);

        // Verify key fields are present in frontmatter
        assert!(formatted.contains("type: decision"));
        assert!(formatted.contains("decision: Use Rust for everything"));
        assert!(formatted.contains("impact: Major architecture change"));
        assert!(formatted.contains("confidence: 4"));
        assert!(formatted.contains("planId: my-plan"));
        assert!(formatted.contains("branch: main"));
        assert!(formatted.contains("commit: 62017a0"));

        // Tags should be in list format
        assert!(formatted.contains("tags:"));
        assert!(formatted.contains("- architecture"));
        assert!(formatted.contains("- decision"));

        // Symbols list
        assert!(formatted.contains("symbols:"));
        assert!(formatted.contains("- SearchIndex"));

        // Alternatives list
        assert!(formatted.contains("alternatives:"));
        assert!(formatted.contains("- TypeScript"));
        assert!(formatted.contains("- Go"));

        // Body after frontmatter
        assert!(formatted.contains("# Full checkpoint\n\nWith body content."));
    }

    #[test]
    fn test_format_checkpoint_git_partial() {
        // Git context with only branch (no commit, no files)
        let cp = Checkpoint {
            id: "checkpoint_test0001".to_string(),
            timestamp: "2026-03-07T12:00:00.000Z".to_string(),
            description: "Partial git".to_string(),
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
            tags: None,
            git: Some(GitContext {
                branch: Some("feature/test".to_string()),
                commit: None,
                files: None,
            }),
            summary: None,
            plan_id: None,
        };

        let formatted = format_checkpoint(&cp);
        assert!(formatted.contains("git:"));
        assert!(formatted.contains("branch: feature/test"));
        assert!(!formatted.contains("commit:"));
        assert!(!formatted.contains("files:"));
    }

    // ========================================================================
    // parse_checkpoint tests
    // ========================================================================

    #[test]
    fn test_parse_checkpoint_minimal() {
        let content = "---\nid: checkpoint_abcd1234\ntimestamp: \"2026-03-07T17:44:14.659Z\"\n---\n\nSimple body text";
        let cp = parse_checkpoint(content).unwrap();
        assert_eq!(cp.id, "checkpoint_abcd1234");
        assert_eq!(cp.timestamp, "2026-03-07T17:44:14.659Z");
        assert_eq!(cp.description, "Simple body text");
        assert!(cp.checkpoint_type.is_none());
        assert!(cp.tags.is_none());
        assert!(cp.git.is_none());
    }

    #[test]
    fn test_parse_checkpoint_with_git() {
        let content = r#"---
id: checkpoint_7bb3fd6e
timestamp: "2026-03-07T17:44:14.659Z"
git:
  branch: main
  commit: 62017a0
  files:
    - src/main.rs
    - Cargo.toml
---

Body with git context"#;

        let cp = parse_checkpoint(content).unwrap();
        let git = cp.git.unwrap();
        assert_eq!(git.branch.as_deref(), Some("main"));
        assert_eq!(git.commit.as_deref(), Some("62017a0"));
        assert_eq!(git.files.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_parse_checkpoint_all_fields() {
        let content = r#"---
id: checkpoint_7bb3fd6e
timestamp: "2026-03-07T17:44:14.659Z"
tags:
  - vision
  - architecture
git:
  branch: main
  commit: 62017a0
  files:
    - docs/plans/design.md
summary: "Vision brainstorm complete"
planId: sidecar-binary-distribution
type: decision
decision: Use Rust for everything
alternatives:
  - Use TypeScript
  - Use Go
impact: Major architecture change
evidence:
  - benchmarks show 10x improvement
symbols:
  - SearchIndex
  - Checkpoint
next: Build it
confidence: 4
unknowns:
  - performance under load
context: Testing all fields
---

# Full Body

With multiple paragraphs."#;

        let cp = parse_checkpoint(content).unwrap();
        assert_eq!(cp.id, "checkpoint_7bb3fd6e");
        assert_eq!(cp.checkpoint_type, Some(CheckpointType::Decision));
        assert_eq!(cp.decision.as_deref(), Some("Use Rust for everything"));
        assert_eq!(cp.alternatives.as_ref().unwrap().len(), 2);
        assert_eq!(cp.impact.as_deref(), Some("Major architecture change"));
        assert_eq!(cp.evidence.as_ref().unwrap().len(), 1);
        assert_eq!(cp.symbols.as_ref().unwrap().len(), 2);
        assert_eq!(cp.next.as_deref(), Some("Build it"));
        assert_eq!(cp.confidence, Some(4));
        assert_eq!(cp.unknowns.as_ref().unwrap().len(), 1);
        assert_eq!(cp.tags.as_ref().unwrap().len(), 2);
        assert_eq!(
            cp.summary.as_deref(),
            Some("Vision brainstorm complete")
        );
        assert_eq!(cp.plan_id.as_deref(), Some("sidecar-binary-distribution"));
        assert_eq!(cp.context.as_deref(), Some("Testing all fields"));
        assert_eq!(cp.description, "# Full Body\n\nWith multiple paragraphs.");
    }

    // ========================================================================
    // Round-trip tests
    // ========================================================================

    #[test]
    fn test_roundtrip_minimal() {
        let original = Checkpoint {
            id: "checkpoint_abcd1234".to_string(),
            timestamp: "2026-03-07T17:44:14.659Z".to_string(),
            description: "Round trip test".to_string(),
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
            tags: None,
            git: None,
            summary: None,
            plan_id: None,
        };

        let formatted = format_checkpoint(&original);
        let parsed = parse_checkpoint(&formatted).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.timestamp, original.timestamp);
        assert_eq!(parsed.description, original.description);
    }

    #[test]
    fn test_roundtrip_full() {
        let original = Checkpoint {
            id: "checkpoint_7bb3fd6e".to_string(),
            timestamp: "2026-03-07T17:44:14.659Z".to_string(),
            description: "# Complete roundtrip\n\nWith **markdown** content.".to_string(),
            checkpoint_type: Some(CheckpointType::Decision),
            context: Some("Full roundtrip context".to_string()),
            decision: Some("Roundtrip works".to_string()),
            alternatives: Some(vec!["Alt A".to_string(), "Alt B".to_string()]),
            impact: Some("Proves correctness".to_string()),
            evidence: Some(vec!["this test".to_string()]),
            symbols: Some(vec!["format_checkpoint".to_string(), "parse_checkpoint".to_string()]),
            next: Some("Ship it".to_string()),
            confidence: Some(5),
            unknowns: Some(vec!["edge cases".to_string()]),
            tags: Some(vec!["test".to_string(), "roundtrip".to_string()]),
            git: Some(GitContext {
                branch: Some("feature/memory".to_string()),
                commit: Some("abc1234".to_string()),
                files: Some(vec!["src/memory/mod.rs".to_string(), "src/memory/storage.rs".to_string()]),
            }),
            summary: Some("Complete roundtrip test".to_string()),
            plan_id: Some("test-plan".to_string()),
        };

        let formatted = format_checkpoint(&original);
        let parsed = parse_checkpoint(&formatted).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.timestamp, original.timestamp);
        assert_eq!(parsed.description, original.description);
        assert_eq!(parsed.checkpoint_type, original.checkpoint_type);
        assert_eq!(parsed.context, original.context);
        assert_eq!(parsed.decision, original.decision);
        assert_eq!(parsed.alternatives, original.alternatives);
        assert_eq!(parsed.impact, original.impact);
        assert_eq!(parsed.evidence, original.evidence);
        assert_eq!(parsed.symbols, original.symbols);
        assert_eq!(parsed.next, original.next);
        assert_eq!(parsed.confidence, original.confidence);
        assert_eq!(parsed.unknowns, original.unknowns);
        assert_eq!(parsed.tags, original.tags);
        assert_eq!(parsed.summary, original.summary);
        assert_eq!(parsed.plan_id, original.plan_id);

        // Git context
        let orig_git = original.git.unwrap();
        let parsed_git = parsed.git.unwrap();
        assert_eq!(parsed_git.branch, orig_git.branch);
        assert_eq!(parsed_git.commit, orig_git.commit);
        assert_eq!(parsed_git.files, orig_git.files);
    }

    // ========================================================================
    // Goldfish compatibility tests — parse real .memories/ files
    // ========================================================================

    #[test]
    fn test_parse_real_goldfish_checkpoint_decision() {
        // This is the actual content from .memories/2026-03-07/174414_7bb3.md
        let content = std::fs::read_to_string(
            "/Users/murphy/Source/julie/.memories/2026-03-07/174414_7bb3.md",
        );

        // If the file doesn't exist in CI, skip gracefully
        let content = match content {
            Ok(c) => c,
            Err(_) => return,
        };

        let cp = parse_checkpoint(&content).unwrap();
        assert_eq!(cp.id, "checkpoint_7bb3fd6e");
        assert_eq!(cp.timestamp, "2026-03-07T17:44:14.659Z");
        assert_eq!(cp.checkpoint_type, Some(CheckpointType::Decision));
        assert!(cp.tags.as_ref().unwrap().contains(&"vision".to_string()));
        assert!(cp.tags.as_ref().unwrap().contains(&"architecture".to_string()));
        assert_eq!(
            cp.git.as_ref().unwrap().branch.as_deref(),
            Some("main")
        );
        assert_eq!(
            cp.git.as_ref().unwrap().commit.as_deref(),
            Some("62017a0")
        );
        assert!(cp.decision.is_some());
        assert!(cp.alternatives.is_some());
        assert!(cp.impact.is_some());
        assert!(cp.symbols.is_some());
        assert_eq!(cp.plan_id.as_deref(), Some("sidecar-binary-distribution"));
        // Body should be the markdown after frontmatter
        assert!(cp.description.starts_with("# Julie Platform Vision"));
    }

    #[test]
    fn test_parse_real_goldfish_checkpoint_standard() {
        // This is the actual content from .memories/2026-03-07/213126_39de.md
        let content = std::fs::read_to_string(
            "/Users/murphy/Source/julie/.memories/2026-03-07/213126_39de.md",
        );

        let content = match content {
            Ok(c) => c,
            Err(_) => return,
        };

        let cp = parse_checkpoint(&content).unwrap();
        assert_eq!(cp.id, "checkpoint_39deec85");
        assert_eq!(cp.checkpoint_type, Some(CheckpointType::Checkpoint));
        assert!(cp.impact.is_some());
        assert!(cp.next.is_some());
        assert_eq!(cp.confidence, Some(4));
        assert!(cp.description.contains("Phase 1"));
    }

    #[test]
    fn test_parse_legacy_format_with_files_changed() {
        // Legacy format used `files_changed` instead of `files`
        // and `dirty` field. Our parser should handle both.
        let content = r#"---
id: checkpoint_69850b28_babb03
timestamp: 1770326824
git:
  branch: main
  commit: 1a33684
  dirty: true
  files_changed:
    - src/search/index.rs
    - src/tools/memory/mod.rs
tags:
  - verification
type: checkpoint
---

Verified markdown memory format working."#;

        let cp = parse_checkpoint(content).unwrap();
        assert_eq!(cp.id, "checkpoint_69850b28_babb03");
        // Legacy unix timestamp should be normalized to ISO 8601
        assert!(
            cp.timestamp.contains("2026") || cp.timestamp.contains("T"),
            "Unix timestamp should be normalized: {}",
            cp.timestamp
        );
        assert_eq!(cp.checkpoint_type, Some(CheckpointType::Checkpoint));
        // files_changed should be normalized to files
        let git = cp.git.unwrap();
        assert_eq!(git.branch.as_deref(), Some("main"));
        assert!(git.files.is_some());
        assert_eq!(git.files.as_ref().unwrap().len(), 2);
    }

    // ========================================================================
    // Edge cases
    // ========================================================================

    #[test]
    fn test_parse_checkpoint_no_frontmatter_error() {
        let content = "Just some text without frontmatter";
        let result = parse_checkpoint(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_checkpoint_empty_body() {
        let content = "---\nid: checkpoint_test\ntimestamp: \"2026-03-07T12:00:00Z\"\n---\n\n";
        let cp = parse_checkpoint(content).unwrap();
        assert_eq!(cp.description, "");
    }

    #[test]
    fn test_parse_checkpoint_crlf_line_endings() {
        let content = "---\r\nid: checkpoint_test\r\ntimestamp: \"2026-03-07T12:00:00Z\"\r\n---\r\n\r\nBody with CRLF";
        let cp = parse_checkpoint(content).unwrap();
        assert_eq!(cp.id, "checkpoint_test");
        assert_eq!(cp.description, "Body with CRLF");
    }

    #[test]
    fn test_parse_checkpoint_bom() {
        let content = "\u{FEFF}---\nid: checkpoint_bom\ntimestamp: \"2026-03-07T12:00:00Z\"\n---\n\nBOM test";
        let cp = parse_checkpoint(content).unwrap();
        assert_eq!(cp.id, "checkpoint_bom");
    }

    #[test]
    fn test_format_checkpoint_empty_tags_omitted() {
        // Empty vec should not produce a tags field in output
        let cp = Checkpoint {
            id: "checkpoint_test".to_string(),
            timestamp: "2026-03-07T12:00:00Z".to_string(),
            description: "Test".to_string(),
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
            tags: Some(vec![]),
            git: None,
            summary: None,
            plan_id: None,
        };

        let formatted = format_checkpoint(&cp);
        assert!(!formatted.contains("tags:"), "Empty tags vec should be omitted");
    }

    #[test]
    fn test_format_checkpoint_empty_git_omitted() {
        // GitContext with all None fields should not produce git field
        let cp = Checkpoint {
            id: "checkpoint_test".to_string(),
            timestamp: "2026-03-07T12:00:00Z".to_string(),
            description: "Test".to_string(),
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
            tags: None,
            git: Some(GitContext {
                branch: None,
                commit: None,
                files: None,
            }),
            summary: None,
            plan_id: None,
        };

        let formatted = format_checkpoint(&cp);
        assert!(!formatted.contains("git:"), "Empty git context should be omitted");
    }

    // ========================================================================
    // Serde serialization tests (JSON — for MCP tool output)
    // ========================================================================

    #[test]
    fn test_checkpoint_json_skip_none_fields() {
        let cp = Checkpoint {
            id: "checkpoint_test".to_string(),
            timestamp: "2026-03-07T12:00:00Z".to_string(),
            description: "Test".to_string(),
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
            tags: None,
            git: None,
            summary: None,
            plan_id: None,
        };

        let json = serde_json::to_string(&cp).unwrap();
        assert!(!json.contains("type"), "None fields should be skipped in JSON");
        assert!(!json.contains("tags"));
        assert!(!json.contains("git"));
        assert!(!json.contains("summary"));
        assert!(!json.contains("planId"));
        // Required fields should be present
        assert!(json.contains("\"id\""));
        assert!(json.contains("\"timestamp\""));
        assert!(json.contains("\"description\""));
    }

    #[test]
    fn test_checkpoint_json_plan_id_camel_case() {
        // planId field should serialize as "planId" not "plan_id"
        let cp = Checkpoint {
            id: "checkpoint_test".to_string(),
            timestamp: "2026-03-07T12:00:00Z".to_string(),
            description: "Test".to_string(),
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
            tags: None,
            git: None,
            summary: None,
            plan_id: Some("test-plan".to_string()),
        };

        let json = serde_json::to_string(&cp).unwrap();
        assert!(
            json.contains("\"planId\""),
            "plan_id should serialize as planId: {}",
            json
        );
        assert!(
            !json.contains("\"plan_id\""),
            "Should not contain snake_case plan_id"
        );
    }

    #[test]
    fn test_checkpoint_type_json_field_name() {
        // The `type` field should serialize as "type" in JSON
        let cp = Checkpoint {
            id: "checkpoint_test".to_string(),
            timestamp: "2026-03-07T12:00:00Z".to_string(),
            description: "Test".to_string(),
            checkpoint_type: Some(CheckpointType::Learning),
            context: None,
            decision: None,
            alternatives: None,
            impact: None,
            evidence: None,
            symbols: None,
            next: None,
            confidence: None,
            unknowns: None,
            tags: None,
            git: None,
            summary: None,
            plan_id: None,
        };

        let json = serde_json::to_string(&cp).unwrap();
        assert!(
            json.contains("\"type\":\"learning\""),
            "checkpoint_type should serialize as 'type': {}",
            json
        );
    }
}
