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
