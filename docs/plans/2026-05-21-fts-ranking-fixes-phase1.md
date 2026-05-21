# FTS Ranking Fixes — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Close the measurable share of Julie's FTS ranking gap vs Eros's `lancedb-fts` that does NOT require unifying the target dispatch, and stand up the ablation + measurement harness that decides the scope of Phase 2's unification refactor.

**Architecture:** Three independent improvements landed in sequence, plus an ablation infrastructure. (1) Profile and fix the 9.5s standalone-cold-start latency on NL symbol queries. (2) Extend Julie's existing reranker to apply `title == query` / `basename == query` exact-match short-circuits across all three search targets (definitions, files, content), not just definitions. (3) Add runtime feature gates to disable English stemming and CamelCase token emission in `CodeTokenizer` so the bakeoff can A/B them. (4) Extend the `search_matrix` xtask harness to run ablations and emit comparable reports. Phase 1 does NOT collapse the symbol/file doc-type split — that is Phase 2, scoped after Phase 1's data lands.

**Tech Stack:** Rust, Tantivy 0.22, sqlite-vec (untouched in Phase 1), tracing for latency profiling, existing `xtask search_matrix` runner, `cargo nextest` for unit verification, `cargo xtask test bucket search_quality` / `dogfood` for regression.

**Architecture Quality:** This is **shared-invariant work** per `RAZORBACK.md` ("search ranking, scoring, tokenization, or query semantics"). Workers may not own search-ranking decisions unattended. All scoring constant changes, schema changes to the reranker contract, and tokenizer behavior gates require the lead to fix the contract before dispatch. Worker eligibility is restricted to coupled-implementation tier minimum. The main architecture risk is regressing the `definitions` target (Julie's strongest path) while extending the reranker to `files` and `content`; mitigated by Task 5's bakeoff + dogfood gate, which must show no regression on `definitions` queries before merging any reranker change.

**Spec source:** `docs/investigation/2026-05-21-fts-ranking-gap-vs-lancedb.md`

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` "Running Tests" + "Subagent & Worker Agent Test Rules" sections, `docs/TESTING_GUIDE.md`, `xtask test list`.

**Worker red/green scope:** `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`. Workers add or update one or more tests and verify only those tests.

**Worker ceiling:** Workers run their assigned tests only. They do NOT run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test dogfood`, or any tier-level command. Two test runs maximum per fix (red, then green).

**Worker gate invariant:** Each task lists the invariant the worker's narrow test must prove. Workers state the invariant in their report.

**Lead affected-change scope:** `cargo xtask test changed` after each task's worker reports green. Falls back to `cargo xtask test dev` if shared infrastructure moved (the xtask runner decides).

**Branch gate:** `cargo xtask test dev` plus `cargo xtask test dogfood` (search_quality bucket) before declaring Phase 1 complete. Search/scoring/tokenization changes always trigger `dogfood` per CLAUDE.md.

**Replay/metric evidence:** Task 5 is metric-evidence ownership (the bakeoff report). Hard gates: (a) no regression in `definitions`-target queries vs the current baseline, defined as top1 and MRR within ±1 absolute count of the pre-Phase-1 numbers on the same May-21 corpus; (b) `function display template` latency below 1s p95 in daemon mode after Task 1. Report-only metrics: per-category top1/top5/MRR deltas across the three categories that drive the gap, and ablation deltas for stemming-off and camel-emit-off variants.

**Escalation triggers:** Any change to `EXACT_TITLE_BOOST`, `PARTIAL_TITLE_BOOST`, `PATH_BOOST`, or kind-boost constants in `src/search/reranker.rs` must go through lead review even if the worker tier is otherwise eligible — these are tuned constants with cross-target impact. Two consecutive worker failures on the same task escalate to strategy tier.

**Assigned verification failure:** Workers stop and report when assigned verification fails. This plan does NOT authorize workers to update any gate; gate changes go through the lead.

**Verification ledger:** Use `docs/plans/verification-ledger-template.md`. Reuse only when the scope label matches AND the commit SHA matches HEAD exactly.

---

## Model Routing

**Project source of truth:** `RAZORBACK.md` (Julie repo root).

**Strategy tier:** Planning, architecture, decomposition, lead review, finding triage.
- Harness mapping: Claude Opus (this session). For Codex review gates, use `gpt-5.5 high`.

**Implementation tier:** Bounded worker tasks from a clear plan with no hidden invariants.
- Harness mapping: **Not eligible** for this plan — RAZORBACK.md explicitly excludes "search ranking, scoring, tokenization, or query semantics" from unattended implementation-tier work.

**Coupled implementation tier:** Bounded cross-file work after the lead has fixed the contract. **This is the default worker tier for Phase 1.**
- Harness mapping: Claude Sonnet high (for Claude Code Agent dispatch, use `sonnet` short name). For Codex, `gpt-5.5 medium` minimum, bump to `gpt-5.5 high` for hard-debugging tasks (Task 1) and constant-tuning tasks (Task 2).

**Mechanical tier:** Docs, fixtures, rote edits.
- Harness mapping: Claude Haiku. Only Task 5b (writing the report) is mechanical-eligible, and only after the data interpretation is decided by the lead.

**Gate-interpretation reviewer:** Plan-vs-diff review, failing-test triage.
- Harness mapping: Codex `gpt-5.5 high` if external review is requested. Default in-session reviewer is the lead (Opus).

**Escalation tier:** Subtle correctness, repeated failure, gate interpretation.
- Harness mapping: Claude Opus. For Codex, `gpt-5.5 high/xhigh`.

**Worker eligibility:** Coupled-implementation tier may own Tasks 1, 2, 3, 4 only after the lead confirms the contract (constant values, gate behavior, ablation env-var name) in this plan. Worker takes one task at a time. No worker spans multiple tasks.

**Escalation triggers:** Two failed reviews on the same task → escalation tier. Any worker proposal to change the reranker scoring constants in `src/search/reranker.rs:21-30` → lead approval required (the worker cannot decide). Latency regression discovered during Task 1 in a non-target codepath → escalation tier.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates. Task 5a (running the bakeoff) is metric-evidence work and stays with the lead or the gate-interpretation reviewer; only the report-writing of Task 5b is mechanical-eligible, and only with concrete numbers already in hand.

**Unsupported harness behavior:** Claude Code's Agent tool accepts only `opus | sonnet | haiku`. Translate accordingly.

---

## File Structure

**Modified:**
- `src/search/expansion.rs` — Latency investigation may add a bounded-expansion safeguard. (Task 1)
- `src/search/index.rs:713-790` (`search_content`) — Wire reranker if needed. (Task 2)
- `src/search/index.rs:792-904` (`search_files`) — Add title/basename short-circuit application. (Task 2)
- `src/search/reranker.rs` — Generalize exact-title/basename boost so it can apply to file and content results, not only definition `Candidate`. (Task 2)
- `src/search/tokenizer.rs:97-104` (`from_language_configs`) and `:186-315` (`tokenize_code`) — Add runtime env-var gates `JULIE_ABLATE_STEMMING` and `JULIE_ABLATE_CAMEL_EMIT`. (Task 3)
- `src/tools/search/text_search.rs:408-452` (`apply_reranker_to_content_results`) — Extend if reranker contract changes. (Task 2)
- `xtask/src/search_matrix.rs` — Add ablation mode flag; record ablation label in `SearchMatrixBaselineExecution`. (Task 4)
- `xtask/src/search_matrix_report.rs` (likely existing; verify path) — Render ablation diffs. (Task 4)

**Created:**
- `src/tests/tools/search/title_exact_boost_tests.rs` — Unit tests for the cross-target title-exact short-circuit. (Task 2)
- `src/tests/tools/search/tokenizer_ablation_tests.rs` — Unit tests proving env-var gates flip behavior. (Task 3)
- `src/tests/integration/search_matrix_ablation.rs` — Integration test that the ablation flag flows end-to-end. (Task 4)
- `docs/investigation/2026-05-21-fts-fixes-phase1-results.md` — Bakeoff result writeup. (Task 5b)

**Untouched in Phase 1 (Phase 2 territory):**
- `src/search/schema.rs` — Schema unification is Phase 2.
- `src/search/index.rs:1403-1518` (`build_annotation_symbol_query`) — Definitions query builder. May need micro-tweak for title short-circuit, but the unified-target rewrite is Phase 2.
- `src/database/symbols/search.rs` — Database query shape is Phase 2.

---

## Tasks

### Task 1: Profile and fix `expand_query_terms` latency

**Files:**
- Investigate: `src/search/expansion.rs:19` (`expand_query_terms`), `src/search/index.rs:1287-1328` (`annotation_context_terms`), `src/search/index.rs:614-705` (`search_symbols_relaxed`)
- Modify: `src/search/expansion.rs` and/or `src/search/index.rs` depending on root cause
- Test: `src/tests/tools/search/<existing or new>` — latency assertion against a fixture repo

**What to build:** The bakeoff measured `function display template` taking 9.5s in standalone mode against an Alamofire-like repo. Suspects: (a) combinatorial expansion in `expand_query_terms` blowing up alias × normalized × original term groups, (b) AND-then-OR fallback in `search_symbols_relaxed` re-running the entire query, (c) cold-start cost of `julie-server --standalone` per query (not a code bug, but verify daemon mode is sub-second on the same query).

First investigate with `tracing` instrumentation. Then fix the root cause. If the cold-start is the dominant factor and daemon mode is fine, document that finding and either bound the expansion as a defensive measure or close the task with a daemon-mode-required note in `docs/SEARCH_FLOW.md`.

**Approach:**
- Add `tracing` spans around `expand_query_terms`, `build_annotation_symbol_query`, the AND-pass, and the relaxed OR-pass in `search_symbols_relaxed`. Use `tracing-tree` or `RUST_LOG=debug` against a debug build to capture timings.
- Reproduce: `./target/debug/julie-server search "function display template" --target definitions --workspace <alamofire-fixture-path> --standalone --json` after a debug build. Capture span timings.
- If `expand_query_terms` is the hot path: cap `MAX_ADDED_TERMS` (currently 8) tighter for queries with ≥4 tokens, OR short-circuit alias expansion when query has no code-like tokens.
- If the AND-then-OR fallback is double-running: see whether `search_symbols_relaxed` can reuse the candidate set from the AND pass instead of re-querying.
- Worker writes a `cargo nextest` test that exercises the slow path against an in-tree fixture and asserts wall-clock under a generous bound (e.g., 1s in debug, document the bound).

**Acceptance criteria:**
- [ ] `function display template` against an Alamofire-shape fixture takes < 1.5s in debug daemon mode (target: < 500ms in release).
- [ ] No regression in single-token symbol queries (top10 query latencies within ±20% on the existing bakeoff baseline).
- [ ] Worker-scope narrow test passes and is added to `src/tests/tools/search/` covering the slow-query path.
- [ ] Invariant proved by the test: "NL multi-token symbol-intent queries do not trigger combinatorial expansion." State this invariant in the worker report.
- [ ] If the root cause is non-code (cold-start), the task instead lands a documentation note in `docs/SEARCH_FLOW.md` and the test asserts daemon-mode latency, not standalone.

---

### Task 2: Title-exact and basename-exact short-circuit across all targets

**Files:**
- Modify: `src/search/reranker.rs:189-200` (`kind_boost`), `:217-237` (`rerank`), `:268-297` (`rerank_content_score`), `:327-398` (`score_symbol`)
- Modify: `src/search/index.rs:792-904` (`search_files`) — wire short-circuit before `rank_file_search_result`
- Modify: `src/tools/search/text_search.rs:408-452` (`apply_reranker_to_content_results`) — apply title-exact across content hits (lift symbol title from same file via DB lookup)
- Create: `src/tests/tools/search/title_exact_boost_tests.rs`

**What to build:** Julie's reranker already has `EXACT_TITLE_BOOST: f32 = 100.0`, `PARTIAL_TITLE_BOOST: f32 = 50.0`, `PATH_BOOST: f32 = 40.0` — exactly Eros's `_field_score` constants. The gap is that these only fire on the `definitions` path. Files and content paths skip them. Extend coverage so:

1. **Files path:** If `query == basename(file_path)` (case-insensitive, stripped of extension), the file ranks first regardless of BM25. Implement as a post-`search_files` reorder, not a query-builder change. Reuse the constant `EXACT_TITLE_BOOST + kind_boost(SymbolKind::Module) = 100.0 + kind_boost_for_file` or define a new `BASENAME_EXACT_FILE_BOOST` matching Eros's `+120`.

2. **Content path:** For each content hit, look up the symbols in that file via `db.find_symbols_in_file(path)` (batched: one query for the file_path set, not per-hit). If any symbol's name equals the query (case-insensitive), apply +100 to that hit's score. Cap the per-hit lookup at the top N pre-rerank candidates to keep cost bounded.

3. **Definitions path:** Already has `promote_exact_name_matches` in `scoring.rs:530`. Verify it fires before the multi-field BM25 ranking dominates. If `displayTemplate` query against Alamofire's duplicate jazzy.search.js copies isn't ranking the title-exact match first, the bug is here — investigate and document why `promote_exact_name_matches` isn't dominating, then fix.

**Approach:**
- Read `promote_exact_name_matches` (scoring.rs:530-586) and `rerank` (reranker.rs:217) to understand the current order of operations.
- Decide once: should the cross-target exact-match be a separate post-filter that runs before any per-target reranker, or a per-path extension? Lead's call before dispatching the worker: **per-path extension** — keeps each target's behavior locally inspectable.
- Reuse existing constants. Do NOT introduce new tuning constants without lead approval per the escalation trigger.
- The DB lookup for content-path symbol titles: add a method `SymbolDatabase::titles_for_files(paths: &[&str]) -> HashMap<String, Vec<String>>` if it doesn't exist, returning lowercase names per file. Batched.
- Tests in `title_exact_boost_tests.rs` must include: (a) `displayTemplate` against an in-tree fixture with multiple files of that name, asserting the file-doc copy with the matching title ranks first; (b) `test_requested_redirect` against a fixture with a `res.location.js`-style file (basename mismatched, body symbol matched), asserting it beats a basename-token-sharing distractor like `res.redirect.js`; (c) regression: `FastSearchTool` against `definitions` returns the same top-1 as before for a single-token exact symbol query.

**Acceptance criteria:**
- [ ] `displayTemplate`-style query: file containing a symbol literally named `displayTemplate` ranks above duplicate files with identical content but no title match. Test asserts rank-1 in the fixture.
- [ ] `test requested redirect`-style query against `files` target: file whose body contains a symbol matching the camelCased query ranks above a file whose basename shares one query token. Test asserts rank-1.
- [ ] No regression in existing `definitions` exact-symbol-lookup tests (run `src/tests/tools/search/` narrow suite).
- [ ] Worker-scope tests pass.
- [ ] Invariant proved: "Title-exact and basename-exact matches dominate BM25 score across all three search targets." State this in the worker report.
- [ ] All existing scoring/reranker constants in `reranker.rs:21-30` unchanged. (If the worker wants to change one, escalate to lead.)

---

### Task 3: Tokenizer ablation gates

**Files:**
- Modify: `src/search/tokenizer.rs:186-315` (`tokenize_code`), `:489-518` (`split_camel_case`)
- Modify: `src/search/tokenizer.rs:42-51` (`CodeTokenizer::new`) or `:97-104` (`from_language_configs`) to read env vars once at construction
- Create: `src/tests/tools/search/tokenizer_ablation_tests.rs`

**What to build:** Add two runtime gates that disable parts of the aggressive tokenization to enable A/B in the bakeoff:

1. `JULIE_ABLATE_STEMMING=1` — `ENGLISH_STEMMER.stem(...)` returns input unchanged (or stemming is skipped at emit time). Documents whose tokens were stemmed at index time will not benefit, so this ablation only meaningfully shifts query-time behavior. Document this limitation in the test.
2. `JULIE_ABLATE_CAMEL_EMIT=1` — `split_camel_case` returns the input as a single token. The full identifier stays, the parts are not emitted. Same caveat: index-time tokens are already split for documents indexed without the flag.

**Critical:** Because Julie's compat marker (`TokenizerCompatibilitySignature` at `tokenizer.rs:34-39`) includes the tokenizer configuration, changing tokenizer behavior at runtime will trigger an index rebuild on the next open. For the ablation to be measurable, the bakeoff harness in Task 4 must do a clean index rebuild with the env var set, then query. Document this in the ablation test's module docstring.

**Approach:**
- Read env vars in `CodeTokenizer::new` (or a new constructor `CodeTokenizer::with_env_overrides`). Store as `bool` fields on `CodeTokenizer`.
- Branch in `tokenize_code` at the stemmer-call and at the camel-split-emit step. Existing behavior is the default when env vars are unset or `0`.
- Tokenizer signature must reflect the ablation state so the compat marker forces a rebuild. Add two bool fields to `TokenizerCompatibilitySignature`.
- Tests verify: (a) with default env, `getUserData` tokenizes to `[getUserData, get, user, data]`; (b) with `JULIE_ABLATE_CAMEL_EMIT=1`, it tokenizes to `[getUserData]` only; (c) with default env, `running` stems to `run`; (d) with `JULIE_ABLATE_STEMMING=1`, `running` stays `running`.

**Acceptance criteria:**
- [ ] Two env vars implemented, defaults match current behavior, ablation gates change behavior as described.
- [ ] `TokenizerCompatibilitySignature` includes ablation state so index rebuild is forced when toggling.
- [ ] Unit tests in `tokenizer_ablation_tests.rs` cover all four combinations (stemming on/off × camel on/off) at the `tokenize_code` level.
- [ ] No change to existing test results for the default (env unset) path. Run existing `src/tests/tools/search/` and tokenizer tests narrow.
- [ ] Worker-scope tests pass.
- [ ] Invariant proved: "Tokenizer behavior is unchanged when ablation env vars are unset; flipping each env var alters output deterministically and updates the compat signature." State this in the worker report.

---

### Task 4: Extend `search_matrix` xtask for ablation

**Files:**
- Modify: `xtask/src/search_matrix.rs:121-194` (`run_search_matrix_command`, `run_search_matrix_baseline_with_home`) — add `ablation` parameter
- Modify: `xtask/src/cli.rs` (or wherever `SearchMatrixCommand` is defined) — surface a `--ablation <none|no-stemming|no-camel|both>` flag
- Modify: `xtask/src/search_matrix.rs:68-90` (`SearchMatrixBaselineReport`, `SearchMatrixBaselineExecution`) — add `ablation_label: String` field
- Modify: `xtask/src/search_matrix_report.rs` if it exists, else create a small helper that diffs two `SearchMatrixBaselineReport`s by case_id
- Create: `src/tests/integration/search_matrix_ablation.rs` — assert the flag flows through and the report contains the label

**What to build:** The bakeoff runner already exists. Extend it so a single command can run baseline + two ablations (`no-stemming`, `no-camel`) + both, producing three or four labeled reports per invocation. The runner must set the relevant env var before kicking off the workspace pool, force a full reindex (because tokenizer signature changed), run the query set, and capture results with an `ablation_label` for downstream diffing.

**Approach:**
- Add `Ablation` enum to `xtask/src/search_matrix.rs` matching the four states: `None`, `NoStemming`, `NoCamel`, `Both`.
- Modify `run_baseline_async` to accept the `Ablation` and set env vars before instantiating workspaces.
- Force a workspace reindex per ablation run (the tokenizer signature change should trigger it automatically; verify and add a `--force` if needed).
- Output one JSON report per ablation, named `<timestamp>-<ablation_label>.json` in the same directory as today's reports.
- Helper that takes two report paths and prints a side-by-side per-case top-rank diff to stdout (or writes a markdown table).
- Integration test in `src/tests/integration/search_matrix_ablation.rs` uses a tiny fixture corpus and asserts: (a) the CLI accepts the flag, (b) the output JSON includes `ablation_label`, (c) different ablation labels produce different top-rank orderings on at least one fixture query (smoke test, not a quality assertion).

**Acceptance criteria:**
- [ ] `cargo xtask search-matrix run --profile <name> --ablation <variant>` works for all four variants.
- [ ] Each run writes a labeled JSON report.
- [ ] `ablation_label` is present in `SearchMatrixBaselineExecution`.
- [ ] Integration test passes.
- [ ] Worker-scope tests pass.
- [ ] Invariant proved: "Ablation flag flows from CLI through workspace setup to per-execution report records, with the correct env var set during reindex." State this in the worker report.

---

### Task 5: Bakeoff run and results report (LEAD-OWNED)

**This task is metric-evidence work and is owned by the lead, not delegated to a coupled-implementation worker.** Task 5b (the docs writeup) may be mechanical-tier ONLY after the lead has decided what the numbers mean.

**Files:**
- Run: `cargo xtask search-matrix run` four times (baseline, no-stemming, no-camel, both) against the May-21 multi-lang corpus
- Compare: against the pre-Phase-1 baseline artifact at `/Users/murphy/.eros-eval/eval/bakeoff/20260521T202136Z-956e68b80b0c.json` (Julie subset)
- Create: `docs/investigation/2026-05-21-fts-fixes-phase1-results.md`

**Step 1 (lead):** Run the four ablation passes. Record commit SHA, ablation label, top1/top5/MRR per category, and per-category latency p50/p95 for each.

**Step 2 (lead, gate interpretation):** Decide whether Tasks 1, 2, 3 individually moved the needle. Hard gates:
- No regression in `definitions`-target queries (top1 and MRR within ±1 absolute count on the same 406 queries / 18 repos).
- `function display template` latency below 1s p95 in daemon mode.

**Step 3 (lead):** Decide Phase 2 scope. If title-exact + basename-exact (Task 2) closed most of Pattern A losses on its own, Phase 2's unification can focus on Patterns B and C. If the ablations show stemming or camel-emit is a net negative, Phase 2's unified tokenizer should drop those by default. If everything was within noise, Phase 2 must do the full unification regardless.

**Step 4 (mechanical-eligible, lead-decided content):** Write `docs/investigation/2026-05-21-fts-fixes-phase1-results.md` with the four ablation reports' numbers, the decided interpretation, and a one-paragraph scope statement for the Phase 2 plan.

**Acceptance criteria:**
- [ ] Four labeled JSON reports exist on disk and are referenced by absolute path in the results doc.
- [ ] Hard gates evaluated explicitly in the results doc.
- [ ] Phase 2 scope statement included.
- [ ] Lead signs off in the doc with their decision rationale.

---

## Sequencing and Gates

Tasks 1, 2, 3 are independent in file scope and may dispatch in parallel **once the lead has confirmed the worker prompts** for each. Task 4 depends on Task 3 (ablation gates exist before the harness can flip them). Task 5 depends on Tasks 1, 2, 3, 4 being merged.

**Per-task gate (lead):** After each worker reports green, lead runs `cargo xtask test changed` against the relevant subsystem. If it falls back to `dev`, accept and run `dev`.

**Pre-branch-gate (lead):** Before Task 5 begins, `cargo xtask test dev` plus `cargo xtask test dogfood` must pass.

**Phase 1 done:** Task 5's results doc is committed, hard gates are met, Phase 2 scope statement is written.

---

## What is explicitly NOT in Phase 1

- Collapsing `SymbolDocument` and `FileDocument` into a single `kind`-discriminated doc type (Phase 2).
- Unified target dispatch (Phase 2; possibly informed by Task 2's results — if cross-target reranker already closes most Pattern B/C losses, the unification scope shrinks).
- The RRF×200 rescale removal (`src/tools/search/text_search.rs:512`) (Phase 2, dependent on unification).
- Any changes to embedding behavior (per user direction 2026-05-21: embeddings stay until we've measured Phase 1 + Phase 2 against test-intent queries).
- Tuning the reranker scoring constants (`reranker.rs:21-30`) — these may move in Phase 2 if the unified scoring path needs different magnitudes, but Phase 1 reuses them as-is.
