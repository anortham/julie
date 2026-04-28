//! Tests for get_context relevance guardrails in pipeline selection.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::search::index::SymbolSearchResult;
    use crate::tools::get_context::scoring::select_pivots_with_code_fallback;

    fn make_result(
        id: &str,
        name: &str,
        kind: &str,
        file_path: &str,
        score: f32,
    ) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: format!("fn {}()", name),
            doc_comment: String::new(),
            file_path: file_path.to_string(),
            kind: kind.to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_fallback_prefers_code_when_initial_pivots_are_non_code() {
        let results = vec![
            make_result(
                "doc_1",
                "workspace_routing_reference_workspace",
                "module",
                "docs/workspace-routing.md",
                12.0,
            ),
            make_result(
                "memory_1",
                "workspace_routing_checkpoint",
                "module",
                ".memories/2026-02-25/checkpoint.md",
                11.0,
            ),
            make_result(
                "code_1",
                "resolve_workspace_filter",
                "function",
                "src/tools/navigation/resolution.rs",
                6.0,
            ),
        ];

        let pivots = select_pivots_with_code_fallback(results, &HashMap::new());

        assert!(!pivots.is_empty(), "expected at least one pivot");
        assert_eq!(
            pivots[0].result.id, "code_1",
            "fallback should prioritize code symbol when initial top pivots are docs/memories"
        );
    }

    #[test]
    fn test_fallback_does_not_trigger_when_primary_has_actionable_pivot() {
        let results = vec![
            make_result(
                "doc_1",
                "workspace_routing_reference_workspace",
                "function",
                "docs/workspace-routing.md",
                10.0,
            ),
            make_result(
                "code_1",
                "resolve_workspace_filter",
                "function",
                "src/tools/navigation/resolution.rs",
                9.0,
            ),
            make_result(
                "memory_1",
                "workspace_routing_checkpoint",
                "module",
                ".memories/2026-02-25/checkpoint.md",
                1.0,
            ),
        ];

        let mut ref_scores = HashMap::new();
        // Strong centrality can still surface docs in primary ranking.
        // This ensures the selected pivot set includes both doc + code candidates.
        ref_scores.insert("doc_1".to_string(), 100_000_000.0);

        let pivots = select_pivots_with_code_fallback(results, &ref_scores);

        assert!(pivots.len() >= 2, "expected top-2 pivots in default path");
        assert_eq!(
            pivots[0].result.id, "doc_1",
            "when an actionable pivot is already present, keep primary ranking"
        );
        assert!(
            pivots.iter().any(|p| p.result.id == "code_1"),
            "expected actionable pivot to remain in selected set"
        );
    }

    #[test]
    fn test_select_pivots_tie_breaker_is_deterministic() {
        let results = vec![
            make_result(
                "z_id",
                "zeta_handler",
                "function",
                "src/handler/zeta.rs",
                5.0,
            ),
            make_result(
                "a_id",
                "alpha_handler",
                "function",
                "src/handler/alpha.rs",
                5.0,
            ),
        ];

        let pivots = select_pivots_with_code_fallback(results, &HashMap::new());

        assert_eq!(pivots.len(), 2);
        assert_eq!(
            pivots[0].result.name, "alpha_handler",
            "equal-score pivots should use stable lexical tie-break"
        );
        assert_eq!(pivots[1].result.name, "zeta_handler");
    }

    #[test]
    fn test_fallback_triggers_when_actionable_coverage_is_too_low() {
        let results = vec![
            make_result(
                "doc_1",
                "workspace_routing_overview",
                "function",
                "docs/workspace-routing.md",
                10.0,
            ),
            make_result(
                "doc_2",
                "workspace_routing_checkpoint",
                "function",
                ".memories/2026-02-25/checkpoint.md",
                9.0,
            ),
            make_result(
                "code_1",
                "resolve_workspace_routing",
                "function",
                "src/workspace/resolver.rs",
                8.0,
            ),
            make_result(
                "code_2",
                "validate_workspace_routing",
                "function",
                "src/workspace/validator.rs",
                7.0,
            ),
        ];

        let mut ref_scores = HashMap::new();
        // Make docs win primary ranking despite non-code penalty.
        ref_scores.insert("doc_1".to_string(), 100_000_000.0);
        ref_scores.insert("doc_2".to_string(), 100_000_000.0);

        let pivots = select_pivots_with_code_fallback(results, &ref_scores);

        assert_eq!(
            pivots.len(),
            2,
            "low actionable coverage should trigger code-only fallback"
        );
        assert_eq!(pivots[0].result.id, "code_1");
        assert_eq!(pivots[1].result.id, "code_2");
    }

    #[test]
    fn test_fallback_keeps_single_code_candidate_when_primary_is_non_actionable() {
        let results = vec![
            make_result(
                "doc_1",
                "workspace_routing_overview",
                "function",
                "docs/workspace-routing.md",
                10.0,
            ),
            make_result(
                "memory_1",
                "workspace_routing_checkpoint",
                "module",
                ".memories/2026-02-25/checkpoint.md",
                9.0,
            ),
            make_result(
                "code_1",
                "resolve_workspace_routing",
                "function",
                "src/workspace/resolver.rs",
                2.0,
            ),
        ];

        let mut ref_scores = HashMap::new();
        ref_scores.insert("doc_1".to_string(), 100_000_000.0);

        let pivots = select_pivots_with_code_fallback(results, &ref_scores);

        assert_eq!(
            pivots.len(),
            1,
            "a single actionable code candidate is enough fallback material"
        );
        assert_eq!(pivots[0].result.id, "code_1");
    }

    #[test]
    fn test_select_pivots_deboosts_auxiliary_paths_vs_src_code() {
        let results = vec![
            make_result(
                "example_1",
                "workspace_routing_example",
                "function",
                "examples/workspace_routing_demo.rs",
                10.0,
            ),
            make_result(
                "src_1",
                "resolve_workspace_filter",
                "function",
                "src/tools/navigation/resolution.rs",
                6.0,
            ),
        ];

        let pivots = select_pivots_with_code_fallback(results, &HashMap::new());

        assert_eq!(
            pivots[0].result.id, "src_1",
            "src production code should rank above auxiliary examples path"
        );
    }
}
