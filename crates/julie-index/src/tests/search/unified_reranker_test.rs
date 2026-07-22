//! T6 — `rerank_unified` acceptance tests.
//!
//! Three acceptance criteria from the T6 plan:
//!
//! 1. `exact_name_beats_partial` — exact-name candidate scores ≥ partial-match
//!    candidate by at least `EXACT_TITLE_BOOST` (≥ 100) when tantivy scores are equal.
//!
//! 2. `file_basename_exact_beats_other_file_path_fragment` — a file-row candidate
//!    whose basename exactly matches the query beats another file-row at an
//!    unrelated path by at least 80 points when tantivy scores are equal.
//!
//! 3. `vendor_demoted` — a vendor-role candidate ends below a src-role candidate
//!    with the same boost level (uses existing VENDOR_PENALTY).

#[cfg(test)]
mod tests {
    use crate::search::query_parse::parse_query;
    use crate::search::reranker::{Candidate, EXACT_TITLE_BOOST, VENDOR_PENALTY, rerank_unified};
    use julie_extractors::SymbolKind;

    /// Build a symbol-row candidate.
    fn sym_cand(title: &str, path: &str, kind: SymbolKind, tantivy_score: f32) -> Candidate {
        Candidate::builder()
            .title(title)
            .path(path)
            .body(format!("fn {}", title))
            .kind(kind)
            .role("source")
            .test_role("")
            .is_test(false)
            .is_file_doc(false)
            .is_source_language(true)
            .tantivy_score(tantivy_score)
            .build()
    }

    /// Build a file-row candidate (is_file_doc = true, kind = Module sentinel).
    fn file_cand(basename: &str, path: &str, tantivy_score: f32) -> Candidate {
        Candidate::builder()
            .title(basename)
            .path(path)
            .body(String::new())
            .kind(SymbolKind::Module)
            .role("source")
            .test_role("")
            .is_test(false)
            .is_file_doc(true)
            .is_source_language(true)
            .tantivy_score(tantivy_score)
            .build()
    }

    // -------------------------------------------------------------------------
    // 1. exact_name_beats_partial
    // -------------------------------------------------------------------------

    /// A candidate whose name exactly matches the query must outscore a
    /// candidate whose name only partially matches by at least EXACT_TITLE_BOOST
    /// (≥ 100) when both start with the same tantivy score.
    #[test]
    fn exact_name_beats_partial() {
        let query = "BrowserClient";
        let parsed = parse_query(query);

        let exact = sym_cand(
            "BrowserClient",
            "src/browser_client.py",
            SymbolKind::Class,
            1.0,
        );
        let partial = sym_cand(
            "init_browser_client_pool",
            "src/pool.py",
            SymbolKind::Function,
            1.0,
        );

        let ranked = rerank_unified(&parsed, &[exact.clone(), partial.clone()]);

        assert_eq!(
            ranked.len(),
            2,
            "rerank_unified must return one Ranked entry per input candidate"
        );

        let exact_score = ranked
            .iter()
            .find(|r| r.candidate.title == "BrowserClient")
            .expect("exact candidate not found in output")
            .final_score;
        let partial_score = ranked
            .iter()
            .find(|r| r.candidate.title == "init_browser_client_pool")
            .expect("partial candidate not found in output")
            .final_score;

        let gap = exact_score - partial_score;
        assert!(
            gap >= EXACT_TITLE_BOOST,
            "exact-name candidate should outscore partial by ≥ {} (EXACT_TITLE_BOOST), \
             got exact={:.1} partial={:.1} gap={:.1}",
            EXACT_TITLE_BOOST,
            exact_score,
            partial_score,
            gap,
        );
    }

    // -------------------------------------------------------------------------
    // 2. file_basename_exact_beats_other_file_path_fragment
    // -------------------------------------------------------------------------

    /// A file-row candidate whose basename exactly matches the query (compact form)
    /// must outscore another file-row whose path merely contains the query as a
    /// fragment, by at least 80 points, when tantivy scores are equal.
    #[test]
    fn file_basename_exact_beats_other_file_path_fragment() {
        // Query is the basename stem (no extension).
        let query = "browser_client";
        let parsed = parse_query(query);

        // Exact basename match: "browser_client.py" → stem "browser_client"
        let exact_file = file_cand("browser_client.py", "src/browser_client.py", 1.0);
        // Path-fragment only: path contains "browser_client" but basename doesn't match
        let fragment_file = file_cand("helper.py", "src/utils/browser_client_helper.py", 1.0);

        let ranked = rerank_unified(&parsed, &[exact_file.clone(), fragment_file.clone()]);

        assert_eq!(ranked.len(), 2, "expected two ranked entries");

        let exact_score = ranked
            .iter()
            .find(|r| r.candidate.title == "browser_client.py")
            .expect("exact file candidate not found")
            .final_score;
        let fragment_score = ranked
            .iter()
            .find(|r| r.candidate.title == "helper.py")
            .expect("fragment file candidate not found")
            .final_score;

        let gap = exact_score - fragment_score;
        assert!(
            gap >= 80.0,
            "file exact-basename must outscore path-fragment by ≥ 80, \
             got exact={:.1} fragment={:.1} gap={:.1}",
            exact_score,
            fragment_score,
            gap,
        );
    }

    // -------------------------------------------------------------------------
    // 3. vendor_demoted
    // -------------------------------------------------------------------------

    /// A vendor-role candidate must end below a src-role candidate with an
    /// identical query match profile, by at least VENDOR_PENALTY.
    #[test]
    fn vendor_demoted() {
        let query = "HttpClient";
        let parsed = parse_query(query);

        let src_cand = sym_cand("HttpClient", "src/http_client.py", SymbolKind::Class, 1.0);

        // Vendor candidate — same title and tantivy_score as src candidate.
        let vendor_cand = Candidate::builder()
            .title("HttpClient")
            .path("vendor/requests/http_client.py")
            .body("class HttpClient")
            .kind(SymbolKind::Class)
            .role("vendor")
            .test_role("")
            .is_test(false)
            .is_file_doc(false)
            .is_source_language(true)
            .tantivy_score(1.0)
            .build();

        let ranked = rerank_unified(&parsed, &[src_cand.clone(), vendor_cand.clone()]);

        assert_eq!(ranked.len(), 2, "expected two ranked entries");

        let src_score = ranked
            .iter()
            .find(|r| r.candidate.role == "source")
            .expect("src candidate not found")
            .final_score;
        let vendor_score = ranked
            .iter()
            .find(|r| r.candidate.role == "vendor")
            .expect("vendor candidate not found")
            .final_score;

        assert!(
            src_score > vendor_score,
            "src candidate must outrank vendor; src={:.1} vendor={:.1}",
            src_score,
            vendor_score
        );

        let gap = src_score - vendor_score;
        assert!(
            gap >= VENDOR_PENALTY,
            "gap between src and vendor must be ≥ VENDOR_PENALTY ({:.0}), got {:.1}",
            VENDOR_PENALTY,
            gap,
        );
    }
}
