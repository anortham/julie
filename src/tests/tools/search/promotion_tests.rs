//! Tests for the composite `SearchExecutionKind::Promoted` variant plus the
//! new `HintKind` and `ZeroHitReason` trace enums introduced in the search
//! quality hardening plan.
//!
//! These tests are shared between Task 6 (trace additions) and Task 7 / Task 8
//! (content→definitions auto-promotion, multi-token hint formatter).

#[cfg(test)]
mod tests {
    use crate::tools::search::hint_formatter::{
        build_multi_token_zero_hit_hint, is_multi_token_query, tokenize_query_for_hint,
    };
    use crate::tools::search::trace::{
        HintKind, PromotionInfo, SearchExecutionKind, SearchExecutionResult, SearchTrace,
        ZeroHitReason,
    };

    #[test]
    fn hint_kind_serializes_snake_case() {
        for (variant, expected) in [
            (HintKind::MultiTokenHint, "multi_token_hint"),
            (
                HintKind::OutOfScopeDefinitionHint,
                "out_of_scope_definition_hint",
            ),
            (HintKind::CommaGlobHint, "comma_glob_hint"),
        ] {
            let json = serde_json::to_value(&variant).expect("serialize hint kind");
            assert_eq!(
                json,
                serde_json::Value::String(expected.to_string()),
                "HintKind::{:?} should serialize as {:?}",
                variant,
                expected
            );
        }
    }

    #[test]
    fn zero_hit_reason_serializes_snake_case() {
        for (variant, expected) in [
            (ZeroHitReason::TantivyNoCandidates, "tantivy_no_candidates"),
            (ZeroHitReason::FilePatternFiltered, "file_pattern_filtered"),
            (ZeroHitReason::LanguageFiltered, "language_filtered"),
            (ZeroHitReason::TestFiltered, "test_filtered"),
            (
                ZeroHitReason::FileContentUnavailable,
                "file_content_unavailable",
            ),
            (ZeroHitReason::LineMatchMiss, "line_match_miss"),
            (ZeroHitReason::Promoted, "promoted"),
        ] {
            let json = serde_json::to_value(&variant).expect("serialize zero hit reason");
            assert_eq!(
                json,
                serde_json::Value::String(expected.to_string()),
                "ZeroHitReason::{:?} should serialize as {:?}",
                variant,
                expected
            );
        }
    }

    #[test]
    fn trace_from_hits_defaults_new_fields_to_none() {
        let trace = SearchTrace::from_hits("fast_search_content", &[]);
        assert!(trace.promoted.is_none());
        assert!(trace.zero_hit_reason.is_none());
        assert!(trace.hint_kind.is_none());
        assert_eq!(trace.result_count, 0);
        assert_eq!(trace.strategy_id, "fast_search_content");
    }

    #[test]
    fn trace_serializes_promoted_zero_hit_and_hint_fields() {
        let mut trace = SearchTrace::from_hits("fast_search_content_promoted", &[]);
        trace.promoted = Some(PromotionInfo {
            requested_target: "content".to_string(),
            effective_target: "definitions".to_string(),
            requested_result_count: 0,
            promotion_reason: "single_identifier_content_zero_hit".to_string(),
        });
        trace.zero_hit_reason = Some(ZeroHitReason::Promoted);
        trace.hint_kind = Some(HintKind::MultiTokenHint);

        let json = serde_json::to_value(&trace).expect("serialize trace");
        assert_eq!(json["strategy_id"], "fast_search_content_promoted");
        assert_eq!(json["result_count"], 0);
        assert_eq!(json["promoted"]["requested_target"], "content");
        assert_eq!(json["promoted"]["effective_target"], "definitions");
        assert_eq!(json["promoted"]["requested_result_count"], 0);
        assert_eq!(
            json["promoted"]["promotion_reason"],
            "single_identifier_content_zero_hit"
        );
        assert_eq!(json["zero_hit_reason"], "promoted");
        assert_eq!(json["hint_kind"], "multi_token_hint");
    }

    #[test]
    fn execution_result_new_promoted_populates_kind_and_trace() {
        let inner_content = SearchExecutionKind::Content {
            workspace_label: Some("primary".to_string()),
            file_level: false,
        };
        let inner_definitions = SearchExecutionKind::Definitions;

        let result = SearchExecutionResult::new_promoted(
            Vec::new(),
            false,
            0,
            "fast_search_content_promoted",
            "content",
            "definitions",
            0,
            "single_identifier_content_zero_hit",
            inner_content,
            inner_definitions,
        );

        match &result.kind {
            SearchExecutionKind::Promoted {
                requested_target,
                effective_target,
                requested_result_count,
                effective_result_count,
                promotion_reason,
                inner_content,
                inner_definitions,
            } => {
                assert_eq!(requested_target, "content");
                assert_eq!(effective_target, "definitions");
                assert_eq!(*requested_result_count, 0);
                assert_eq!(*effective_result_count, 0);
                assert_eq!(promotion_reason, "single_identifier_content_zero_hit");
                assert!(matches!(
                    inner_content.as_ref(),
                    SearchExecutionKind::Content { .. }
                ));
                assert!(matches!(
                    inner_definitions.as_ref(),
                    SearchExecutionKind::Definitions
                ));
            }
            other => panic!("expected Promoted kind, got {:?}", other),
        }

        let info = result
            .trace
            .promoted
            .as_ref()
            .expect("promotion info present on trace");
        assert_eq!(info.requested_target, "content");
        assert_eq!(info.effective_target, "definitions");
        assert_eq!(info.requested_result_count, 0);
        assert_eq!(info.promotion_reason, "single_identifier_content_zero_hit");
    }

    #[test]
    fn existing_callers_of_new_still_compile() {
        // Smoke: the plain `new` constructor still accepts the pre-existing
        // `Definitions` and `Content` variants without migration.
        let definitions_result = SearchExecutionResult::new(
            Vec::new(),
            false,
            0,
            "fast_search_definitions",
            SearchExecutionKind::Definitions,
        );
        assert!(definitions_result.trace.promoted.is_none());
        assert!(definitions_result.trace.zero_hit_reason.is_none());
        assert!(definitions_result.trace.hint_kind.is_none());

        let content_result = SearchExecutionResult::new(
            Vec::new(),
            false,
            0,
            "fast_search_content",
            SearchExecutionKind::Content {
                workspace_label: Some("primary".to_string()),
                file_level: true,
            },
        );
        assert!(content_result.trace.promoted.is_none());
        assert!(content_result.trace.zero_hit_reason.is_none());
        assert!(content_result.trace.hint_kind.is_none());
    }

    // -------------------------------------------------------------------------
    // Task 8: Multi-token content zero-hit hint formatter
    // -------------------------------------------------------------------------

    #[test]
    fn is_multi_token_query_true_for_two_or_more_whitespace_tokens() {
        assert!(is_multi_token_query("retry backoff"));
        assert!(is_multi_token_query("retry backoff jitter"));
        assert!(is_multi_token_query("  error  handling  retry  "));
        assert!(is_multi_token_query("a b"));
    }

    #[test]
    fn is_multi_token_query_false_for_single_token_or_empty() {
        assert!(!is_multi_token_query("retry"));
        assert!(!is_multi_token_query(""));
        assert!(!is_multi_token_query("   "));
        assert!(!is_multi_token_query("CodeTokenizer"));
        // Hyphenated / snake / CamelCase identifiers are still a single
        // whitespace token even though CodeTokenizer will split them further.
        assert!(!is_multi_token_query("delete_orphaned_files_atomic"));
        assert!(!is_multi_token_query("getUserById"));
    }

    #[test]
    fn tokenize_query_for_hint_matches_code_tokenizer_on_simple_words() {
        let tokens = tokenize_query_for_hint("retry backoff jitter");
        // CodeTokenizer lower-cases its output; exact set must include the
        // three input tokens. Order reflects tokenizer traversal.
        assert!(
            tokens.iter().any(|t| t == "retry"),
            "expected 'retry' in tokens: {:?}",
            tokens
        );
        assert!(
            tokens.iter().any(|t| t == "backoff"),
            "expected 'backoff' in tokens: {:?}",
            tokens
        );
        assert!(
            tokens.iter().any(|t| t == "jitter"),
            "expected 'jitter' in tokens: {:?}",
            tokens
        );
    }

    #[test]
    fn tokenize_query_for_hint_deduplicates_repeated_tokens() {
        // "foo foo" should yield one "foo" (deduplicated like index tokenizer).
        let tokens = tokenize_query_for_hint("foo foo");
        let foo_count = tokens.iter().filter(|t| t.as_str() == "foo").count();
        assert_eq!(foo_count, 1, "tokens should be deduplicated, got {:?}", tokens);
    }

    #[test]
    fn multi_token_hint_contains_query_filters_tokens_strategy_and_reason() {
        let hint = build_multi_token_zero_hit_hint(
            "retry backoff jitter",
            Some("src/**/*.rs"),
            Some("rust"),
            Some(true),
            Some(&ZeroHitReason::LineMatchMiss),
        );
        assert!(hint.contains("0 content matches for \"retry backoff jitter\""));
        assert!(hint.contains("file_pattern=src/**/*.rs"));
        assert!(hint.contains("Concept query → try: get_context(query=\"retry backoff jitter\")"));
        // Symbol-lookup suggestion picks the first tokenizer token.
        assert!(
            hint.contains("fast_search(query=\"retry\", search_target=\"definitions\")"),
            "symbol lookup suggestion missing: {}",
            hint
        );
        // CodeTokenizer emits each input token followed by any stem variants
        // that differ (e.g., "retry" → stem "retri"). Assert each input
        // token is present in the Tokens: [...] list rather than pinning the
        // exact order, so stemmer behavior stays implementation-owned.
        let tokens_line = hint
            .lines()
            .find(|l| l.starts_with("Tokens: ["))
            .expect("hint must contain Tokens: line");
        assert!(tokens_line.contains("retry"), "missing retry: {}", tokens_line);
        assert!(tokens_line.contains("backoff"), "missing backoff: {}", tokens_line);
        assert!(tokens_line.contains("jitter"), "missing jitter: {}", tokens_line);
        // "retry backoff jitter" (multi-word, no exclusions) → FileLevel
        // per `line_match_strategy`. Tokens strategy is tested separately
        // with an exclusion query below.
        assert!(hint.contains("Strategy used: FileLevel"));
        assert!(hint.contains("language=rust"));
        assert!(hint.contains("exclude_tests=true"));
        assert!(hint.contains("Zero-hit reason: line_match_miss"));
    }

    #[test]
    fn multi_token_hint_renders_none_filters_and_unknown_reason() {
        let hint = build_multi_token_zero_hit_hint(
            "error handling retry",
            None,
            None,
            None,
            None,
        );
        assert!(hint.contains("file_pattern=(none)"));
        assert!(hint.contains("language=(none)"));
        assert!(hint.contains("exclude_tests=auto"));
        assert!(hint.contains("Zero-hit reason: unknown"));
    }

    #[test]
    fn multi_token_hint_strategy_reflects_line_match_strategy() {
        // Quoted queries fall into Substring regardless of token count.
        let hint = build_multi_token_zero_hit_hint(
            "\"fn main\"",
            None,
            None,
            None,
            None,
        );
        assert!(
            hint.contains("Strategy used: Substring"),
            "expected Substring strategy for quoted query, got: {}",
            hint
        );

        // Multi-token with exclusion token (leading '-') triggers Tokens.
        let hint_tokens = build_multi_token_zero_hit_hint(
            "retry -mock",
            None,
            None,
            None,
            None,
        );
        assert!(
            hint_tokens.contains("Strategy used: Tokens"),
            "expected Tokens strategy for exclusion query, got: {}",
            hint_tokens
        );
    }

    #[test]
    fn multi_token_hint_preserves_file_pattern_in_filters_line_exactly() {
        let hint = build_multi_token_zero_hit_hint(
            "foo bar",
            Some("src/database/*.rs,src/database/**/*.rs"),
            None,
            None,
            None,
        );
        // The literal comma-separated pattern should appear intact in both
        // the header and the Filters line (no splitting, no quoting).
        assert!(hint.contains("with file_pattern=src/database/*.rs,src/database/**/*.rs."));
        assert!(hint.contains("file_pattern=src/database/*.rs,src/database/**/*.rs, language="));
    }
}
