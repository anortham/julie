//! Tests for memory Tantivy index (`src/memory/index.rs`).
//!
//! Covers: `MemoryIndex` — creation, checkpoint indexing, BM25 search across
//! body/tags/symbols/decision/impact fields, and rebuild_from_files.

#[cfg(test)]
mod tests {
    use crate::memory::index::MemoryIndex;
    use crate::memory::storage::{format_checkpoint, generate_checkpoint_id};
    use crate::memory::{Checkpoint, GitContext};
    use std::path::Path;
    use tempfile::TempDir;

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Create a minimal checkpoint for testing.
    fn make_checkpoint(
        timestamp: &str,
        description: &str,
        tags: Option<Vec<String>>,
        symbols: Option<Vec<String>>,
        decision: Option<String>,
        impact: Option<String>,
        git_branch: Option<String>,
    ) -> Checkpoint {
        let id = generate_checkpoint_id(timestamp, description);
        Checkpoint {
            id,
            timestamp: timestamp.to_string(),
            description: description.to_string(),
            checkpoint_type: None,
            context: None,
            decision,
            alternatives: None,
            impact,
            evidence: None,
            symbols,
            next: None,
            confidence: None,
            unknowns: None,
            tags,
            git: git_branch.map(|b| GitContext {
                branch: Some(b),
                commit: None,
                files: None,
            }),
            summary: None,
            plan_id: None,
        }
    }

    /// Write a checkpoint file at .memories/{date}/{HHMMSS}_{hash}.md
    fn write_checkpoint(root: &Path, checkpoint: &Checkpoint) {
        let date = &checkpoint.timestamp[..10]; // YYYY-MM-DD
        let date_dir = root.join(".memories").join(date);
        std::fs::create_dir_all(&date_dir).unwrap();

        let id = &checkpoint.id;
        let time_part = &checkpoint.timestamp[11..19]; // HH:MM:SS
        let hhmmss = time_part.replace(':', "");
        let hash4 = id
            .strip_prefix("checkpoint_")
            .unwrap_or(id)
            .get(..4)
            .unwrap_or("0000");
        let filename = format!("{}_{}.md", hhmmss, hash4);

        let content = format_checkpoint(checkpoint);
        std::fs::write(date_dir.join(&filename), &content).unwrap();
    }

    // ========================================================================
    // Creation tests
    // ========================================================================

    #[test]
    fn test_create_memory_index() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path);
        assert!(index.is_ok(), "Should create a new memory index");
        assert_eq!(index.unwrap().num_docs(), 0);
    }

    #[test]
    fn test_open_or_create_memory_index() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        // First call creates
        let index = MemoryIndex::open_or_create(&index_path).unwrap();
        assert_eq!(index.num_docs(), 0);
        drop(index);

        // Second call opens
        let index = MemoryIndex::open_or_create(&index_path).unwrap();
        assert_eq!(index.num_docs(), 0);
    }

    // ========================================================================
    // add_checkpoint + commit tests
    // ========================================================================

    #[test]
    fn test_add_checkpoint_and_search_by_body() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Refactored the database connection pooling to use async/await patterns",
            Some(vec!["architecture".to_string(), "database".to_string()]),
            Some(vec!["ConnectionPool".to_string(), "DbManager".to_string()]),
            Some("Use tokio connection pool".to_string()),
            Some("Reduced latency by 40%".to_string()),
            Some("feature/db-pool".to_string()),
        );

        index.add_checkpoint(&cp, None).unwrap();
        index.commit().unwrap();

        assert_eq!(index.num_docs(), 1);

        // Search by body text
        let results = index.search("database connection pooling", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].score > 0.0);
        assert!(results[0].body.contains("database connection pooling"));
        assert_eq!(results[0].id, cp.id);
    }

    #[test]
    fn test_search_by_tags() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp1 = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Added new feature X",
            Some(vec!["performance".to_string(), "optimization".to_string()]),
            None,
            None,
            None,
            None,
        );

        let cp2 = make_checkpoint(
            "2026-03-07T11:00:00.000Z",
            "Fixed a small bug",
            Some(vec!["bugfix".to_string()]),
            None,
            None,
            None,
            None,
        );

        index.add_checkpoint(&cp1, None).unwrap();
        index.add_checkpoint(&cp2, None).unwrap();
        index.commit().unwrap();

        let results = index.search("performance optimization", 10).unwrap();
        assert!(!results.is_empty());
        // The checkpoint with performance tag should rank highest
        assert_eq!(results[0].id, cp1.id);
    }

    #[test]
    fn test_search_by_symbols() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Updated error handling in the parser",
            None,
            Some(vec!["ParseError".to_string(), "TokenStream".to_string()]),
            None,
            None,
            None,
        );

        index.add_checkpoint(&cp, None).unwrap();
        index.commit().unwrap();

        let results = index.search("ParseError", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, cp.id);
        assert!(results[0].symbols.contains("ParseError"));
    }

    #[test]
    fn test_search_by_decision() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Reviewed authentication options",
            None,
            None,
            Some("Chose JWT over session cookies for stateless auth".to_string()),
            None,
            None,
        );

        index.add_checkpoint(&cp, None).unwrap();
        index.commit().unwrap();

        let results = index.search("JWT stateless auth", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, cp.id);
    }

    #[test]
    fn test_search_by_impact() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Infrastructure change",
            None,
            None,
            None,
            Some("Reduced deployment time from 20 minutes to 3 minutes".to_string()),
            None,
        );

        index.add_checkpoint(&cp, None).unwrap();
        index.commit().unwrap();

        let results = index.search("deployment time reduced", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, cp.id);
    }

    // ========================================================================
    // Result fields completeness
    // ========================================================================

    #[test]
    fn test_search_result_has_all_fields() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp = make_checkpoint(
            "2026-03-07T14:30:00.000Z",
            "Complete checkpoint with all fields",
            Some(vec!["tag1".to_string(), "tag2".to_string()]),
            Some(vec!["SymbolA".to_string(), "SymbolB".to_string()]),
            Some("Decided on approach X".to_string()),
            Some("Improved throughput by 50%".to_string()),
            Some("main".to_string()),
        );

        index
            .add_checkpoint(&cp, Some("2026-03-07/143000_abcd.md"))
            .unwrap();
        index.commit().unwrap();

        let results = index.search("complete checkpoint", 10).unwrap();
        assert_eq!(results.len(), 1);

        let r = &results[0];
        assert_eq!(r.id, cp.id);
        assert_eq!(r.body, "Complete checkpoint with all fields");
        assert_eq!(r.tags, "tag1 tag2");
        assert_eq!(r.symbols, "SymbolA SymbolB");
        assert_eq!(r.decision, "Decided on approach X");
        assert_eq!(r.impact, "Improved throughput by 50%");
        assert_eq!(r.branch, "main");
        assert_eq!(r.timestamp, "2026-03-07T14:30:00.000Z");
        assert_eq!(r.file_path, "2026-03-07/143000_abcd.md");
        assert!(r.score > 0.0);
    }

    // ========================================================================
    // None/empty field handling
    // ========================================================================

    #[test]
    fn test_add_checkpoint_with_none_fields() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Minimal checkpoint with sparse optional fields",
            None, // no tags
            None, // no symbols
            None, // no decision
            None, // no impact
            None, // no git branch
        );

        index.add_checkpoint(&cp, None).unwrap();
        index.commit().unwrap();

        let results = index.search("minimal checkpoint sparse", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tags, "");
        assert_eq!(results[0].symbols, "");
        assert_eq!(results[0].decision, "");
        assert_eq!(results[0].impact, "");
        assert_eq!(results[0].branch, "");
    }

    // ========================================================================
    // Limit and ranking
    // ========================================================================

    #[test]
    fn test_search_respects_limit() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        for i in 0..10 {
            let cp = make_checkpoint(
                &format!("2026-03-07T{:02}:00:00.000Z", i + 8),
                &format!("Checkpoint about database migration number {}", i),
                Some(vec!["database".to_string()]),
                None,
                None,
                None,
                None,
            );
            index.add_checkpoint(&cp, None).unwrap();
        }
        index.commit().unwrap();

        let results = index.search("database migration", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_no_results() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Fixed a CSS layout bug",
            Some(vec!["frontend".to_string()]),
            None,
            None,
            None,
            None,
        );
        index.add_checkpoint(&cp, None).unwrap();
        index.commit().unwrap();

        let results = index.search("kubernetes deployment cluster", 10).unwrap();
        assert!(results.is_empty());
    }

    // ========================================================================
    // clear_all
    // ========================================================================

    #[test]
    fn test_clear_all() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        let cp = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Some checkpoint to be cleared",
            None,
            None,
            None,
            None,
            None,
        );
        index.add_checkpoint(&cp, None).unwrap();
        index.commit().unwrap();
        assert_eq!(index.num_docs(), 1);

        index.clear_all().unwrap();
        assert_eq!(index.num_docs(), 0);
    }

    // ========================================================================
    // rebuild_from_files
    // ========================================================================

    #[test]
    fn test_rebuild_from_files() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path();

        // Create checkpoint files on disk
        let cp1 = make_checkpoint(
            "2026-03-06T09:00:00.000Z",
            "Implemented the caching layer for API responses",
            Some(vec!["caching".to_string(), "api".to_string()]),
            Some(vec!["CacheManager".to_string()]),
            None,
            None,
            Some("feature/cache".to_string()),
        );
        let cp2 = make_checkpoint(
            "2026-03-07T15:00:00.000Z",
            "Refactored authentication middleware to support OAuth2",
            Some(vec!["auth".to_string(), "middleware".to_string()]),
            Some(vec!["AuthMiddleware".to_string()]),
            Some("Use passport.js for OAuth2".to_string()),
            Some("Simplified auth flow".to_string()),
            Some("feature/oauth".to_string()),
        );

        write_checkpoint(workspace_root, &cp1);
        write_checkpoint(workspace_root, &cp2);

        // Create index and rebuild
        let index_path = tmp.path().join("index_tantivy");
        std::fs::create_dir_all(&index_path).unwrap();
        let index = MemoryIndex::create(&index_path).unwrap();

        index.rebuild_from_files(workspace_root).unwrap();
        assert_eq!(index.num_docs(), 2);

        // Search should find both
        let results = index.search("caching API responses", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].id, cp1.id);

        let results = index.search("authentication OAuth2", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].id, cp2.id);
    }

    #[test]
    fn test_rebuild_from_files_clears_existing() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path();

        let index_path = tmp.path().join("index_tantivy");
        std::fs::create_dir_all(&index_path).unwrap();
        let index = MemoryIndex::create(&index_path).unwrap();

        // Add a checkpoint manually with unique terms not shared by the new one
        let cp_old = make_checkpoint(
            "2026-03-05T10:00:00.000Z",
            "Dinosaur paleontology excavation fossils jurassic",
            None,
            None,
            None,
            None,
            None,
        );
        index.add_checkpoint(&cp_old, None).unwrap();
        index.commit().unwrap();
        assert_eq!(index.num_docs(), 1);

        // Now write a different checkpoint to disk and rebuild
        let cp_new = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Astronomy telescope observatory stargazing nebula",
            Some(vec!["astronomy".to_string()]),
            None,
            None,
            None,
            None,
        );
        write_checkpoint(workspace_root, &cp_new);

        index.rebuild_from_files(workspace_root).unwrap();
        assert_eq!(index.num_docs(), 1);

        // Old checkpoint should be gone — search for its unique terms
        let results = index.search("dinosaur paleontology fossils", 10).unwrap();
        assert!(results.is_empty());

        // New checkpoint should be present
        let results = index.search("astronomy telescope nebula", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_rebuild_from_files_no_memories_dir() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path();
        // No .memories/ directory

        let index_path = tmp.path().join("index_tantivy");
        std::fs::create_dir_all(&index_path).unwrap();
        let index = MemoryIndex::create(&index_path).unwrap();

        // Should succeed with 0 documents (not error)
        index.rebuild_from_files(workspace_root).unwrap();
        assert_eq!(index.num_docs(), 0);
    }

    #[test]
    fn test_rebuild_preserves_file_path() {
        let tmp = TempDir::new().unwrap();
        let workspace_root = tmp.path();

        let cp = make_checkpoint(
            "2026-03-07T12:30:00.000Z",
            "Checkpoint with file path tracking",
            None,
            None,
            None,
            None,
            None,
        );
        write_checkpoint(workspace_root, &cp);

        let index_path = tmp.path().join("index_tantivy");
        std::fs::create_dir_all(&index_path).unwrap();
        let index = MemoryIndex::create(&index_path).unwrap();

        index.rebuild_from_files(workspace_root).unwrap();

        let results = index.search("file path tracking", 10).unwrap();
        assert_eq!(results.len(), 1);
        // file_path should be a relative path like "2026-03-07/123000_xxxx.md"
        assert!(
            results[0].file_path.starts_with("2026-03-07/"),
            "file_path should be relative to .memories/: {}",
            results[0].file_path
        );
        assert!(results[0].file_path.ends_with(".md"));
    }

    // ========================================================================
    // Multiple fields in search ranking
    // ========================================================================

    #[test]
    fn test_search_ranks_across_multiple_fields() {
        let tmp = TempDir::new().unwrap();
        let index_path = tmp.path().join("tantivy");
        std::fs::create_dir_all(&index_path).unwrap();

        let index = MemoryIndex::create(&index_path).unwrap();

        // cp1: "Tantivy" only in decision
        let cp1 = make_checkpoint(
            "2026-03-07T10:00:00.000Z",
            "Made a search engine choice",
            None,
            None,
            Some("Chose Tantivy over Elasticsearch for embedded search".to_string()),
            None,
            None,
        );

        // cp2: "Tantivy" in body AND tags AND symbols
        let cp2 = make_checkpoint(
            "2026-03-07T11:00:00.000Z",
            "Implemented Tantivy integration for full-text search indexing",
            Some(vec!["tantivy".to_string(), "search".to_string()]),
            Some(vec!["TantivyIndex".to_string()]),
            None,
            None,
            None,
        );

        index.add_checkpoint(&cp1, None).unwrap();
        index.add_checkpoint(&cp2, None).unwrap();
        index.commit().unwrap();

        let results = index.search("Tantivy", 10).unwrap();
        assert_eq!(results.len(), 2);
        // cp2 should rank higher (Tantivy appears in more fields)
        assert_eq!(results[0].id, cp2.id);
    }
}
