# Search Quality Recovery Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development (on Claude Code) or razorback:subagent-driven-development (elsewhere) to implement this plan. Fall back to razorback:executing-plans for single-task or tightly-sequential plans.

**Goal:** Reduce live `fast_search(search_target="content")` zero-hit rate from the current `33% to 36%` band to `<= 20%`, cut without-recourse to `<= 8%`, and do it by fixing proven causes on `main` instead of piling more fallback behavior onto search.

**Architecture:** Stage the recovery around live telemetry. First remove common `file_pattern` separator mistakes, then split scoped zero-hits into `NoInScopeCandidates` versus `CandidateStarvation`, then repair scoped retrieval in `line_mode_matches`, then shrink `LineMatchMiss` by aligning line verification with Tantivy tokenization. Keep `zero_hit_reason` as the coarse stage field and add a scoped sub-diagnostic instead of mutating the whole trace model again.

**Patch from review:** The implementation must make three design choices explicit before code starts:

- `file_pattern_diagnostic` is the root-cause field for scoped `file_pattern` issues. It owns `WhitespaceSeparatedMultiGlob`, `NoInScopeCandidates`, and `CandidateStarvation`.
- `hint_kind` stays the user-facing, single-value hint channel. When more than one hint could apply, precedence is:
  1. syntax hint
  2. out-of-scope hint
  3. multi-token hint
- Task 3 cannot rely on "widen later" unless the scoped fast path stops starting at the current `max(limit * 100, 500).min(1000)` window. The adaptive loop must start from a smaller scoped window and widen toward a larger hard cap, or add an index helper that supports repeated larger windows cleanly.

**Tech Stack:** Rust, Tantivy, globset, SQLite telemetry, existing search-quality dogfood fixtures.

**Design doc:** `docs/plans/2026-04-21-search-quality-recovery-design.md`

---

## Execution Order and Gates

**Gate 0, rollout and fresh baseline capture**

- Build `main` in release, restart the daemon or client, and let the new `zero_hit_reason` and `hint_kind` fields populate `~/.julie/daemon.db`.
- Let fresh telemetry collect while Batch 1 is in flight.
- Capture a fresh 24h baseline before judging Task 3 or Task 4. The current live baseline is the pre-rollout query we already ran, not a post-merge measurement.

**Batch 1**

- Task 1, separator recovery and syntax hinting
- Task 2, scoped-miss diagnostic split

**Batch 2**

- Task 3, adaptive scoped fetch for `CandidateStarvation`
- Task 4, out-of-scope content hint, only if Task 2 shows a live `NoInScopeCandidates` bucket worth targeting

**Batch 3**

- Task 5, line verifier repair for `LineMatchMiss`

**Batch 4**

- Task 6, live validation and diagnosis update

**TDD expectation:** Every task follows RED, GREEN, commit. Write the failing test first, verify the narrow test fails, land the smallest change that makes it pass, then run `cargo xtask test dev` and `cargo xtask test dogfood` once per completed batch.

---

## File Map

- `src/tools/search/query.rs`
  `file_pattern` parsing and line verification helpers.
- `src/tools/search/line_mode.rs`
  Live content-search pipeline, stage counters, scoped filtering, and line extraction.
- `src/tools/search/trace.rs`
  Search trace model, zero-hit fields, and new scoped diagnostic enum.
- `src/tools/search/hint_formatter.rs`
  User-facing zero-hit hints.
- `src/tools/search/mod.rs`
  Final tool output formatting and hint insertion.
- `src/handler/search_telemetry.rs`
  Persisted `fast_search` telemetry payload.
- `src/tests/tools/search/file_pattern_tests.rs`
  Parser regressions and separator behavior.
- `src/tests/tools/search/line_mode_or_fallback_tests.rs`
  Scoped content-search behavior, fetch-window cases, and stage counter coverage.
- `src/tests/tools/search/zero_hit_reason_tests.rs`
  End-to-end live-path attribution coverage.
- `src/tests/tools/search/promotion_tests.rs`
  Trace enum serialization and hint formatter coverage.
- `src/tests/tools/search/zero_hit_reason_propagation_tests.rs`
  Propagation from `line_mode_matches` into public trace metadata.
- `src/tests/core/handler_telemetry.rs`
  Trace serialization coverage.
- `src/tests/integration/zero_hit_replay_tests.rs`
  Replay and diagnosis reporting that must learn the new diagnostic field.
- `src/tests/tools/search_quality/`
  Dogfood regressions for search behavior that matters in Julie’s own repo.
- `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md`
  Existing diagnosis artifact to extend with post-recovery measurements.

---

## Task 1: Recover `file_pattern` separator mistakes

**Files:**
- Modify: `src/tools/search/query.rs`
- Modify: `src/tools/search/trace.rs`
- Modify: `src/tools/search/hint_formatter.rs`
- Modify: `src/tools/search/mod.rs`
- Test: `src/tests/tools/search/file_pattern_tests.rs`
- Test: `src/tests/core/handler_telemetry.rs`

**What to build:** Support top-level `|` as an OR separator in `file_pattern` parsing, keep comma and brace handling intact, and surface a dedicated hint when a pattern looks like whitespace-separated globs such as `src/** tests/**`.

**Approach:**
- Extend the top-level separator splitter in `query.rs` to treat `|` the same way it treats top-level commas, while still respecting brace depth.
- Do not split on whitespace. A literal filename glob with spaces must stay valid.
- Add a new `FilePatternDiagnostic` variant for whitespace-separated multi-glob mistakes and persist it on the trace as the root cause.
- Add a matching `HintKind` variant only for the user-facing response, and fire it only when the diagnostic is `WhitespaceSeparatedMultiGlob`.
- Keep telemetry shape explicit. If a hint fires, it must persist through `search_telemetry.rs` consumers via the trace model.

**Acceptance criteria:**
- [ ] `src/**|tests/**` matches files in either tree
- [ ] `src/**|{tests/**,docs/**}` still respects brace nesting and top-level splitting
- [ ] `**/file name.rs` still matches a literal path with spaces
- [ ] `src/** tests/**` stays invalid as a pattern, but zero-hit output carries the dedicated syntax hint
- [ ] Trace metadata records both `file_pattern_diagnostic=whitespace_separated_multi_glob` and the corresponding hint when that case fires
- [ ] Trace serialization includes the new hint kind
- [ ] Narrow tests pass, committed

## Task 2: Split scoped zero-hits into real causes

**Files:**
- Modify: `src/tools/search/trace.rs`
- Modify: `src/tools/search/line_mode.rs`
- Modify: `src/handler/search_telemetry.rs`
- Test: `src/tests/tools/search/line_mode_or_fallback_tests.rs`
- Test: `src/tests/tools/search/zero_hit_reason_tests.rs`
- Test: `src/tests/core/handler_telemetry.rs`

**What to build:** Keep `zero_hit_reason` coarse, but add a new scoped sub-diagnostic for content zero-hits with `file_pattern` so we can tell apart `NoInScopeCandidates` and `CandidateStarvation`.

**Approach:**
- Add a new trace field in `trace.rs`, `file_pattern_diagnostic`, rather than expanding `ZeroHitReason`.
- In `line_mode_matches`, when a scoped content search zero-hits and the fetched window contains no in-scope candidates, run a bounded wider probe against `search_content`.
- Classify the outcome as:
  - `NoInScopeCandidates` when the wider probe still finds no candidate paths matching the requested scope
  - `CandidateStarvation` when the wider probe finds in-scope candidate paths that the first window missed
- Keep `WhitespaceSeparatedMultiGlob` in the same enum so all `file_pattern`-specific root causes live in one persisted field.
- Persist this diagnostic in `search_telemetry.rs`.

**Acceptance criteria:**
- [ ] A scoped zero-hit with no matching paths in either the initial or wider probe records `NoInScopeCandidates`
- [ ] A scoped zero-hit where the initial window misses in-scope files but the wider probe finds them records `CandidateStarvation`
- [ ] Existing `zero_hit_reason` values stay unchanged for other pipeline stages
- [ ] Trace JSON includes the new field only when relevant
- [ ] `promotion_tests.rs`, `zero_hit_reason_propagation_tests.rs`, and telemetry tests are updated for the new trace field
- [ ] Narrow tests pass, committed

## Task 3: Fix `CandidateStarvation` in the live content path

**Files:**
- Modify: `src/tools/search/line_mode.rs`
- Modify: `src/search/index.rs` if a small search helper is needed for clean repeated windows
- Test: `src/tests/tools/search/line_mode_or_fallback_tests.rs`
- Test: `src/tests/tools/search/zero_hit_reason_tests.rs`
- Test: `src/tests/tools/search_quality/`

**What to build:** Replace the one-shot fixed fetch window in `line_mode_matches` with an adaptive scoped fetch loop that widens only when the current window cannot satisfy the requested scope.

**Approach:**
- Keep the current fast path for unscoped content queries.
- For scoped queries, stop using the current "start near the cap" fetch rule. Start from a smaller scoped window that can expose starvation, apply scope and verification, and stop early if enough hits are found.
- If the current window produces no in-scope candidates and Task 2 would classify the miss as `CandidateStarvation`, rerun `search_content` with a larger window.
- Use a bounded growth rule, such as exponential widening capped at a hard limit, so this path does not turn into an unbounded scan.
- If repeated widening needs cleaner plumbing than a raw `limit` loop, add a small helper in `src/search/index.rs`; do not copy-paste the search call across both workspace branches.
- Stop widening once:
  - enough hits are found
  - in-scope candidates were found and exhausted through line verification
  - the hard cap is reached

**Acceptance criteria:**
- [ ] A fixture where the first ranked window is out-of-scope but later ranked files are in-scope now returns the in-scope hit
- [ ] Unscoped content searches keep their current behavior and do not widen
- [ ] Scoped searches stop widening after the hard cap
- [ ] Shared primary/target workspace logic lives in a helper instead of duplicating the widening loop twice
- [ ] `cargo xtask test dogfood` stays green after the change
- [ ] Tests pass, committed

## Task 4: Add an out-of-scope content hint, if live telemetry earns it

**Files:**
- Modify: `src/tools/search/trace.rs`
- Modify: `src/tools/search/hint_formatter.rs`
- Modify: `src/tools/search/mod.rs`
- Modify: `src/handler/search_telemetry.rs`
- Test: `src/tests/tools/search/zero_hit_reason_tests.rs`
- Test: `src/tests/core/handler_telemetry.rs`

**Depends on:** Task 2 showing a meaningful live `NoInScopeCandidates` bucket after Gate 0 rollout.

**What to build:** When a scoped content search dies because no candidate paths exist inside the requested tree, prepend a targeted hint that tells the caller to broaden or remove the `file_pattern`.

**Approach:**
- Add a new `HintKind` for out-of-scope content misses.
- Fire it only when Task 2 classified the miss as `NoInScopeCandidates`.
- Respect the shared hint precedence: syntax hint beats out-of-scope hint, out-of-scope hint beats multi-token hint.
- Do not auto-unscope in this task. The hint is the product. Hidden behavior changes wait until the telemetry proves they are safe.
- Include the requested `file_pattern` in the hint text so the caller can see what scoped them out.

**Acceptance criteria:**
- [ ] `NoInScopeCandidates` zero-hits prepend the out-of-scope hint
- [ ] `CandidateStarvation` zero-hits do not use the out-of-scope hint
- [ ] Whitespace-separated syntax mistakes still use the syntax hint instead of the out-of-scope hint
- [ ] Without-recourse measurement can read the new hint from persisted trace metadata
- [ ] Narrow tests pass, committed

## Task 5: Repair `LineMatchMiss` by aligning line verification with Tantivy

**Files:**
- Modify: `src/tools/search/query.rs`
- Modify: `src/tools/search/line_mode.rs`
- Test: `src/tests/tools/search/line_mode_or_fallback_tests.rs`
- Test: `src/tests/tools/search/zero_hit_reason_tests.rs`
- Test: `src/tests/tools/search_quality/`

**What to build:** Replace the current lowercase `contains()` verifier for token strategies with token-aware line matching that tracks Tantivy tokenization more closely.

**Approach:**
- Keep literal substring behavior for quoted, punctuation-heavy, and single-token substring queries where raw text matching is still the right tool.
- For token strategies, tokenize each line with the same `CodeTokenizer` path used by hint formatting and compare token sets instead of raw lowercase substrings.
- Preserve the current `FileLevel` semantics, "tokens can appear across the file, line output may show any matching line", while making the per-line decision token-aware.
- Use real `LineMatchMiss` rows from the replay fixture or dogfood cases as regression inputs, not synthetic smoke tests alone.

**Acceptance criteria:**
- [ ] A line that matches under Tantivy tokenization but not under raw lowercase substring now survives verification
- [ ] Quoted or punctuation-heavy literal queries still use literal matching
- [ ] Existing line-mode formatting stays unchanged
- [ ] `cargo xtask test dogfood` stays green after the change
- [ ] Tests pass, committed

## Task 6: Validate against live telemetry and update the diagnosis doc

**Files:**
- Modify: `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md`
- Test: `src/tests/integration/zero_hit_replay_tests.rs` only as a diagnostic check, not the primary success metric

**What to build:** Re-run the daemon telemetry queries after Tasks 1 through 5 land, then update the diagnosis doc with the new live 24h and 7d breakdowns plus a short note on what moved.

**Approach:**
- Query `~/.julie/daemon.db` after fresh post-rollout traffic, grouping content zero-hits by `zero_hit_reason`, `file_pattern_diagnostic`, and `hint_kind`.
- Record before and after numbers in the diagnosis doc.
- Re-run the historical replay harness as a smoke check and call it what it is, a regression fixture, not the primary KPI.

**Acceptance criteria:**
- [ ] Diagnosis doc records fresh live 24h and 7d content zero-hit rates after the recovery tasks
- [ ] Diagnosis doc shows the new `file_pattern_diagnostic` split
- [ ] The recovery is judged on live daemon telemetry, not only on the historical replay fixture
- [ ] Tests and telemetry queries pass, committed

---

## Done Means

- `main` has the separator recovery, scoped-miss split, starvation fix, and line-match repair
- live daemon telemetry shows content zero-hit at or below `20%`
- live without-recourse is at or below `8%`
- the diagnosis doc explains the remaining misses in plain language
- no promotion behavior was reintroduced to get there
