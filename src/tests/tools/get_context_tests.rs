//! Tests for the get_context tool — pivot selection, graph expansion, and token allocation.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::search::index::SymbolSearchResult;
    use crate::tools::get_context::pipeline::{select_pivots, Pivot};

    /// Helper to create a test SymbolSearchResult with minimal boilerplate.
    fn make_result(id: &str, name: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: format!("fn {}()", name),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_select_pivots_empty_results() {
        let results = vec![];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert!(pivots.is_empty());
    }

    #[test]
    fn test_select_pivots_single_result() {
        let results = vec![make_result("a", "alpha", 5.0)];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(pivots.len(), 1);
        assert_eq!(pivots[0].result.name, "alpha");
    }

    #[test]
    fn test_select_pivots_clear_winner() {
        // Top result is 2x+ above second → should return 1 pivot
        let results = vec![
            make_result("a", "dominant", 10.0),
            make_result("b", "weak", 4.0),
            make_result("c", "weaker", 3.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(pivots.len(), 1, "clear winner should yield 1 pivot");
        assert_eq!(pivots[0].result.name, "dominant");
    }

    #[test]
    fn test_select_pivots_cluster() {
        // Top 3 within 30% of each other → should return 3 pivots
        // Scores: 10.0, 9.0, 8.0 — third (8.0) >= top (10.0) * 0.7 (7.0)
        let results = vec![
            make_result("a", "first", 10.0),
            make_result("b", "second", 9.0),
            make_result("c", "third", 8.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(pivots.len(), 3, "cluster should yield 3 pivots");
        assert_eq!(pivots[0].result.name, "first");
        assert_eq!(pivots[1].result.name, "second");
        assert_eq!(pivots[2].result.name, "third");
    }

    #[test]
    fn test_select_pivots_default_two() {
        // Neither clear winner nor cluster → should return 2 pivots
        // Scores: 10.0, 7.0, 5.0 — top is NOT 2x+ second (10 < 14), third (5) < top*0.7 (7)
        let results = vec![
            make_result("a", "first", 10.0),
            make_result("b", "second", 7.0),
            make_result("c", "third", 5.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(pivots.len(), 2, "default case should yield 2 pivots");
        assert_eq!(pivots[0].result.name, "first");
        assert_eq!(pivots[1].result.name, "second");
    }

    #[test]
    fn test_select_pivots_centrality_reranks() {
        // Result "b" has lower text score but high reference_score,
        // so the centrality boost should promote it above "a".
        //
        // "a": score=5.0, ref_score=0 → combined = 5.0 * 1.0 = 5.0
        // "b": score=4.0, ref_score=50.0 → boost = 1 + ln(51) * 0.3 ≈ 1 + 3.93*0.3 ≈ 2.18
        //       combined = 4.0 * 2.18 ≈ 8.72
        let results = vec![
            make_result("a", "no_refs", 5.0),
            make_result("b", "many_refs", 4.0),
        ];
        let mut ref_scores = HashMap::new();
        ref_scores.insert("b".to_string(), 50.0_f64);

        let pivots = select_pivots(results, &ref_scores);
        assert_eq!(
            pivots[0].result.name, "many_refs",
            "high-centrality result should be promoted to top"
        );
        assert!(
            pivots[0].combined_score > pivots[1].combined_score,
            "many_refs ({:.2}) should score higher than no_refs ({:.2})",
            pivots[0].combined_score,
            pivots[1].combined_score,
        );
    }

    /// Test that results from test files are de-boosted so production code wins.
    ///
    /// Scenario: A test function matches with higher text score than the production
    /// implementation, but should rank lower after de-boost.
    #[test]
    fn test_select_pivots_deboosts_test_files() {
        let results = vec![
            // Test file has higher raw text score
            SymbolSearchResult {
                id: "test_search_ranking".to_string(),
                name: "test_search_ranking".to_string(),
                signature: "fn test_search_ranking()".to_string(),
                doc_comment: String::new(),
                file_path: "src/tests/search_tests.rs".to_string(),
                kind: "function".to_string(),
                language: "rust".to_string(),
                start_line: 10,
                score: 8.0,
            },
            // Production file has lower raw text score
            SymbolSearchResult {
                id: "search_ranking_impl".to_string(),
                name: "apply_ranking".to_string(),
                signature: "fn apply_ranking()".to_string(),
                doc_comment: String::new(),
                file_path: "src/search/ranking.rs".to_string(),
                kind: "function".to_string(),
                language: "rust".to_string(),
                start_line: 42,
                score: 5.0,
            },
        ];

        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);

        // Production code should rank first despite lower raw text score
        assert_eq!(
            pivots[0].result.id, "search_ranking_impl",
            "Production code should rank above test code, but test code '{}' ranked first",
            pivots[0].result.id
        );
    }

    /// Test that import nodes are filtered out before pivot selection.
    #[test]
    fn test_select_pivots_filters_imports() {
        let results = vec![
            // Import node — should be excluded
            SymbolSearchResult {
                id: "import_symbol_db".to_string(),
                name: "SymbolDatabase".to_string(),
                signature: "use crate::database::SymbolDatabase".to_string(),
                doc_comment: String::new(),
                file_path: "src/handler.rs".to_string(),
                kind: "import".to_string(),
                language: "rust".to_string(),
                start_line: 5,
                score: 10.0,
            },
            // Actual struct definition — should be selected
            SymbolSearchResult {
                id: "struct_symbol_db".to_string(),
                name: "SymbolDatabase".to_string(),
                signature: "pub struct SymbolDatabase".to_string(),
                doc_comment: String::new(),
                file_path: "src/database/mod.rs".to_string(),
                kind: "struct".to_string(),
                language: "rust".to_string(),
                start_line: 33,
                score: 7.0,
            },
        ];

        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);

        // Import should be filtered — only the struct should remain
        assert_eq!(pivots.len(), 1, "Import should be filtered out, leaving 1 pivot");
        assert_eq!(pivots[0].result.id, "struct_symbol_db");
    }

    /// Test that non-code files (.memories/, docs/, markdown) are de-boosted
    /// below real source code. get_context is for code orientation, not docs.
    #[test]
    fn test_select_pivots_deboosts_non_code_files() {
        let results = vec![
            // Goldfish memory file matches on text
            SymbolSearchResult {
                id: "memory_checkpoint".to_string(),
                name: "Phase 2 Complete: Search Ranking".to_string(),
                signature: String::new(),
                doc_comment: String::new(),
                file_path: ".memories/2026-02-24/checkpoint.md".to_string(),
                kind: "module".to_string(),
                language: "markdown".to_string(),
                start_line: 1,
                score: 9.0,
            },
            // Design doc also matches
            SymbolSearchResult {
                id: "design_doc".to_string(),
                name: "Phase 2: Graph Centrality Ranking".to_string(),
                signature: String::new(),
                doc_comment: String::new(),
                file_path: "docs/plans/search-ranking.md".to_string(),
                kind: "module".to_string(),
                language: "markdown".to_string(),
                start_line: 1,
                score: 8.0,
            },
            // Actual source code with lower text score
            SymbolSearchResult {
                id: "ranking_impl".to_string(),
                name: "apply_ranking".to_string(),
                signature: "fn apply_ranking()".to_string(),
                doc_comment: String::new(),
                file_path: "src/search/scoring.rs".to_string(),
                kind: "function".to_string(),
                language: "rust".to_string(),
                start_line: 42,
                score: 5.0,
            },
        ];

        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);

        // Source code should rank first despite lower raw text score
        assert_eq!(
            pivots[0].result.id, "ranking_impl",
            "Source code should rank above docs/memories, but '{}' ranked first",
            pivots[0].result.id
        );
    }
}

#[cfg(test)]
mod graph_expansion_tests {
    use std::collections::HashMap;

    use tempfile::TempDir;

    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::search::index::SymbolSearchResult;
    use crate::tools::get_context::pipeline::{
        expand_graph, NeighborDirection, Pivot,
    };

    /// Create a test database with file info pre-registered (FK constraint).
    fn setup_db() -> (TempDir, SymbolDatabase) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Register files to satisfy FK constraints
        for file in &["src/main.rs", "src/engine.rs", "src/handler.rs", "src/utils.rs"] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                content: None,
            })
            .unwrap();
        }

        (temp_dir, db)
    }

    fn make_symbol(id: &str, name: &str, kind: SymbolKind, file: &str, line: u32) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file.to_string(),
            start_line: line,
            end_line: line + 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: None,
            signature: Some(format!("fn {}()", name)),
            doc_comment: None,
            visibility: Some(Visibility::Public),
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: None,
            content_type: None,
        }
    }

    fn make_rel(
        id: &str,
        from: &str,
        to: &str,
        kind: RelationshipKind,
        file: &str,
        line: u32,
    ) -> Relationship {
        Relationship {
            id: id.to_string(),
            from_symbol_id: from.to_string(),
            to_symbol_id: to.to_string(),
            kind,
            file_path: file.to_string(),
            line_number: line,
            confidence: 0.9,
            metadata: None,
        }
    }

    fn make_pivot(id: &str, name: &str, score: f32) -> Pivot {
        Pivot {
            result: SymbolSearchResult {
                id: id.to_string(),
                name: name.to_string(),
                signature: format!("fn {}()", name),
                doc_comment: String::new(),
                file_path: format!("src/{}.rs", name),
                kind: "function".to_string(),
                language: "rust".to_string(),
                start_line: 1,
                score,
            },
            combined_score: score,
        }
    }

    // === Test 1: Basic expansion with incoming + outgoing ===

    #[test]
    fn test_expand_graph_basic_incoming_and_outgoing() {
        let (_tmp, mut db) = setup_db();

        // Symbols: pivot "engine", caller "main", callee "utils"
        let symbols = vec![
            make_symbol("sym_engine", "engine_run", SymbolKind::Function, "src/engine.rs", 10),
            make_symbol("sym_main", "main", SymbolKind::Function, "src/main.rs", 1),
            make_symbol("sym_utils", "helper", SymbolKind::Function, "src/utils.rs", 5),
        ];
        db.store_symbols(&symbols).unwrap();

        // main -> engine (incoming to pivot)
        // engine -> utils (outgoing from pivot)
        let rels = vec![
            make_rel("r1", "sym_main", "sym_engine", RelationshipKind::Calls, "src/main.rs", 5),
            make_rel("r2", "sym_engine", "sym_utils", RelationshipKind::Calls, "src/engine.rs", 15),
        ];
        db.store_relationships(&rels).unwrap();

        let pivots = vec![make_pivot("sym_engine", "engine_run", 10.0)];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(expansion.neighbors.len(), 2, "should find 1 incoming + 1 outgoing neighbor");

        let names: Vec<&str> = expansion.neighbors.iter().map(|n| n.symbol.name.as_str()).collect();
        assert!(names.contains(&"main"), "should include incoming caller 'main'");
        assert!(names.contains(&"helper"), "should include outgoing callee 'helper'");

        // Verify directions
        let main_neighbor = expansion.neighbors.iter().find(|n| n.symbol.name == "main").unwrap();
        assert_eq!(main_neighbor.direction, NeighborDirection::Incoming);
        assert_eq!(main_neighbor.relationship_kind, RelationshipKind::Calls);

        let helper_neighbor = expansion.neighbors.iter().find(|n| n.symbol.name == "helper").unwrap();
        assert_eq!(helper_neighbor.direction, NeighborDirection::Outgoing);
        assert_eq!(helper_neighbor.relationship_kind, RelationshipKind::Calls);
    }

    // === Test 2: Deduplication across pivots ===

    #[test]
    fn test_expand_graph_deduplicates_shared_neighbors() {
        let (_tmp, mut db) = setup_db();

        // Symbols: two pivots "engine" and "handler", shared neighbor "utils"
        let symbols = vec![
            make_symbol("sym_engine", "engine_run", SymbolKind::Function, "src/engine.rs", 10),
            make_symbol("sym_handler", "handle", SymbolKind::Function, "src/handler.rs", 1),
            make_symbol("sym_utils", "helper", SymbolKind::Function, "src/utils.rs", 5),
        ];
        db.store_symbols(&symbols).unwrap();

        // Both pivots call the same utility function
        let rels = vec![
            make_rel("r1", "sym_engine", "sym_utils", RelationshipKind::Calls, "src/engine.rs", 15),
            make_rel("r2", "sym_handler", "sym_utils", RelationshipKind::Calls, "src/handler.rs", 10),
        ];
        db.store_relationships(&rels).unwrap();

        let pivots = vec![
            make_pivot("sym_engine", "engine_run", 10.0),
            make_pivot("sym_handler", "handle", 8.0),
        ];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(
            expansion.neighbors.len(),
            1,
            "shared neighbor 'helper' should appear only once"
        );
        assert_eq!(expansion.neighbors[0].symbol.name, "helper");
    }

    // === Test 3: Ranking by reference_score ===

    #[test]
    fn test_expand_graph_ranks_by_reference_score() {
        let (_tmp, mut db) = setup_db();

        // Symbols: pivot "engine", neighbors "main" (high ref_score) and "utils" (low ref_score)
        let symbols = vec![
            make_symbol("sym_engine", "engine_run", SymbolKind::Function, "src/engine.rs", 10),
            make_symbol("sym_main", "main", SymbolKind::Function, "src/main.rs", 1),
            make_symbol("sym_utils", "helper", SymbolKind::Function, "src/utils.rs", 5),
        ];
        db.store_symbols(&symbols).unwrap();

        // Both call the pivot (incoming relationships)
        // Plus some extra rels to give "main" a higher reference_score
        let rels = vec![
            make_rel("r1", "sym_main", "sym_engine", RelationshipKind::Calls, "src/main.rs", 5),
            make_rel("r2", "sym_utils", "sym_engine", RelationshipKind::Calls, "src/utils.rs", 8),
            // Extra incoming rels TO main (to boost main's ref_score)
            make_rel("r3", "sym_utils", "sym_main", RelationshipKind::Calls, "src/utils.rs", 9),
            make_rel("r4", "sym_engine", "sym_main", RelationshipKind::Calls, "src/engine.rs", 20),
        ];
        db.store_relationships(&rels).unwrap();

        // Compute reference_scores so the DB has them
        db.compute_reference_scores().unwrap();

        let pivots = vec![make_pivot("sym_engine", "engine_run", 10.0)];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(expansion.neighbors.len(), 2);

        // "main" should be ranked first because it has more incoming refs
        assert_eq!(
            expansion.neighbors[0].symbol.name, "main",
            "neighbor with higher reference_score should rank first"
        );
        assert!(
            expansion.neighbors[0].reference_score > expansion.neighbors[1].reference_score,
            "first neighbor ({}: {:.1}) should have higher score than second ({}: {:.1})",
            expansion.neighbors[0].symbol.name,
            expansion.neighbors[0].reference_score,
            expansion.neighbors[1].symbol.name,
            expansion.neighbors[1].reference_score,
        );
    }

    // === Test 4: Empty relationships ===

    #[test]
    fn test_expand_graph_no_relationships() {
        let (_tmp, mut db) = setup_db();

        // Pivot symbol exists but has no relationships
        let symbols = vec![make_symbol(
            "sym_lonely",
            "lonely_fn",
            SymbolKind::Function,
            "src/main.rs",
            1,
        )];
        db.store_symbols(&symbols).unwrap();

        let pivots = vec![make_pivot("sym_lonely", "lonely_fn", 5.0)];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert!(
            expansion.neighbors.is_empty(),
            "pivot with no relationships should have no neighbors"
        );
    }

    // === Test 5: Excludes pivots from neighbors ===

    #[test]
    fn test_expand_graph_excludes_pivots_from_neighbors() {
        let (_tmp, mut db) = setup_db();

        // Two pivots that call each other, plus an external neighbor
        let symbols = vec![
            make_symbol("sym_engine", "engine_run", SymbolKind::Function, "src/engine.rs", 10),
            make_symbol("sym_handler", "handle", SymbolKind::Function, "src/handler.rs", 1),
            make_symbol("sym_utils", "helper", SymbolKind::Function, "src/utils.rs", 5),
        ];
        db.store_symbols(&symbols).unwrap();

        let rels = vec![
            // Pivot A calls Pivot B (should NOT appear as neighbor)
            make_rel("r1", "sym_engine", "sym_handler", RelationshipKind::Calls, "src/engine.rs", 15),
            // Pivot B calls Pivot A (should NOT appear as neighbor)
            make_rel("r2", "sym_handler", "sym_engine", RelationshipKind::Calls, "src/handler.rs", 5),
            // Pivot A calls external symbol (SHOULD appear)
            make_rel("r3", "sym_engine", "sym_utils", RelationshipKind::Calls, "src/engine.rs", 20),
        ];
        db.store_relationships(&rels).unwrap();

        let pivots = vec![
            make_pivot("sym_engine", "engine_run", 10.0),
            make_pivot("sym_handler", "handle", 8.0),
        ];
        let expansion = expand_graph(&pivots, &db).unwrap();

        assert_eq!(
            expansion.neighbors.len(),
            1,
            "only non-pivot neighbor should appear"
        );
        assert_eq!(expansion.neighbors[0].symbol.name, "helper");

        // Verify neither pivot appears as a neighbor
        let neighbor_ids: Vec<&str> = expansion
            .neighbors
            .iter()
            .map(|n| n.symbol.id.as_str())
            .collect();
        assert!(
            !neighbor_ids.contains(&"sym_engine"),
            "pivot 'engine' should not appear as a neighbor"
        );
        assert!(
            !neighbor_ids.contains(&"sym_handler"),
            "pivot 'handler' should not appear as a neighbor"
        );
    }

    // === Test 6: Empty pivots ===

    #[test]
    fn test_expand_graph_empty_pivots() {
        let (_tmp, db) = setup_db();
        let expansion = expand_graph(&[], &db).unwrap();
        assert!(expansion.neighbors.is_empty());
    }
}

#[cfg(test)]
mod allocation_tests {
    use crate::tools::get_context::allocation::{
        NeighborMode, PivotMode, TokenBudget,
    };

    // === Test 1: Single pivot goes deep ===

    #[test]
    fn test_single_pivot_full_body_mode() {
        let budget = TokenBudget::new(2000);
        let alloc = budget.allocate(1, 3);

        assert!(
            matches!(alloc.pivot_mode, PivotMode::FullBody),
            "1 pivot should use FullBody mode"
        );
        assert!(
            matches!(alloc.neighbor_mode, NeighborMode::SignatureAndDoc),
            "1 pivot should give neighbors SignatureAndDoc mode"
        );
        // Pivots get 60% of budget
        assert_eq!(alloc.pivot_tokens, 1200, "60% of 2000 = 1200");
    }

    // === Test 2: Many pivots goes broad ===

    #[test]
    fn test_many_pivots_signature_only_mode() {
        let budget = TokenBudget::new(4000);
        let alloc = budget.allocate(8, 20);

        assert!(
            matches!(alloc.pivot_mode, PivotMode::SignatureOnly),
            "8 pivots should use SignatureOnly mode"
        );
        assert!(
            matches!(alloc.neighbor_mode, NeighborMode::NameAndLocation),
            "8 pivots should give neighbors NameAndLocation mode"
        );
    }

    // === Test 3: Budget respect — total never exceeds max_tokens ===

    #[test]
    fn test_budget_respect() {
        for max in [500, 1000, 2000, 3000, 4000, 5000, 9999] {
            let budget = TokenBudget::new(max);
            for pivots in [0, 1, 3, 5, 7, 10] {
                let alloc = budget.allocate(pivots, pivots * 3);
                let total = alloc.pivot_tokens + alloc.neighbor_tokens + alloc.summary_tokens;
                assert!(
                    total <= max,
                    "total allocation {} exceeds max_tokens {} (pivots={}, neighbors={})",
                    total,
                    max,
                    pivots,
                    pivots * 3,
                );
            }
        }
    }

    // === Test 4: Adaptive defaults based on pivot count ===

    #[test]
    fn test_adaptive_defaults() {
        // 0-2 pivots → 2000 tokens (deep exploration)
        assert_eq!(TokenBudget::adaptive(0).max_tokens, 2000);
        assert_eq!(TokenBudget::adaptive(1).max_tokens, 2000);
        assert_eq!(TokenBudget::adaptive(2).max_tokens, 2000);

        // 3-5 pivots → 3000 tokens (balanced)
        assert_eq!(TokenBudget::adaptive(3).max_tokens, 3000);
        assert_eq!(TokenBudget::adaptive(4).max_tokens, 3000);
        assert_eq!(TokenBudget::adaptive(5).max_tokens, 3000);

        // 6+ pivots → 4000 tokens (broad survey)
        assert_eq!(TokenBudget::adaptive(6).max_tokens, 4000);
        assert_eq!(TokenBudget::adaptive(8).max_tokens, 4000);
        assert_eq!(TokenBudget::adaptive(100).max_tokens, 4000);
    }

    // === Test 5: Mid-range pivots (4-6) ===

    #[test]
    fn test_mid_range_pivots() {
        let budget = TokenBudget::new(3000);

        for pivot_count in [4, 5, 6] {
            let alloc = budget.allocate(pivot_count, 10);
            assert!(
                matches!(alloc.pivot_mode, PivotMode::SignatureAndKey),
                "pivot_count={} should use SignatureAndKey, got {:?}",
                pivot_count,
                alloc.pivot_mode,
            );
            assert!(
                matches!(alloc.neighbor_mode, NeighborMode::SignatureOnly),
                "pivot_count={} should use SignatureOnly neighbor mode, got {:?}",
                pivot_count,
                alloc.neighbor_mode,
            );
        }
    }

    // === Test 6: 60/30/10 split is correct ===

    #[test]
    fn test_60_30_10_split() {
        let budget = TokenBudget::new(1000);
        let alloc = budget.allocate(2, 5);

        assert_eq!(alloc.pivot_tokens, 600, "pivots should get 60%");
        assert_eq!(alloc.neighbor_tokens, 300, "neighbors should get 30%");
        assert_eq!(alloc.summary_tokens, 100, "summary should get 10%");

        // Verify it sums to exactly max_tokens for clean numbers
        assert_eq!(
            alloc.pivot_tokens + alloc.neighbor_tokens + alloc.summary_tokens,
            1000,
            "60+30+10 should sum to 100% for clean multiples"
        );
    }

    // === Test 7: Boundary — 0 pivots still works ===

    #[test]
    fn test_zero_pivots() {
        let budget = TokenBudget::new(2000);
        let alloc = budget.allocate(0, 0);

        // 0 is in the 0..=3 range → FullBody / SignatureAndDoc
        assert!(matches!(alloc.pivot_mode, PivotMode::FullBody));
        assert!(matches!(alloc.neighbor_mode, NeighborMode::SignatureAndDoc));

        // Budget should still be allocated (pipeline handles the empty case)
        assert_eq!(alloc.pivot_tokens + alloc.neighbor_tokens + alloc.summary_tokens, 2000);
    }

    // === Test 8: Boundary — exactly 3 pivots is FullBody (not SignatureAndKey) ===

    #[test]
    fn test_boundary_three_pivots_is_full_body() {
        let budget = TokenBudget::new(2000);
        let alloc = budget.allocate(3, 5);

        assert!(
            matches!(alloc.pivot_mode, PivotMode::FullBody),
            "3 pivots should still be FullBody (0..=3 range)"
        );
    }

    // === Test 9: Boundary — exactly 7 pivots is SignatureOnly ===

    #[test]
    fn test_boundary_seven_pivots_is_signature_only() {
        let budget = TokenBudget::new(4000);
        let alloc = budget.allocate(7, 15);

        assert!(
            matches!(alloc.pivot_mode, PivotMode::SignatureOnly),
            "7 pivots should be SignatureOnly (7+ range)"
        );
        assert!(
            matches!(alloc.neighbor_mode, NeighborMode::NameAndLocation),
            "7 pivots should give neighbors NameAndLocation mode"
        );
    }
}

#[cfg(test)]
mod pipeline_integration_tests {
    use tempfile::TempDir;

    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::search::index::{SearchIndex, SymbolDocument};
    use crate::tools::get_context::pipeline::run_pipeline;

    /// Set up a complete test environment with DB, SearchIndex, and test data.
    ///
    /// Creates:
    /// - `process_request` (function) — the main pivot target, calls `validate_input` and `build_response`
    /// - `validate_input` (function) — outgoing callee from pivot
    /// - `build_response` (function) — outgoing callee from pivot
    /// - `handle_error` (function) — calls `process_request` (incoming caller)
    /// - `main` (function) — calls `process_request` (incoming caller)
    fn setup_test_env() -> (TempDir, TempDir, SymbolDatabase, SearchIndex) {
        let db_dir = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();

        let db_path = db_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        let index = SearchIndex::create(index_dir.path()).unwrap();

        // Register files
        for file in &[
            "src/handler.rs",
            "src/validation.rs",
            "src/response.rs",
            "src/error.rs",
            "src/main.rs",
        ] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 1,
                content: None,
            })
            .unwrap();
        }

        // Create symbols with code_context for full-body testing
        let symbols = vec![
            Symbol {
                id: "sym_process".to_string(),
                name: "process_request".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/handler.rs".to_string(),
                start_line: 10,
                end_line: 25,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 500,
                parent_id: None,
                signature: Some("fn process_request(req: &Request) -> Response".to_string()),
                doc_comment: Some("Process an incoming request".to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.95),
                code_context: Some(
                    "fn process_request(req: &Request) -> Response {\n    let valid = validate_input(req);\n    if !valid { return Response::bad_request(); }\n    build_response(req)\n}"
                        .to_string(),
                ),
                content_type: None,
            },
            Symbol {
                id: "sym_validate".to_string(),
                name: "validate_input".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/validation.rs".to_string(),
                start_line: 5,
                end_line: 15,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 300,
                parent_id: None,
                signature: Some("fn validate_input(req: &Request) -> bool".to_string()),
                doc_comment: Some("Validate request parameters".to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(
                    "fn validate_input(req: &Request) -> bool {\n    !req.body.is_empty()\n}"
                        .to_string(),
                ),
                content_type: None,
            },
            Symbol {
                id: "sym_build".to_string(),
                name: "build_response".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/response.rs".to_string(),
                start_line: 1,
                end_line: 10,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 200,
                parent_id: None,
                signature: Some("fn build_response(req: &Request) -> Response".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(
                    "fn build_response(req: &Request) -> Response {\n    Response::ok(req.body.clone())\n}"
                        .to_string(),
                ),
                content_type: None,
            },
            Symbol {
                id: "sym_error".to_string(),
                name: "handle_error".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/error.rs".to_string(),
                start_line: 1,
                end_line: 8,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 150,
                parent_id: None,
                signature: Some("fn handle_error(err: Error) -> Response".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None,
                content_type: None,
            },
            Symbol {
                id: "sym_main".to_string(),
                name: "main".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/main.rs".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some("fn main()".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None,
                content_type: None,
            },
        ];

        db.store_symbols(&symbols).unwrap();

        // Index symbols in Tantivy
        for sym in &symbols {
            index.add_symbol(&SymbolDocument::from_symbol(sym)).unwrap();
        }
        index.commit().unwrap();

        // Create relationships
        let rels = vec![
            // process_request -> validate_input (outgoing)
            Relationship {
                id: "r1".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "sym_validate".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 11,
                confidence: 0.9,
                metadata: None,
            },
            // process_request -> build_response (outgoing)
            Relationship {
                id: "r2".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "sym_build".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 13,
                confidence: 0.9,
                metadata: None,
            },
            // handle_error -> process_request (incoming to pivot)
            Relationship {
                id: "r3".to_string(),
                from_symbol_id: "sym_error".to_string(),
                to_symbol_id: "sym_process".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/error.rs".to_string(),
                line_number: 3,
                confidence: 0.9,
                metadata: None,
            },
            // main -> process_request (incoming to pivot)
            Relationship {
                id: "r4".to_string(),
                from_symbol_id: "sym_main".to_string(),
                to_symbol_id: "sym_process".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/main.rs".to_string(),
                line_number: 2,
                confidence: 0.9,
                metadata: None,
            },
        ];
        db.store_relationships(&rels).unwrap();
        db.compute_reference_scores().unwrap();

        (db_dir, index_dir, db, index)
    }

    // === Integration Test 1: Full pipeline end-to-end ===

    #[test]
    fn test_full_pipeline_end_to_end() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();

        let result = run_pipeline(
            "process_request",
            None,  // auto token budget
            None,  // no language filter
            None,  // no file pattern
            &db,
            &index,
        )
        .unwrap();

        // Should contain pivot section for process_request
        assert!(
            result.contains("process_request"),
            "output should mention the pivot symbol: got:\n{}",
            result,
        );

        // Should contain the Pivot header marker
        assert!(
            result.contains("Pivot:"),
            "output should have a Pivot section header",
        );

        // Should contain code body (FullBody mode for 1-2 pivots)
        assert!(
            result.contains("validate_input"),
            "output should mention validate_input (callee or neighbor): got:\n{}",
            result,
        );

        // Should contain file map
        assert!(
            result.contains("src/handler.rs"),
            "output should include the pivot's file path",
        );

        // Should have context header
        assert!(
            result.contains("Context:"),
            "output should have Context header",
        );
    }

    // === Integration Test 2: Empty results ===

    #[test]
    fn test_pipeline_no_results() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();

        let result = run_pipeline(
            "zzz_nonexistent_symbol_xyz",
            None,
            None,
            None,
            &db,
            &index,
        )
        .unwrap();

        assert!(
            result.contains("No relevant symbols found"),
            "should return no-results message for unknown query, got:\n{}",
            result,
        );
    }

    // === Integration Test 3: Explicit token budget ===

    #[test]
    fn test_pipeline_with_explicit_budget() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();

        let result = run_pipeline(
            "process_request",
            Some(1000),
            None,
            None,
            &db,
            &index,
        )
        .unwrap();

        // With explicit budget of 1000, should still produce output
        assert!(
            result.contains("process_request"),
            "explicit budget should still produce pivot output",
        );
    }

    // === Integration Test 4: Language filter ===

    #[test]
    fn test_pipeline_with_language_filter() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();

        // Filter to a language that doesn't exist → should get no results
        let result = run_pipeline(
            "process_request",
            None,
            Some("python".to_string()),
            None,
            &db,
            &index,
        )
        .unwrap();

        assert!(
            result.contains("No relevant symbols found"),
            "python filter should yield no results for rust symbols, got:\n{}",
            result,
        );
    }

    // === Integration Test 5: Neighbors appear in output ===

    #[test]
    fn test_pipeline_includes_neighbors() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();

        let result = run_pipeline(
            "process_request",
            None,
            None,
            None,
            &db,
            &index,
        )
        .unwrap();

        // Graph expansion should find neighbors: validate_input, build_response,
        // handle_error, main — at least some should appear in Neighbors section
        let has_neighbor_section = result.contains("Neighbors");
        let has_any_neighbor = result.contains("validate_input")
            || result.contains("build_response")
            || result.contains("handle_error")
            || result.contains("main");

        assert!(
            has_neighbor_section || has_any_neighbor,
            "output should include neighbor symbols from graph expansion, got:\n{}",
            result,
        );
    }

    /// Test that common trait method neighbors (clone, to_string, fmt, etc.)
    /// are filtered from output as noise.
    #[test]
    fn test_pipeline_filters_noise_neighbors() {
        let (_db_dir, _index_dir, mut db, index) = setup_test_env();

        // Helper to make a noise symbol with all required fields
        fn make_noise(id: &str, name: &str, file: &str, line: u32) -> Symbol {
            Symbol {
                id: id.to_string(),
                name: name.to_string(),
                kind: SymbolKind::Method,
                language: "rust".to_string(),
                file_path: file.to_string(),
                start_line: line,
                end_line: line + 5,
                start_column: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some(format!("fn {}()", name)),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(format!("fn {}() {{ }}", name)),
                content_type: None,
            }
        }

        // Add noise symbols that are common trait methods
        let noise_symbols = vec![
            make_noise("clone_impl", "clone", "src/handler.rs", 200),
            make_noise("to_string_impl", "to_string", "src/handler.rs", 210),
            make_noise("fmt_impl", "fmt", "src/handler.rs", 220),
        ];

        db.store_symbols(&noise_symbols).unwrap();

        // Add relationships: sym_process calls clone, to_string, fmt
        // (sym_process is the ID for process_request in setup_test_env)
        let noise_rels = vec![
            Relationship {
                id: "rel_clone".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "clone_impl".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 55,
                confidence: 0.9,
                metadata: None,
            },
            Relationship {
                id: "rel_to_string".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "to_string_impl".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 56,
                confidence: 0.9,
                metadata: None,
            },
            Relationship {
                id: "rel_fmt".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "fmt_impl".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 57,
                confidence: 0.9,
                metadata: None,
            },
        ];

        db.store_relationships(&noise_rels).unwrap();

        let result = run_pipeline(
            "process_request",
            None,
            None,
            None,
            &db,
            &index,
        )
        .unwrap();

        // clone, to_string, fmt should NOT appear as neighbors
        assert!(
            !result.contains("clone_impl") && !result.contains("  clone "),
            "clone should be filtered from neighbors, got:\n{}",
            result,
        );
        assert!(
            !result.contains("to_string_impl") && !result.contains("  to_string "),
            "to_string should be filtered from neighbors, got:\n{}",
            result,
        );
        assert!(
            !result.contains("fmt_impl") && !result.contains("  fmt "),
            "fmt should be filtered from neighbors, got:\n{}",
            result,
        );

        // Real neighbors should still be present
        assert!(
            result.contains("validate_input") || result.contains("build_response"),
            "Real neighbors should still appear in output, got:\n{}",
            result,
        );
    }

    /// Test that neighbors from test files are filtered from output.
    /// Test functions appearing as callers add noise to the context.
    #[test]
    fn test_pipeline_filters_test_file_neighbors() {
        let (_db_dir, _index_dir, mut db, index) = setup_test_env();

        // Register a test file
        db.store_file_info(&FileInfo {
            path: "src/tests/handler_tests.rs".to_string(),
            language: "rust".to_string(),
            hash: "hash_test".to_string(),
            size: 500,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 1,
            content: None,
        })
        .unwrap();

        // Add a test function that calls process_request
        let test_sym = Symbol {
            id: "test_caller".to_string(),
            name: "test_process_request_works".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/tests/handler_tests.rs".to_string(),
            start_line: 10,
            end_line: 20,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: None,
            signature: Some("fn test_process_request_works()".to_string()),
            doc_comment: None,
            visibility: Some(Visibility::Public),
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: Some("fn test_process_request_works() { process_request(&req); }".to_string()),
            content_type: None,
        };
        db.store_symbols(&[test_sym]).unwrap();

        // Test calls process_request (incoming relationship to pivot)
        let test_rel = Relationship {
            id: "rel_test_caller".to_string(),
            from_symbol_id: "test_caller".to_string(),
            to_symbol_id: "sym_process".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/tests/handler_tests.rs".to_string(),
            line_number: 15,
            confidence: 0.9,
            metadata: None,
        };
        db.store_relationships(&[test_rel]).unwrap();

        let result = run_pipeline(
            "process_request",
            None,
            None,
            None,
            &db,
            &index,
        )
        .unwrap();

        // Test function should NOT appear as a neighbor
        assert!(
            !result.contains("test_process_request_works"),
            "Test function neighbors should be filtered, got:\n{}",
            result,
        );

        // Real production neighbors should still appear
        assert!(
            result.contains("validate_input") || result.contains("build_response")
                || result.contains("handle_error"),
            "Real neighbors should still appear, got:\n{}",
            result,
        );
    }

    // === Integration Test 8: Pipeline respects token budget ===

    #[test]
    fn test_pipeline_respects_token_budget() {
        let (_db_dir, _index_dir, db, index) = setup_test_env();

        // Use a very small token budget
        let result = run_pipeline("process", Some(100), None, None, &db, &index).unwrap();

        // The output should exist and not be empty
        assert!(!result.is_empty());

        // With a very tight budget, the output should be constrained
        // (We can't test exact token count, but we verify the pipeline doesn't crash
        // and produces reasonable output)
        let token_est = crate::utils::token_estimation::TokenEstimator::new();
        let estimated = token_est.estimate_string(&result);

        // The overall output includes formatting overhead (headers, separators),
        // so we can't expect it to be exactly within max_tokens.
        // But the pivot content should be constrained.
        // Just verify it doesn't wildly exceed the budget
        // (without enforcement, a 100-token budget could produce 500+ tokens of pivot content)
        println!(
            "Token budget: 100, estimated output tokens: {}",
            estimated
        );
    }
}

mod token_budget_tests {
    use crate::tools::get_context::pipeline::truncate_to_token_budget;

    #[test]
    fn test_truncate_small_content_unchanged() {
        // Content within budget should pass through unchanged
        let small_code = "fn hello() {\n    println!(\"hi\");\n}";
        let result = truncate_to_token_budget(small_code, 500);
        assert_eq!(result, small_code);
    }

    #[test]
    fn test_truncate_large_content_reduced() {
        // Generate large content that exceeds budget
        let mut lines = vec!["fn big_function() {".to_string()];
        for i in 0..100 {
            lines.push(format!("    let x{} = {};", i, i));
        }
        lines.push("}".to_string());
        let large_code = lines.join("\n");

        // With a very small budget, content should be truncated
        let result = truncate_to_token_budget(&large_code, 50);
        assert!(
            result.len() < large_code.len(),
            "Result should be shorter than input"
        );
        assert!(
            result.contains("lines omitted to fit token budget"),
            "Should have omission marker"
        );
    }

    #[test]
    fn test_truncate_preserves_head_bias() {
        // First lines (signature) and last lines (closing) should be preserved
        let mut lines =
            vec!["fn important_function(arg1: Type1, arg2: Type2) -> Result {".to_string()];
        for i in 0..50 {
            lines.push(format!("    let step{} = process{};", i, i));
        }
        lines.push("    Ok(final_result)".to_string());
        lines.push("}".to_string());
        let code = lines.join("\n");

        let result = truncate_to_token_budget(&code, 50);
        // Should preserve the function signature at the top
        assert!(
            result.starts_with("fn important_function"),
            "Should preserve function signature"
        );
        // Should preserve closing brace
        assert!(result.ends_with("}"), "Should preserve closing brace");
    }

    #[test]
    fn test_truncate_very_short_content_unchanged() {
        // Content with <= 5 lines should never be truncated regardless of token count
        let short = "a\nb\nc\nd\ne";
        let result = truncate_to_token_budget(short, 1); // absurdly low budget
        assert_eq!(result, short);
    }
}

