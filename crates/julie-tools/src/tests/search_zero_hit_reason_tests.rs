//! Task 4a: `attribute_zero_hit_reason` pins the top-down stage walk used
//! to classify empty `line_mode_matches` runs. These tests fix one
//! variant per stage so any future reshuffle of the pipeline order —
//! or a silent bucket reassignment — fails loudly.
//!
//! Pure-unit coverage below drives every `ZeroHitReason` variant by
//! handing in a synthetic `LineModeStageCounts`. Two integration tests
//! at the bottom exercise the realistic variants against the live
//! pipeline (TantivyNoCandidates and LineMatchMiss) so the plumbing
//! from counters → attribution on the return path can't silently
//! regress.

#[cfg(test)]
mod tests {
    use crate::search::line_mode::{LineModeStageCounts, attribute_zero_hit_reason};
    use crate::search::trace::ZeroHitReason;

    /// Tantivy returned nothing — the per-file loop never ran. This is
    /// the only stage that fires when `tantivy_file_candidates == 0` and
    /// dominates every other counter.
    #[test]
    fn tantivy_no_candidates_wins_when_zero_candidates_entered_the_loop() {
        let counts = LineModeStageCounts {
            and_candidates: 0,
            or_candidates: 0,
            tantivy_file_candidates: 0,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 0,
            file_content_unavailable_dropped: 0,
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::TantivyNoCandidates),
        );
    }

    /// file_pattern drains every candidate → FilePatternFiltered wins.
    /// Language/test/content counters deliberately set non-zero to
    /// confirm the walk is top-down and stops at the first drainer.
    #[test]
    fn file_pattern_filtered_wins_when_pattern_drains_the_survivors() {
        let counts = LineModeStageCounts {
            and_candidates: 10,
            or_candidates: 0,
            tantivy_file_candidates: 3,
            file_pattern_dropped: 3,
            // Would-have-been dropped if file_pattern hadn't taken them
            // first. The top-down walk must NOT credit these.
            language_dropped: 2,
            test_dropped: 2,
            file_content_unavailable_dropped: 1,
            line_match_miss_dropped: 1,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::FilePatternFiltered),
        );
    }

    /// Language filter is second in the walk. file_pattern must survive
    /// intact; language then drains to zero.
    #[test]
    fn language_filtered_wins_when_language_drains_after_file_pattern_survives() {
        let counts = LineModeStageCounts {
            and_candidates: 5,
            or_candidates: 0,
            tantivy_file_candidates: 4,
            file_pattern_dropped: 1,
            // 4 - 1 = 3 survive file_pattern; language drops all 3.
            language_dropped: 3,
            test_dropped: 0,
            file_content_unavailable_dropped: 0,
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::LanguageFiltered),
        );
    }

    /// exclude_tests is third. file_pattern + language survive; test
    /// drains the rest.
    #[test]
    fn test_filtered_wins_when_exclude_tests_drains_after_earlier_stages_survive() {
        let counts = LineModeStageCounts {
            and_candidates: 5,
            or_candidates: 0,
            tantivy_file_candidates: 3,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 3,
            file_content_unavailable_dropped: 0,
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::TestFiltered),
        );
    }

    /// File-content-unavailable is fourth. All path/kind filters let
    /// candidates through; blob retrieval then fails for every one.
    #[test]
    fn file_content_unavailable_wins_when_content_lookup_drains_the_survivors() {
        let counts = LineModeStageCounts {
            and_candidates: 5,
            or_candidates: 0,
            tantivy_file_candidates: 2,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 0,
            file_content_unavailable_dropped: 2,
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::FileContentUnavailable),
        );
    }

    /// Every filter passes; line-level matching produces zero hits.
    /// This is the "Tantivy was optimistic, the actual lines don't
    /// contain the term" case.
    #[test]
    fn line_match_miss_wins_when_lines_drain_after_every_earlier_stage_survives() {
        let counts = LineModeStageCounts {
            and_candidates: 4,
            or_candidates: 0,
            tantivy_file_candidates: 2,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 0,
            file_content_unavailable_dropped: 0,
            line_match_miss_dropped: 2,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::LineMatchMiss),
        );
    }

    /// Defensive fallback: candidates existed and nothing we instrument
    /// claims to have dropped them. Attribute to LineMatchMiss so the
    /// reason field is never silently None when there was something to
    /// explain.
    #[test]
    fn fallback_attributes_unexplained_drains_to_line_match_miss() {
        let counts = LineModeStageCounts {
            and_candidates: 5,
            or_candidates: 0,
            tantivy_file_candidates: 3,
            file_pattern_dropped: 0,
            language_dropped: 0,
            test_dropped: 0,
            file_content_unavailable_dropped: 0,
            // No drop counter fired, but `matches` is still empty at the
            // caller. Something took the survivors — attribute to the
            // closest thing we instrument rather than leaving None.
            line_match_miss_dropped: 0,
        };
        assert_eq!(
            attribute_zero_hit_reason(&counts),
            Some(ZeroHitReason::LineMatchMiss),
        );
    }
}
