//! C.4 — Test-helper discoverability dogfood.
//!
//! Loads `fixtures/search-quality/test-helper-discoverability.json` and
//! asserts that every query's `expect_name_in_top` symbol appears in the
//! top-N of a definition search against julie's own fixture, with the
//! reranker enabled.
//!
//! This is the regression guard for the design's C.4 promise: a user
//! searching `target="definitions"` for a known test-helper name (e.g.
//! `MockFooProvider`, `assertion_helper`) must still find it on page one
//! after the reranker re-weights production code.
//!
//! Acceptance ref: docs/plans/2026-05-16-daemon-split-and-search-reranker-plan.md §C.4

use serde::Deserialize;
use serial_test::serial;
use std::path::PathBuf;

use super::helpers::{search_definitions, setup_handler_with_fixture};

const FIXTURE_RELATIVE_PATH: &str = "fixtures/search-quality/test-helper-discoverability.json";
const RERANKER_ENV: &str = "JULIE_RERANKER_ENABLED";

#[derive(Debug, Deserialize)]
struct QuerySuite {
    default_top_n: usize,
    queries: Vec<QuerySpec>,
}

#[derive(Debug, Deserialize)]
struct QuerySpec {
    id: String,
    query: String,
    /// Symbol name we expect to find in top-N. Match is case-sensitive
    /// against `Symbol::name`.
    expect_name_in_top: String,
}

fn load_suite() -> QuerySuite {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_RELATIVE_PATH);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {}", path.display(), e));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse fixture {}: {}", path.display(), e))
}

/// Helper that overrides `JULIE_RERANKER_ENABLED` for the duration of
/// `body` and restores prior value on drop so a panicking test doesn't
/// leak the env var to siblings.
///
/// C.5: reranker is default-on, so `enable()` is a no-op semantically
/// (it explicitly sets `"1"` to defeat an inherited `"0"` from a parent
/// shell). `disable()` sets `"0"` to exercise the legacy-off path.
struct RerankerEnvGuard {
    prior: Option<String>,
}

impl RerankerEnvGuard {
    fn enable() -> Self {
        Self::set("1")
    }

    fn disable() -> Self {
        Self::set("0")
    }

    fn set(value: &str) -> Self {
        let prior = std::env::var(RERANKER_ENV).ok();
        // SAFETY: env var mutation is non-thread-safe; tests using this
        // helper are `#[serial]`.
        unsafe {
            std::env::set_var(RERANKER_ENV, value);
        }
        Self { prior }
    }
}

impl Drop for RerankerEnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.prior {
                Some(v) => std::env::set_var(RERANKER_ENV, v),
                None => std::env::remove_var(RERANKER_ENV),
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_c4_test_helpers_discoverable_with_reranker_enabled() {
    let _guard = RerankerEnvGuard::enable();
    let handler = setup_handler_with_fixture().await;
    let suite = load_suite();
    let top_n = suite.default_top_n as u32;

    let mut failures: Vec<String> = Vec::new();
    for spec in &suite.queries {
        let results = search_definitions(&handler, &spec.query, top_n)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "query '{}' (id={}): search error: {}",
                    spec.query, spec.id, e
                )
            });

        let found = results.iter().any(|s| s.name == spec.expect_name_in_top);
        if !found {
            let rendered = results
                .iter()
                .enumerate()
                .map(|(i, s)| format!("    {}. {} ({})", i + 1, s.name, s.file_path))
                .collect::<Vec<_>>()
                .join("\n");
            failures.push(format!(
                "[{}] query={:?}: expected {:?} in top {} but got:\n{}",
                spec.id, spec.query, spec.expect_name_in_top, top_n, rendered
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{}/{} test-helper queries failed discoverability with reranker enabled:\n\n{}",
        failures.len(),
        suite.queries.len(),
        failures.join("\n\n")
    );
}

/// Diagnostic baseline: the same queries also resolve with the reranker
/// explicitly disabled via `JULIE_RERANKER_ENABLED=0`. This catches the
/// case where a test helper is missing from the fixture entirely (which
/// would otherwise look like a reranker regression rather than the
/// fixture-staleness it actually is).
///
/// C.5: reranker is default-on, so "off" requires an explicit opt-out.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_c4_test_helpers_discoverable_baseline_reranker_off() {
    let _guard = RerankerEnvGuard::disable();
    let handler = setup_handler_with_fixture().await;
    let suite = load_suite();
    let top_n = suite.default_top_n as u32;

    let mut failures: Vec<String> = Vec::new();
    for spec in &suite.queries {
        let results = search_definitions(&handler, &spec.query, top_n)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "query '{}' (id={}): search error: {}",
                    spec.query, spec.id, e
                )
            });
        if !results.iter().any(|s| s.name == spec.expect_name_in_top) {
            failures.push(format!(
                "[{}] {:?} not in top {} (baseline, reranker off)",
                spec.id, spec.expect_name_in_top, top_n
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "baseline (reranker off): {}/{} queries failed — fixture may not contain these symbols:\n{}",
        failures.len(),
        suite.queries.len(),
        failures.join("\n")
    );
}
