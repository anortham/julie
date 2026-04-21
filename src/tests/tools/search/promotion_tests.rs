//! Tests for the composite `SearchExecutionKind::Promoted` variant plus the
//! new `HintKind` and `ZeroHitReason` trace enums introduced in the search
//! quality hardening plan.
//!
//! These tests are shared between Task 6 (trace additions) and Task 7 / Task 8
//! (content→definitions auto-promotion, multi-token hint formatter).

#[cfg(test)]
mod tests {
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
}
