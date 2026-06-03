//! Tests for search-result reranking boost rules and deterministic ordering.

use julie_extractors::SymbolKind;
use crate::search::query_parse::parse_query;
use crate::search::reranker::{
    BODY_TERM_BOOST, Candidate, EXACT_TITLE_BOOST, INTENT_ROLE_MATCH_BOOST,
    INTENT_TITLE_MATCH_BOOST, PARTIAL_TITLE_BOOST, PATH_BOOST, PHRASE_BOOST, PHRASE_FILE_DOC_BOOST,
    PHRASE_SOURCE_LANG_BOOST, kind_boost, rerank_unified,
};

/// Helper: a single-candidate rerank that returns the final score.
fn score_query(raw: &str, c: Candidate) -> f32 {
    let q = parse_query(raw);
    rerank_unified(&q, &[c])[0].final_score
}

// ----- Per-term boosts -----

#[test]
fn test_reranker_score_exact_title_match_per_term() {
    // Isolate the per-term exact-title boost: the exact match must
    // land on a NON-FIRST target term so the kind_boost rule (which
    // fires only when the title equals the first term) doesn't
    // co-fire and inflate the score.
    let c = Candidate::builder().title("fooBar").build();
    let s = score_query("zzz fooBar yyy", c);
    // "zzz" no match, "foobar" exact (+100), "yyy" no match.
    // kind_boost: first term "zzz" != "foobar" -> does not fire.
    assert!((s - EXACT_TITLE_BOOST).abs() < 1e-3, "got {s}");
}

#[test]
fn test_reranker_score_partial_title_match() {
    // Title is "compute_fooBar"; lowercased contains "foobar" but
    // isn't equal to it -> +50 (partial), not +100 (exact).
    let c = Candidate::builder().title("compute_fooBar").build();
    let s = score_query("fooBar", c);
    assert!(
        (s - PARTIAL_TITLE_BOOST).abs() < 1e-3,
        "expected partial-title boost, got {s}"
    );
}

#[test]
fn test_reranker_score_path_contains_term() {
    let c = Candidate::builder()
        .title("totally_unrelated")
        .path("src/foobar/mod.rs")
        .build();
    let s = score_query("foobar", c);
    // Path boost only; no title match.
    assert!((s - PATH_BOOST).abs() < 1e-3, "got {s}");
}

#[test]
fn test_reranker_score_body_contains_term() {
    let c = Candidate::builder()
        .title("unrelated")
        .path("nowhere.rs")
        .body("This computes the foobar across iterations.")
        .build();
    let s = score_query("foobar", c);
    assert!((s - BODY_TERM_BOOST).abs() < 1e-3, "got {s}");
}

// ----- Phrase boost -----

#[test]
fn test_reranker_score_phrase_boost_requires_min_terms() {
    // 3-term query -> phrase boost does NOT apply even if body
    // contains the phrase verbatim.
    let body = "alpha beta gamma is here";
    let c = Candidate::builder()
        .title("zzz")
        .path("zzz.rs")
        .body(body)
        .build();
    let s = score_query("alpha beta gamma", c);
    // Body contains each term individually -> 3 x BODY_TERM_BOOST = 30
    // Plus the path/title contain none. Phrase boost MUST NOT fire.
    assert!(
        s < PHRASE_BOOST,
        "phrase boost should not fire on 3-term query; got {s}"
    );
    assert!(
        (s - 3.0 * BODY_TERM_BOOST).abs() < 1e-3,
        "got {s}, expected only per-term body boosts"
    );
}

#[test]
fn test_reranker_score_phrase_boost_fires_at_4_terms() {
    // 4 target terms with the contiguous phrase in body.
    let body = "...alpha beta gamma delta is the magic phrase...";
    let c = Candidate::builder()
        .title("zzz")
        .path("zzz.rs")
        .body(body)
        .build();
    let s = score_query("alpha beta gamma delta", c);
    // 4x body-term + phrase-boost. No title/path match.
    let expected = 4.0 * BODY_TERM_BOOST + PHRASE_BOOST;
    assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
}

#[test]
fn test_reranker_score_phrase_boost_file_doc_bonus() {
    let c = Candidate::builder()
        .title("zzz")
        .path("docs/zzz.md")
        .body("alpha beta gamma delta")
        .is_file_doc(true)
        .build();
    let s = score_query("alpha beta gamma delta", c);
    let expected = 4.0 * BODY_TERM_BOOST + PHRASE_BOOST + PHRASE_FILE_DOC_BOOST;
    assert!((s - expected).abs() < 1e-3, "got {s}");
}

#[test]
fn test_reranker_score_phrase_boost_source_language_bonus() {
    let c = Candidate::builder()
        .title("zzz")
        .path("src/foo.rs")
        .body("alpha beta gamma delta")
        .is_source_language(true)
        .build();
    let s = score_query("alpha beta gamma delta", c);
    let expected = 4.0 * BODY_TERM_BOOST + PHRASE_BOOST + PHRASE_SOURCE_LANG_BOOST;
    assert!((s - expected).abs() < 1e-3, "got {s}");
}

#[test]
fn test_reranker_source_language_phrase_bonus_requires_non_docs_role() {
    let c = Candidate::builder()
        .title("zzz")
        .path("docs/Classes/Foo.html")
        .body("alpha beta gamma delta")
        .role("docs")
        .is_file_doc(true)
        .is_source_language(true)
        .build();
    let s = score_query("alpha beta gamma delta", c);
    let expected = 4.0 * BODY_TERM_BOOST + PHRASE_BOOST + PHRASE_FILE_DOC_BOOST;
    assert!(
        (s - expected).abs() < 1e-3,
        "docs-role candidates must not receive source-language phrase boost; got {s}, expected {expected}"
    );
}

// ----- Intent boosts -----

#[test]
fn test_reranker_score_symbol_intent_kind_match_and_title_match() {
    // "function fooBar baz" -> Symbol(Function), target_terms=[foobar,baz].
    // Candidate is a Function whose title contains "foobar".
    let c = Candidate::builder()
        .title("fooBar")
        .kind(SymbolKind::Function)
        .build();
    let s = score_query("function fooBar baz", c);
    // Boosts that fire:
    //   per-term title "foobar" -> exact match (+100)
    //   per-term title "baz" -> no match (lowercased title "foobar"
    //     does not contain "baz")
    //   intent_title_match (+180)
    //   intent_role_match (+120) since is_test=false
    //   kind_boost on first-term exact title (+60 for Function)
    let expected = EXACT_TITLE_BOOST + INTENT_TITLE_MATCH_BOOST + INTENT_ROLE_MATCH_BOOST + 60.0;
    assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
}

#[test]
fn test_reranker_score_symbol_intent_no_test_penalty_when_is_test() {
    // Same query but the candidate is a test - drop the +120 role bonus.
    let c = Candidate::builder()
        .title("fooBar")
        .kind(SymbolKind::Function)
        .is_test(true)
        .build();
    let s = score_query("function fooBar baz", c);
    let expected = EXACT_TITLE_BOOST + INTENT_TITLE_MATCH_BOOST + 60.0;
    assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
}

#[test]
fn test_reranker_score_symbol_intent_skips_when_kind_mismatch() {
    // Symbol(Function) intent but candidate is a Struct.
    let c = Candidate::builder()
        .title("fooBar")
        .kind(SymbolKind::Struct)
        .build();
    let s = score_query("function fooBar baz", c);
    // Only per-term title-exact + (no kind_boost because the first
    // target term "foobar" matches title_lc -> kind_boost fires per
    // its rule, which is independent of intent).
    let expected = EXACT_TITLE_BOOST + kind_boost(&SymbolKind::Struct);
    assert!(
        (s - expected).abs() < 1e-3,
        "got {s}, expected {expected} (intent bonus must NOT fire on kind mismatch)"
    );
}

#[test]
fn test_reranker_score_test_intent_title_match() {
    // "test foo bar" -> Test, target_terms=[foo, bar].
    // Candidate is a test whose title matches "foo".
    let c = Candidate::builder()
        .title("foo")
        .kind(SymbolKind::Function)
        .is_test(true)
        .build();
    let s = score_query("test foo bar", c);
    // Per-term: title "foo" exact (+100); "bar" no match.
    // intent_title_match (+180) and intent_role_match (+120) since is_test=true.
    // kind_boost on first-term exact match: Function = +60.
    let expected = EXACT_TITLE_BOOST + INTENT_TITLE_MATCH_BOOST + INTENT_ROLE_MATCH_BOOST + 60.0;
    assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
}

#[test]
fn test_reranker_score_test_intent_no_role_bonus_when_not_test() {
    // Test intent matched on title but candidate is production code.
    let c = Candidate::builder()
        .title("foo")
        .kind(SymbolKind::Function)
        .is_test(false)
        .build();
    let s = score_query("test foo bar", c);
    let expected = EXACT_TITLE_BOOST + INTENT_TITLE_MATCH_BOOST + 60.0;
    assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
}

#[test]
fn test_reranker_score_free_intent_has_no_intent_bonus() {
    // 3 generic terms -> Free intent. Candidate title matches none of
    // them so per-term boosts only fire on body.
    let c = Candidate::builder()
        .title("Calculator")
        .body("alpha beta gamma here")
        .build();
    let s = score_query("alpha beta gamma", c);
    // 3x body-term, no title, no path, no phrase boost (<4 terms),
    // no intent boost.
    assert!(
        (s - 3.0 * BODY_TERM_BOOST).abs() < 1e-3,
        "got {s}, expected only 3x body term boost"
    );
}

// ----- Kind boost -----

#[test]
fn test_reranker_score_kind_boost_fires_on_first_term_exact_title() {
    // Title exactly equals first target term -> +kind_boost(kind).
    let c = Candidate::builder()
        .title("fooBar")
        .kind(SymbolKind::Trait)
        .build();
    let s = score_query("fooBar irrelevant other", c);
    // Per-term boosts:
    //   "foobar" -> title exact (+100)
    //   "irrelevant" -> no match
    //   "other" -> no match
    // Kind boost for first target equals title: Trait = +50.
    let expected = EXACT_TITLE_BOOST + 50.0;
    assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
}

#[test]
fn test_reranker_score_kind_boost_not_applied_when_title_doesnt_match_first_term() {
    // First target term doesn't equal the title -> no kind boost.
    let c = Candidate::builder()
        .title("compute_fooBar") // partial match only
        .kind(SymbolKind::Trait)
        .build();
    let s = score_query("fooBar other", c);
    // Per-term:
    //   "foobar" -> title contains (partial +50)
    //   "other" -> no match
    // No kind boost (title_lc != "foobar").
    let expected = PARTIAL_TITLE_BOOST;
    assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
}

// ----- Sort stability -----

#[test]
fn test_reranker_sort_stability_equal_scores_break_on_title_then_path() {
    // Three candidates with identical zero score (free query, no
    // matches). Output must come back in title-asc, path-asc order.
    let c_b_z = Candidate::builder().title("Beta").path("z/file.rs").build();
    let c_a_x = Candidate::builder()
        .title("Alpha")
        .path("x/file.rs")
        .build();
    let c_a_y = Candidate::builder()
        .title("Alpha")
        .path("y/file.rs")
        .build();

    let q = parse_query("nothing matches");
    let ranked = rerank_unified(&q, &[c_b_z, c_a_x, c_a_y]);
    let titles_paths: Vec<(String, String)> = ranked
        .iter()
        .map(|r| (r.candidate.title.clone(), r.candidate.path.clone()))
        .collect();
    assert_eq!(
        titles_paths,
        vec![
            ("Alpha".to_string(), "x/file.rs".to_string()),
            ("Alpha".to_string(), "y/file.rs".to_string()),
            ("Beta".to_string(), "z/file.rs".to_string()),
        ]
    );
}

#[test]
fn test_reranker_preserves_tantivy_base_score() {
    // No boosts fire; tantivy_score should pass through.
    let c = Candidate::builder()
        .title("unrelated")
        .tantivy_score(7.5)
        .build();
    let s = score_query("absolutely no overlap here", c);
    assert!((s - 7.5).abs() < 1e-3, "got {s}, expected 7.5 passthrough");
}

// ----- Codex finding #6: reranker writeback key collision -----

/// A file row with name="foo" at path="src/foo.rs" and a symbol named "foo"
/// in that same file have identical (path, title) — the old HashMap key used
/// for score writeback.  With ordinal-based writeback the two candidates must
/// receive distinct final scores that correctly map back by position.
#[test]
fn reranker_writeback_disambiguates_same_name_same_path() {
    let file_row = Candidate::builder()
        .title("foo")
        .path("src/foo.rs")
        .is_file_doc(true)
        .tantivy_score(1.5)
        .build();

    let sym_row = Candidate::builder()
        .title("foo")
        .path("src/foo.rs")
        .is_file_doc(false)
        .kind(SymbolKind::Function)
        .tantivy_score(0.8)
        .build();

    let candidates = vec![file_row, sym_row];
    let parsed = parse_query("foo");
    let ranked = rerank_unified(&parsed, &candidates);

    assert_eq!(
        ranked.len(),
        2,
        "both candidates must appear in ranked output"
    );

    // Each Ranked entry carries its original candidate ordinal.
    let r_for_0 = ranked
        .iter()
        .find(|r| r.original_index == 0)
        .expect("original_index=0 (file row) must appear in ranked output");
    let r_for_1 = ranked
        .iter()
        .find(|r| r.original_index == 1)
        .expect("original_index=1 (symbol row) must appear in ranked output");

    // The file row gets different boosts than the function symbol, so scores must differ.
    assert_ne!(
        r_for_0.final_score, r_for_1.final_score,
        "file row and symbol with the same (path, title) must produce distinct final scores"
    );

    // Simulate index-based writeback (the fixed approach in search_unified_full).
    let mut scores = vec![f32::NAN; 2];
    for r in &ranked {
        scores[r.original_index] = r.final_score;
    }

    assert!(
        scores[0].is_finite() && scores[1].is_finite(),
        "both hit slots must be populated by index-based writeback"
    );
    assert_ne!(
        scores[0], scores[1],
        "index-based writeback must assign distinct scores to candidates \
         that share (path, title) but differ in row type"
    );
}
