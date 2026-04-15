# Tree-Sitter World-Class Hardening Review

## Status

- Result: pass
- Decision: extractor hardening is recovered, the integration-bucket blocker was resolved as parallel test interference, and fresh full-tier verification is green

## Verification Commands

### Current gate

- `cargo fmt --check`
  - Status: pass
- `cargo clippy --quiet`
  - Status: pass
  - Note: exited successfully with warning noise in pre-existing legacy files and tests
- `cargo test -p julie-extractors --lib`
  - Status: pass
  - Evidence: `test result: ok. 1593 passed; 0 failed; 7 ignored; 0 measured; 0 filtered out; finished in 0.33s`
- `cargo xtask test dev`
  - Status: pass
  - Evidence: `SUMMARY: 9 buckets passed in 377.4s`
- `cargo xtask test system`
  - Status: pass
  - Evidence: `SUMMARY: 2 buckets passed in 50.9s`
- `cargo xtask test full`
  - Status: pass
  - Evidence: `SUMMARY: 12 buckets passed in 587.7s`

### Resolved full-tier blocker

- `cargo test --lib test_fresh_index_no_reindex_needed -- --nocapture`
  - Status: pass
- `cargo test --lib test_fresh_index_with_extensionless_text_files_needs_no_reindex -- --nocapture`
  - Status: pass
- `cargo test --lib tests::integration::stale_index_detection -- --nocapture`
  - Status: pass
  - Evidence: `12 passed; 0 failed`
- `cargo test --lib tests::integration -- --skip search_quality`
  - Status: pass when serialized, fail when parallel before the fix
  - Evidence:
    - serialized run: `141 passed; 0 failed; 5 ignored`
    - pre-fix xtask/system run: the two stale-index freshness tests failed under the parallel `integration` bucket
- `cargo test --lib tests::integration -- --skip search_quality --test-threads=1`
  - Status: pass
  - Evidence: `141 passed; 0 failed; 5 ignored`
  - Interpretation: the stale-index logic was sound; the real bug was parallel test interference from unsafe process-environment mutation

### Targeted extractor checks

- `cargo test -p julie-extractors --lib test_cross_file_extends_creates_pending_relationship`
  - Red: fail, missing structured namespace-qualified heritage targets in JS and TS
  - Green: pass
- `cargo test -p julie-extractors --lib test_cross_file_implements_creates_pending_relationship`
  - Green after TS generic heritage fix: pass
- `cargo test -p julie-extractors --lib test_csharp_constructor_cross_file_type_creates_pending`
  - Red: fail, missing structured pending Uses targets
  - Green: pass
- `cargo test -p julie-extractors --lib test_di_cross_file_creates_pending`
  - Red: fail, missing structured pending Instantiates targets
  - Green: pass
- `cargo test -p julie-extractors --lib test_cross_file_field_type_creates_pending_relationship`
  - Red: fail, missing structured pending Uses targets
  - Green: pass
- `cargo test -p julie-extractors --lib test_rust_plain_line_comment_is_not_doc_comment`
  - Red: fail, plain Rust `//` comment was treated as doc
  - Green: pass
- `cargo test -p julie-extractors --lib test_python_hash_comment_is_not_doc_comment`
  - Red: fail, Python `#` comment was treated as doc
  - Green: pass
- `cargo test -p julie-extractors --lib test_identifier_kind_import_falls_back_to_variable_ref`
  - Red: fail, dead `IdentifierKind::Import` still round-tripped
  - Green: pass
- `cargo test -p julie-extractors --lib test_method_call_on_external_type_creates_pending_relationship`
  - Red: fail, Kotlin test still asserted the pre-hardening degraded pending name (`compute`)
  - Green: pass after updating the test to assert the structured/receiver-qualified contract
- `cargo test -p julie-extractors --lib test_cross_file_method_call_creates_pending_relationship`
  - Green: pass after tightening Java receiver-qualified pending expectations
- `cargo test -p julie-extractors --lib test_cross_file_constructor_call_creates_pending_relationship`
  - Green: pass after tightening Java constructor and receiver-qualified method expectations

## Review Findings

### Final standalone review

- Reviewer found no remaining extractor-core Critical blockers in code or tests.
- Reviewer found two Medium regression-quality gaps in Java cross-file tests:
  - stale allowance for bare `process` instead of `Helper.process`
  - stale allowance for bare `getValue` instead of `calc.getValue`
- Disposition: fixed by tightening the Java regression tests to require the receiver-qualified structured contract.

### Earlier Important findings, now closed

- Extra public extraction path bypassing canonical parsing
  - Disposition: closed
  - Evidence: `extract_symbols_and_relationships` is internal-only; workspace indexing now calls `extract_canonical`
- Incomplete structured pending migration in JS, TS, and C# helper paths
  - Disposition: closed
  - Evidence: targeted regression tests above plus fresh extractor suite pass
- Loose shared doc-comment policy and dead `IdentifierKind::Import`
  - Disposition: closed
  - Evidence: dedicated `doc_comments.rs` and `identifier_semantics.rs` invariants plus fresh extractor suite pass
- Missing Task 10 and Task 11 invariant modules
  - Disposition: closed
  - Evidence: added `type_invariants.rs`, `path_invariants.rs`, `jsonl_invariants.rs`, and `review_regressions.rs`
- Missing consumer docs
  - Disposition: closed
  - Evidence: `crates/julie-extractors/README.md`

### Resolved branch-level finding

- Parallel integration runs were mutating process env without a shared lock.
  - Disposition: closed
  - Evidence:
    - the stale-index freshness tests passed in isolation and under serialized integration runs
    - `cargo xtask test system` failed before the fix and passed after aligning env-mutating integration tests with `#[serial_test::serial(embedding_env)]`
    - `cargo xtask test full` is now green
  - Assessment: this was test interference, not an extractor-crate or stale-index production-logic regression

## Remaining Important Findings

- None.

## Remaining Minor Findings

- `cargo clippy --quiet` still emits warning noise across legacy extractor and test files outside the hardening slice.
  - Disposition: accepted for the extractor slice
  - Reason: the lint run exits successfully, and the warnings are style debt rather than extractor-core correctness regressions

## Downgrade Records

- Prior blocked status is cleared.
  - Reason: fresh verification now shows the branch is green end-to-end, and the former blocker was a resolved parallel-test interference issue

## Exit-Criteria Judgment

- Canonical extraction path is the single public source of truth: yes
- Public entrypoints are consistent thin projections or canonical entrypoints: yes
- JSONL production-path correctness is pinned: yes
- Path and ID invariants are pinned by dedicated tests: yes
- Structured unresolved relationships preserve higher-fidelity context in the touched review gaps: yes
- Shared semantic policy for doc comments and identifier semantics is explicit and tested: yes
- Regression coverage is sharper and includes dedicated invariant modules: yes
- Fresh extractor-crate verification is green: yes
- Fresh repo-wide full-tier verification is green: yes

## Recommendation

The extractor hardening slice is recovered, the parallel integration-test interference is resolved, and the branch now has fresh repo-wide full-tier evidence. It is ready for repo-owner world-class sign-off, subject to the accepted pre-existing warning noise from `cargo clippy --quiet`.
