# TODO

## Bugs (found by Eros CT acceptance run, 2026-07-02, at v7.15.3 HEAD a12aad88)

- [ ] **`one_sidecar_serves_three_sessions` fails under any non-default TMPDIR** -- deterministic,
  cohort-independent: `TMPDIR=$(mktemp -d) cargo test --lib -- --exact 'tests::registry::embedding_host_multi_session::unix::one_sidecar_serves_three_sessions'`
  fails with "embedding-host did not become live within 15s" (embedding_host_multi_session.rs:193);
  passes with the login-default TMPDIR. Likely the host and client resolve temp differently
  (env temp vs darwin-user-temp), so the liveness/socket rendezvous never matches. **This is a
  product bug, not just a test bug** -- any user with a custom TMPDIR would hit it.
- [ ] **`test_all_dashboard_pages_return_200` is cohort-dependent** -- passes alone and in the full
  suite, but deterministically fails (GET / returns 500, metrics.rs:15) when run in a specific
  ~120-test cohort, with default env. Some test in that cohort poisons shared state. Repro cohort:
  the `--exact` filter list is in the Eros CT artifact
  (`~/.eros/workspaces/workspace_528d4264a7e9bdc41915c0f2/ct-build/ct_project_9251b26698b3a909c6f61dcc/TestResults/run-b85f171b*.cargo.log`,
  invocation at log line 2205).
- [ ] **Test suite writes into the repository while running** -- `.julie/indexes/**` (tantivy
  segments), `.julie/config/julie.toml`, and fixture-workspace mutations
  (`fixtures/test-workspaces/tiny-reference/src/helper.rs`, `tiny-primary/.julie/**`). Under any
  file-watcher-driven CI/CT, each run generates "source changed" events and re-triggers itself in
  an infinite loop (observed live: 4 consecutive full-suite runs). Tests should write only to
  temp/target, or the write paths need documenting so watchers can exclude them.

## Enhancements

- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

- [x] **Extractor structural facts query** -- `patterns` queries the typed, upstream-maintained structural registry without accepting raw grammar-specific tree-sitter expressions.
- [x] **AST-based complexity metrics** -- Persist upstream `complexity_metrics` and show per-symbol counts in `deep_dive`; hotspot ranking remains a separate product feature.
- [ ] **Function body hashing for duplication detection** -- `body_hash` is already stored per symbol but currently used only for change detection. Repurpose for near-duplicate function detection across a codebase (normalize whitespace/identifiers before hashing for fuzzier matching). Low priority.
