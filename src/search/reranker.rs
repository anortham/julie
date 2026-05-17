//! Search-result reranker.
//!
//! Plan task C.2: given a [`ParsedQuery`] from C.1 and a slice of
//! [`Candidate`]s (Tantivy top-K plus enriched schema fields from C.3),
//! produce a ranked vector with adjusted scores. Pure function, no I/O.
//!
//! Design ref: `docs/plans/2026-05-15-daemon-split-and-search-reranker-design.md` §C.3
//!
//! Boost values are starting points lifted from eros's scorer and will be
//! tuned via the `cargo xtask test dogfood` regression bucket in C.4.

use std::cmp::Ordering;

use crate::extractors::SymbolKind;
use crate::search::query_parse::{ParsedQuery, QueryIntent};

// ---------------------------------------------------------------------------
// Boost weights (tunable; named so they show up in grep when tuning)
// ---------------------------------------------------------------------------

const EXACT_TITLE_BOOST: f32 = 100.0;
const PARTIAL_TITLE_BOOST: f32 = 50.0;
const PATH_BOOST: f32 = 40.0;
const BODY_TERM_BOOST: f32 = 10.0;
const PHRASE_BOOST: f32 = 260.0;
const PHRASE_FILE_DOC_BOOST: f32 = 120.0;
const PHRASE_SOURCE_LANG_BOOST: f32 = 50.0;
const INTENT_TITLE_MATCH_BOOST: f32 = 180.0;
const INTENT_ROLE_MATCH_BOOST: f32 = 120.0;
const PHRASE_MIN_TERMS: usize = 4;

// ---------------------------------------------------------------------------
// Candidate
// ---------------------------------------------------------------------------

/// A single candidate the reranker scores. The fields below mirror what
/// the enriched Tantivy schema (C.3) returns; tests construct candidates
/// via [`Candidate::builder`].
#[derive(Debug, Clone)]
pub struct Candidate {
    pub title: String,
    pub path: String,
    /// Body excerpt — typically the symbol signature + a slice of source.
    /// The reranker lowercases this on the fly; callers can pass the
    /// original capitalization.
    pub body: String,
    pub kind: SymbolKind,
    /// Role per C.3: `"test" | "source" | "docs" | "generated" | "vendor"
    /// | "unknown"`. Stored as String to match the Tantivy schema.
    pub role: String,
    /// Test sub-role per C.3: `"unit" | "integration" | "smoke" | ""`.
    pub test_role: String,
    pub is_test: bool,
    pub is_file_doc: bool,
    pub is_source_language: bool,
    pub tantivy_score: f32,
}

impl Candidate {
    pub fn builder() -> CandidateBuilder {
        CandidateBuilder::default()
    }

    /// Body contains `term` (case-insensitive). Caller passes a lowercased
    /// term; we lowercase the body inside.
    fn body_contains_term(&self, term: &str) -> bool {
        self.body.to_lowercase().contains(term)
    }

    /// Body contains `phrase` (case-insensitive).
    fn body_contains_phrase(&self, phrase: &str) -> bool {
        self.body.to_lowercase().contains(phrase)
    }

    fn kind_matches(&self, kind: &SymbolKind) -> bool {
        &self.kind == kind
    }

    /// True iff at least one target term appears (case-insensitively) in
    /// the title. Lowercased title is passed in so we don't re-allocate
    /// per call.
    fn title_matches_any_term(title_lc: &str, target_terms: &[String]) -> bool {
        target_terms.iter().any(|t| title_lc.contains(t.as_str()))
    }
}

/// Fluent builder for tests and Tantivy result wiring. Defaults populate
/// every field so tests only set what the case under exercise needs.
#[derive(Debug, Clone)]
pub struct CandidateBuilder {
    inner: Candidate,
}

impl Default for CandidateBuilder {
    fn default() -> Self {
        Self {
            inner: Candidate {
                title: String::new(),
                path: String::new(),
                body: String::new(),
                kind: SymbolKind::Function,
                role: "unknown".to_string(),
                test_role: String::new(),
                is_test: false,
                is_file_doc: false,
                is_source_language: false,
                tantivy_score: 0.0,
            },
        }
    }
}

#[allow(dead_code)]
impl CandidateBuilder {
    pub fn title(mut self, v: impl Into<String>) -> Self {
        self.inner.title = v.into();
        self
    }
    pub fn path(mut self, v: impl Into<String>) -> Self {
        self.inner.path = v.into();
        self
    }
    pub fn body(mut self, v: impl Into<String>) -> Self {
        self.inner.body = v.into();
        self
    }
    pub fn kind(mut self, v: SymbolKind) -> Self {
        self.inner.kind = v;
        self
    }
    pub fn role(mut self, v: impl Into<String>) -> Self {
        self.inner.role = v.into();
        self
    }
    pub fn test_role(mut self, v: impl Into<String>) -> Self {
        self.inner.test_role = v.into();
        self
    }
    pub fn is_test(mut self, v: bool) -> Self {
        self.inner.is_test = v;
        self
    }
    pub fn is_file_doc(mut self, v: bool) -> Self {
        self.inner.is_file_doc = v;
        self
    }
    pub fn is_source_language(mut self, v: bool) -> Self {
        self.inner.is_source_language = v;
        self
    }
    pub fn tantivy_score(mut self, v: f32) -> Self {
        self.inner.tantivy_score = v;
        self
    }
    pub fn build(self) -> Candidate {
        self.inner
    }
}

// ---------------------------------------------------------------------------
// Ranked output
// ---------------------------------------------------------------------------

/// One reranked result.
#[derive(Debug, Clone)]
pub struct Ranked {
    pub candidate: Candidate,
    pub final_score: f32,
}

// ---------------------------------------------------------------------------
// Per-kind boost for exact-title matches
// ---------------------------------------------------------------------------

/// Bonus applied when the candidate title equals the first target term
/// exactly. Higher for "primary" symbol kinds; lower for import/export
/// shadows.
fn kind_boost(kind: &SymbolKind) -> f32 {
    use SymbolKind::*;
    match kind {
        Function | Method => 60.0,
        Class | Struct | Trait | Interface => 50.0,
        Enum | Type => 40.0,
        Module | Namespace => 30.0,
        Constant | Variable | Field | Property | EnumMember | Constructor | Destructor
        | Operator | Event | Delegate | Union => 20.0,
        Import | Export => 5.0,
    }
}

// ---------------------------------------------------------------------------
// rerank()
// ---------------------------------------------------------------------------

/// Score every candidate per the C.3 algorithm and return them sorted by
/// `final_score` descending. Ties break on title-asc then path-asc for
/// determinism.
pub fn rerank(query: &ParsedQuery, candidates: &[Candidate]) -> Vec<Ranked> {
    let mut out: Vec<Ranked> = candidates
        .iter()
        .map(|c| Ranked {
            candidate: c.clone(),
            final_score: score_one(query, c),
        })
        .collect();

    out.sort_by(|a, b| {
        b.final_score
            .partial_cmp(&a.final_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.candidate.title.cmp(&b.candidate.title))
            .then_with(|| a.candidate.path.cmp(&b.candidate.path))
    });

    out
}

/// Score a single candidate per the C.3 algorithm without sorting.
///
/// Useful when the caller already has a result list with stable IDs and
/// wants to update each item's score in place, then sort itself.
pub fn rerank_score(query: &ParsedQuery, candidate: &Candidate) -> f32 {
    score_one(query, candidate)
}

fn score_one(query: &ParsedQuery, c: &Candidate) -> f32 {
    let mut score = c.tantivy_score;
    let title_lc = c.title.to_lowercase();

    // Per-term boosts: title, path, body.
    for term in &query.target_terms {
        if title_lc == *term {
            score += EXACT_TITLE_BOOST;
        } else if title_lc.contains(term) {
            score += PARTIAL_TITLE_BOOST;
        }
        if c.path.to_lowercase().contains(term) {
            score += PATH_BOOST;
        }
        if c.body_contains_term(term) {
            score += BODY_TERM_BOOST;
        }
    }

    // Phrase boost — only fires when the user gave us enough terms that
    // a contiguous phrase match means something.
    if query.target_terms.len() >= PHRASE_MIN_TERMS {
        let phrase = query.target_terms.join(" ");
        if c.body_contains_phrase(&phrase) {
            score += PHRASE_BOOST;
            if c.is_file_doc {
                score += PHRASE_FILE_DOC_BOOST;
            }
            if c.is_source_language {
                score += PHRASE_SOURCE_LANG_BOOST;
            }
        }
    }

    // Intent boosts.
    match &query.intent {
        QueryIntent::Symbol(kind) => {
            if c.kind_matches(kind)
                && Candidate::title_matches_any_term(&title_lc, &query.target_terms)
            {
                score += INTENT_TITLE_MATCH_BOOST;
                if !c.is_test {
                    score += INTENT_ROLE_MATCH_BOOST;
                }
            }
        }
        QueryIntent::Test => {
            if Candidate::title_matches_any_term(&title_lc, &query.target_terms) {
                score += INTENT_TITLE_MATCH_BOOST;
                if c.is_test {
                    score += INTENT_ROLE_MATCH_BOOST;
                }
            }
        }
        QueryIntent::Free => {}
    }

    // Kind boost on exact title-equals-first-term match.
    if let Some(first) = query.target_terms.first() {
        if title_lc == *first {
            score += kind_boost(&c.kind);
        }
    }

    score
}

// ---------------------------------------------------------------------------
// Tests (one per boost rule + sort stability)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::query_parse::parse_query;

    /// Helper: a single-candidate rerank that returns the final score.
    fn score_query(raw: &str, c: Candidate) -> f32 {
        let q = parse_query(raw);
        rerank(&q, &[c])[0].final_score
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
        // kind_boost: first term "zzz" != "foobar" → does not fire.
        assert!((s - EXACT_TITLE_BOOST).abs() < 1e-3, "got {s}");
    }

    #[test]
    fn test_reranker_score_partial_title_match() {
        // Title is "compute_fooBar"; lowercased contains "foobar" but
        // isn't equal to it → +50 (partial), not +100 (exact).
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
        // 3-term query → phrase boost does NOT apply even if body
        // contains the phrase verbatim.
        let body = "alpha beta gamma is here";
        let c = Candidate::builder()
            .title("zzz")
            .path("zzz.rs")
            .body(body)
            .build();
        let s = score_query("alpha beta gamma", c);
        // Body contains each term individually → 3 × BODY_TERM_BOOST = 30
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
        // 4× body-term + phrase-boost. No title/path match.
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

    // ----- Intent boosts -----

    #[test]
    fn test_reranker_score_symbol_intent_kind_match_and_title_match() {
        // "function fooBar baz" → Symbol(Function), target_terms=[foobar,baz].
        // Candidate is a Function whose title contains "foobar".
        let c = Candidate::builder()
            .title("fooBar")
            .kind(SymbolKind::Function)
            .build();
        let s = score_query("function fooBar baz", c);
        // Boosts that fire:
        //   per-term title "foobar" → exact match (+100)
        //   per-term title "baz" → no match (lowercased title "foobar"
        //     does not contain "baz")
        //   intent_title_match (+180)
        //   intent_role_match (+120) since is_test=false
        //   kind_boost on first-term exact title (+60 for Function)
        let expected = EXACT_TITLE_BOOST + INTENT_TITLE_MATCH_BOOST + INTENT_ROLE_MATCH_BOOST + 60.0;
        assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
    }

    #[test]
    fn test_reranker_score_symbol_intent_no_test_penalty_when_is_test() {
        // Same query but the candidate is a test — drop the +120 role bonus.
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
        // target term "foobar" matches title_lc → kind_boost fires per
        // its rule, which is independent of intent).
        let expected = EXACT_TITLE_BOOST + kind_boost(&SymbolKind::Struct);
        assert!(
            (s - expected).abs() < 1e-3,
            "got {s}, expected {expected} (intent bonus must NOT fire on kind mismatch)"
        );
    }

    #[test]
    fn test_reranker_score_test_intent_title_match() {
        // "test foo bar" → Test, target_terms=[foo, bar].
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
        // 3 generic terms → Free intent. Candidate title matches none of
        // them so per-term boosts only fire on body.
        let c = Candidate::builder()
            .title("Calculator")
            .body("alpha beta gamma here")
            .build();
        let s = score_query("alpha beta gamma", c);
        // 3× body-term, no title, no path, no phrase boost (<4 terms),
        // no intent boost.
        assert!(
            (s - 3.0 * BODY_TERM_BOOST).abs() < 1e-3,
            "got {s}, expected only 3× body term boost"
        );
    }

    // ----- Kind boost -----

    #[test]
    fn test_reranker_score_kind_boost_fires_on_first_term_exact_title() {
        // Title exactly equals first target term → +kind_boost(kind).
        let c = Candidate::builder()
            .title("fooBar")
            .kind(SymbolKind::Trait)
            .build();
        let s = score_query("fooBar irrelevant other", c);
        // Per-term boosts:
        //   "foobar" → title exact (+100)
        //   "irrelevant" → no match
        //   "other" → no match
        // Kind boost for first target equals title: Trait = +50.
        let expected = EXACT_TITLE_BOOST + 50.0;
        assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
    }

    #[test]
    fn test_reranker_score_kind_boost_not_applied_when_title_doesnt_match_first_term() {
        // First target term doesn't equal the title → no kind boost.
        let c = Candidate::builder()
            .title("compute_fooBar") // partial match only
            .kind(SymbolKind::Trait)
            .build();
        let s = score_query("fooBar other", c);
        // Per-term:
        //   "foobar" → title contains (partial +50)
        //   "other" → no match
        // No kind boost (title_lc != "foobar").
        let expected = PARTIAL_TITLE_BOOST;
        assert!((s - expected).abs() < 1e-3, "got {s}, expected {expected}");
    }

    // ----- Sort stability -----

    #[test]
    fn test_reranker_sort_stability_equal_scores_break_on_title_then_path() {
        // Three candidates with identical zero score (free query, no
        // matches). Output must come back in title-asc, path-asc order.
        let c_b_z = Candidate::builder()
            .title("Beta")
            .path("z/file.rs")
            .build();
        let c_a_x = Candidate::builder()
            .title("Alpha")
            .path("x/file.rs")
            .build();
        let c_a_y = Candidate::builder()
            .title("Alpha")
            .path("y/file.rs")
            .build();

        let q = parse_query("nothing matches");
        let ranked = rerank(&q, &[c_b_z, c_a_x, c_a_y]);
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
}
