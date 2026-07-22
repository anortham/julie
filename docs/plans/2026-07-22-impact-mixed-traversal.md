# Impact Mixed Traversal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Evaluate and, only if the corpus passes its precision gates, promote opt-in `blast_radius` web mode from a one-hop web-edge add-on to one deterministic reverse breadth-first traversal across ordinary, identifier, HTTP, and SQL edges.

**Architecture:** Keep `BlastRadiusTool.mode` as the stable caller-facing interface: omitted/`default` remains the legacy graph and `web` selects the combined graph. Build a durable caller-facing evaluation corpus first, then make the existing impact walker policy-driven so ordinary and internal web edges share one visited set, frontier, depth budget, ranking path, and deterministic ordering.

**Tech Stack:** Rust, SQLite/rusqlite, `julie-core` web edges, `julie-tools` impact traversal, serde/TOML test corpus, cargo-nextest, Julie xtask verification tiers.

**Architecture Quality:** Affected modules are `julie_tools::impact::{mod,walk}` plus a test-only corpus adapter. The caller-facing interface remains `BlastRadiusTool { mode: Option<String>, max_depth, ... }`; no new tool parameter, output mode, language branch, or public traversal service is introduced. The test surface is the same `BlastRadiusTool::call_tool` interface callers use. Complexity stays local in the existing reverse walker; a duplicated mixed walker and reuse of forward `call_path` traversal are rejected because either would split impact budget, ranking, and deduplication semantics. Architecture risk is medium because an opt-in graph algorithm changes, while the default path is snapshot-locked.

## Global Constraints

- Build and inspect the curated evaluation corpus before changing production traversal behavior.
- `mode` omitted or `mode = "default"` must remain byte-identical to the pre-Phase-3 output for the locked corpus.
- `mode = "web"` remains opt-in; do not change the default or add an automatic promotion path.
- One reverse breadth-first walk must cover ordinary relationships, identifier fallback edges, and internal `http_call` / `sql_query` edges.
- Seed symbols begin in the visited set; each impacted symbol is emitted once at its shortest distance.
- External or ambiguously resolved web targets remain terminal and never become internal symbol candidates.
- `max_depth` applies once to the combined graph, not separately to ordinary and web edge families.
- Equal-distance candidates are deterministic across repeated runs; stored ordinary relationships keep precedence when the same source is reachable through multiple edge families at the same depth.
- Traversal stays language-agnostic. The corpus must cover more than one existing framework/language family without production language checks.
- Promotion hard gates are 100% of expected internal symbols found, zero unexpected internal symbol links, and unchanged default-mode output. Recall detail and latency relative to the default baseline are report-only metrics.
- Implementation files remain at or below 500 lines and test files at or below 1000 lines.
- Keep Julie's maintenance-mode/new-user positioning unchanged.
- Do not push, merge, publish, or release without explicit user approval.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, `xtask/test_tiers.toml`, and `docs/plans/verification-ledger-template.md`.

**Worker red/green scope:** `cargo nextest run --lib <exact_phase3_test_name>` for each corpus or traversal behavior, with the failing test observed before production edits and the same exact command rerun after the fix.

**Worker ceiling:** Exact named tests only during RED/GREEN. This is a no-delegation run, so the lead owns all broader commands.

**Worker gate invariant:** Corpus tests prove the fixture is complete and unambiguous; default snapshot proves the disabled path is unchanged; mixed tests prove shortest-distance, terminal-external, cycle/self-edge, deduplication, combined-depth, and deterministic-order behavior through `BlastRadiusTool::call_tool`.

**Lead affected-change scope:** `cargo xtask test bucket tools-blast-spillover`, then `cargo xtask test changed` after the coherent implementation batch.

**Branch gate:** `cargo fmt --check`, `cargo check`, `cargo xtask test dev`, and `cargo xtask test full` on the final code HEAD.

**Replay/metric evidence:** The Phase 3 scorecard prints expected-found count, unexpected-internal count, per-case recall, default and web latency samples, and default/web p50 and p95. Expected-found completeness, zero unexpected links, and the default snapshot are hard gates. Recall breakdown and latency are report-only and must not be labeled passing gates.

**Escalation triggers:** Any default snapshot difference, unexpected internal link, unstable repeated ordering, ambiguous web target promoted to a symbol, changed-file routing outside `tools-blast-spillover`, or test-tier timeout requires diagnosis before promotion. Search dogfood is not required unless implementation touches search/scoring/tokenization.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/plans/2026-07-22-impact-mixed-traversal-verification.md`. For the scorecard, also record the hard-gate metrics and report-only latency/recall metrics. Reuse evidence only when scope and exact HEAD match.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Curated corpus and pre-change baseline | None - serial | `fixtures/eval/blast_radius_mixed_traversal.toml`; `src/tests/mod.rs`; `src/tests/tools/blast_radius_mixed_traversal.rs`; `src/tests/tools/blast_radius_mixed_traversal/**`; `docs/plans/2026-07-22-impact-mixed-traversal-verification.md` | Yes | The corpus and legacy snapshot must exist and be measured before production behavior changes. |
| Task 2: Policy-driven combined reverse walk | None - serial | `crates/julie-tools/src/impact/walk.rs`; `crates/julie-tools/src/impact/mod.rs`; Task 1 test files | Yes | The implementation is accepted only against the reviewed corpus and locked default snapshot from Task 1. |
| Task 3: Promotion evidence and roadmap closeout | None - serial | `docs/plans/2026-07-22-impact-mixed-traversal-verification.md`; `.memories/briefs/julie-quality-improvement-roadmap.md`; this plan's checkboxes | Yes | Promotion and brief updates require the final implementation metrics and branch gates. |

### Task 1: Curated corpus and pre-change baseline

**Files:**
- Create: `fixtures/eval/blast_radius_mixed_traversal.toml`
- Create: `src/tests/tools/blast_radius_mixed_traversal.rs`
- Create: `src/tests/tools/blast_radius_mixed_traversal/corpus.rs`
- Create: `src/tests/tools/blast_radius_mixed_traversal/scorecard.rs`
- Create: `docs/plans/2026-07-22-impact-mixed-traversal-verification.md`
- Modify: `src/tests/mod.rs:54-65`

**Interfaces:**
- Consumes: `BlastRadiusTool::call_tool`, `SymbolDatabase`, `CanonicalWriteSet`, `Symbol`, `Relationship`, `WebEdge`, `WebEdgeKind`, `FakeToolContext`, and existing test builders.
- Produces: A versioned TOML corpus, a fixture loader, a locked default-output snapshot, and a repeatable scorecard command used by Tasks 2 and 3.

**Contract inputs:** The approved Phase 3 corpus categories from `docs/plans/2026-07-21-julie-improvement-roadmap-design.md`: ordinary caller → HTTP client → handler → downstream ordinary call; uniquely resolved SQL table; ambiguous/external terminal edges; cycles; self-calls; duplicate routes; combined depth; deterministic order; multiple framework/language families.

**File ownership:** `fixtures/eval/blast_radius_mixed_traversal.toml`; `src/tests/mod.rs`; `src/tests/tools/blast_radius_mixed_traversal.rs`; `src/tests/tools/blast_radius_mixed_traversal/**`; `docs/plans/2026-07-22-impact-mixed-traversal-verification.md`

**Serialization required:** Yes.

**Dependency reason:** The corpus and legacy snapshot must exist and be measured before production behavior changes.

**Step 1: Write the corpus completeness test**

Create a typed TOML loader whose cases name seed IDs, `max_depth`, expected internal symbol IDs, and explicitly terminal external endpoints. The corpus must include at least these two language-agnostic paths:

```text
page_loader --calls--> fetch_user --http_call--> show_user --calls--> save_user
report_page --calls--> load_report --sql_query--> users_table
```

Add unresolved HTTP and SQL edges, a self-edge, a two-node cycle, and a duplicate same-distance route. Validate that all referenced IDs exist, expected and terminal sets are disjoint, every case has a seed, and at least two language/framework families are present.

**Step 2: Run the completeness test**

Run: `cargo nextest run --lib phase3_mixed_traversal_corpus_is_complete`

Expected: PASS after the corpus and loader exist.

**Step 3: Lock the default-mode output**

Build the SQLite fixture from the corpus using the canonical write set plus `replace_all_web_edges`. Call `BlastRadiusTool` with `mode = None`, `include_tests = false`, and a fixed limit/depth. Capture the current compact output as an exact string assertion in `phase3_default_mode_matches_legacy_snapshot`.

Run: `cargo nextest run --lib phase3_default_mode_matches_legacy_snapshot`

Expected: PASS before production traversal changes.

**Step 4: Add and run the report-only baseline scorecard**

The scorecard must call the real tool in default and web modes, derive internal symbol IDs only from the corpus's known symbol labels, run repeated samples after a warm-up, and print one JSON report. It must not turn latency into a flaky wall-clock assertion.

Run: `cargo nextest run --lib phase3_mixed_traversal_scorecard --no-capture`

Expected: PASS for harness integrity while reporting the current web-mode mixed-path gaps and zero unexpected internal links. Create the verification ledger from the repository template and record the JSON before Task 2.

**Step 5: Apply commit mode**

- `serial-worker-commit`: checkpoint, commit the corpus/baseline slice as `test(impact): add mixed traversal evaluation corpus`, and record the SHA.

**Acceptance criteria:**
- [x] The corpus covers every approved category and at least two language/framework families.
- [x] The pre-change default compact output is locked byte-for-byte.
- [x] The pre-change scorecard records hard-gate counts and report-only latency/recall without claiming promotion.
- [x] No production behavior changes in this task.
- [x] Exact corpus, snapshot, and scorecard tests pass and the slice is committed.

### Task 2: Policy-driven combined reverse walk

**Files:**
- Modify: `crates/julie-tools/src/impact/walk.rs:55-323`
- Modify: `crates/julie-tools/src/impact/mod.rs:175-226`
- Modify: `src/tests/tools/blast_radius_mixed_traversal/scorecard.rs`
- Modify or create focused behavior tests under: `src/tests/tools/blast_radius_mixed_traversal/`

**Interfaces:**
- Consumes: Task 1 corpus/fixture, existing `walk_impacts_with_budget`, `walk_web_callers`, `WalkBudget`, and `BlastRadiusTool.mode` validation.
- Produces: A private impact traversal policy selected by `mode`, with the existing default wrapper preserved and web mode using the same BFS/ranking pipeline.

**Contract inputs:** Ordinary relationship candidates retain precedence at the same depth; web edges are only followed when `to_symbol_id` is internal; direct web callers remain available to the existing `Web callers` formatting section; high-impact candidates are deduplicated by symbol ID and shortest distance.

**File ownership:** `crates/julie-tools/src/impact/walk.rs`; `crates/julie-tools/src/impact/mod.rs`; Task 1 test files

**Serialization required:** Yes.

**Dependency reason:** The implementation is accepted only against the reviewed corpus and locked default snapshot from Task 1.

**Step 1: Write the failing caller-facing promotion tests**

Add exact tests that invoke `BlastRadiusTool::call_tool` in `web` mode and assert:

```rust
assert_case("http-chain", &["show_user", "fetch_user", "page_loader"]);
assert_case("sql-chain", &["load_report", "report_page"]);
assert_eq!(unexpected_internal_symbols, Vec::<String>::new());
```

Separate exact tests must prove combined `max_depth`, shortest-distance deduplication, cycles/self-calls, deterministic repeated output, and external-terminal behavior.

**Step 2: Verify RED against the one-hop implementation**

Run: `cargo nextest run --lib phase3_web_mode_traverses_http_mixed_path`

Expected: FAIL because the current one-hop web add-on cannot reach `page_loader` across ordinary → web → ordinary traversal.

**Step 3: Make the existing impact walker policy-driven**

Keep the legacy entry point as the default-policy wrapper and add one parent-private entry point for `run_with_db`:

```rust
#[derive(Clone, Copy)]
pub(super) enum ImpactTraversalPolicy {
    Default,
    Web,
}

pub fn walk_impacts_with_budget(...) -> Result<(Vec<ImpactCandidate>, WalkStats)> {
    walk_impacts_with_policy(..., ImpactTraversalPolicy::Default)
}

pub(super) fn walk_impacts_with_policy(
    db: &SymbolDatabase,
    seed_symbols: &[Symbol],
    max_depth: u32,
    budget: WalkBudget,
    policy: ImpactTraversalPolicy,
) -> Result<(Vec<ImpactCandidate>, WalkStats)> { ... }
```

At each BFS depth, load reverse ordinary relationships first, then identifier fallback edges, then internal reverse web edges only for `Web`. Merge by source symbol without replacing an existing ordinary candidate at the same depth. Use the existing visited set, frontier cap, reference-score lookup, `impact_order`, and ranking path for every edge family.

**Step 4: Route web mode through the unified walk**

In `run_with_db`, select `ImpactTraversalPolicy::Web` only for `mode = "web"`. Continue calling `walk_web_callers` for the direct provenance rows in the `Web callers` section, but remove the old post-walk candidate extension because the combined BFS now owns candidate discovery and distance.

**Step 5: Verify GREEN and edge semantics**

Run these exact tests one at a time:

```bash
cargo nextest run --lib phase3_web_mode_traverses_http_mixed_path
cargo nextest run --lib phase3_web_mode_traverses_sql_mixed_path
cargo nextest run --lib phase3_web_mode_keeps_external_edges_terminal
cargo nextest run --lib phase3_web_mode_deduplicates_cycles_at_shortest_distance
cargo nextest run --lib phase3_web_mode_applies_one_combined_depth_limit
cargo nextest run --lib phase3_web_mode_is_deterministic
cargo nextest run --lib phase3_default_mode_matches_legacy_snapshot
```

Expected: all PASS. The default snapshot must not be updated to accommodate an implementation difference; a mismatch is a defect to fix.

**Step 6: Run focused existing regressions**

Run:

```bash
cargo nextest run --lib impact_web_mode_lists_calling_frontend_symbols
cargo nextest run --lib impact_web_mode_lists_routines_querying_table
cargo nextest run --lib test_walk_impacts_caps_identifier_fanout_for_common_names
cargo nextest run --lib test_blast_radius_limit_bounds_depth_frontier
```

Expected: all PASS with existing direct-web provenance, fanout, and budget behavior preserved.

**Step 7: Apply commit mode**

- `serial-worker-commit`: checkpoint, commit the reviewed implementation as `feat(impact): traverse mixed graph in web mode`, and record the SHA.

**Acceptance criteria:**
- [x] Web mode finds every expected HTTP and SQL mixed path at the shortest distance.
- [x] External/ambiguous edges remain terminal with zero unexpected internal links.
- [x] Cycles, self-calls, duplicate routes, combined depth, and deterministic ordering pass exact tests.
- [x] Default mode remains byte-identical to the Task 1 snapshot.
- [x] Existing direct web caller provenance and ordinary walk budget tests pass.
- [x] `walk.rs` and every new test file stay within project line limits.
- [x] Exact RED/GREEN and focused regression evidence is recorded and the slice is committed.

### Task 3: Promotion evidence and roadmap closeout

**Files:**
- Modify: `docs/plans/2026-07-22-impact-mixed-traversal-verification.md`
- Modify: `.memories/briefs/julie-quality-improvement-roadmap.md`
- Modify: `docs/plans/2026-07-22-impact-mixed-traversal.md`

**Interfaces:**
- Consumes: Final corpus scorecard, exact tests, changed-file routing, xtask tiers, and final git/worktree state.
- Produces: An auditable promotion/rejection decision, exact-SHA verification ledger, completed plan checklist, and honest active brief.

**Contract inputs:** Promotion is allowed only if expected-found is complete, unexpected-internal is zero, and the default snapshot passes. Latency and detailed recall remain report-only. If any hard gate fails, preserve web mode's current one-hop behavior and document rejection instead of weakening the corpus.

**File ownership:** `docs/plans/2026-07-22-impact-mixed-traversal-verification.md`; `.memories/briefs/julie-quality-improvement-roadmap.md`; this plan's checkboxes

**Serialization required:** Yes.

**Dependency reason:** Promotion and brief updates require the final implementation metrics and branch gates.

**Step 1: Run and record the final scorecard**

Run: `cargo nextest run --lib phase3_mixed_traversal_scorecard --no-capture`

Expected hard gates: every expected internal symbol found; zero unexpected internal links; default snapshot still passing. Record per-case recall and default/web p50/p95 as report-only.

**Step 2: Run affected-change verification**

Run in order:

```bash
cargo fmt --check
cargo check
cargo xtask test bucket tools-blast-spillover
cargo xtask test changed
```

Expected: all PASS. `changed` must select the blast/spillover coverage for the production and `src/tests/tools/blast_radius...` paths.

**Live-code correction:** Task 2 was committed before this final gate, and `changed` intentionally maps only local working-tree changes, so the clean worktree produced a successful no-op instead of selecting a bucket. The directly selected `tools-blast-spillover` bucket provides the intended coverage and the ledger records the no-op without labeling it mapped coverage. Repository-wide `cargo fmt --check` also reproduces the already documented Phase 4 normalization debt; every Phase 3 Rust file passes the pinned formatter directly.

**Step 3: Inspect the final diff impact**

Use Miller `impact(git=true)` and inspect the modified traversal symbols. Confirm the stable tool interface, no default-mode caller expansion, and no unexpected dependency or language-specific branch.

**Step 4: Run branch gates on the final code HEAD**

Run one command at a time:

```bash
cargo xtask test dev
cargo xtask test full
```

Expected: all selected buckets PASS. Record warm, prebuild, and cold-wall timings with the exact commit SHA.

**Step 5: Close the durable records**

Write the verification ledger from `docs/plans/verification-ledger-template.md`, tick this plan's acceptance criteria, update the active Goldfish brief to mark Phase 3 promoted or rejected with evidence, and create a checkpoint before the final local documentation commit.

**Step 6: Final worktree-state check**

Record current path, branch, HEAD, `git status --short --branch`, `git worktree list`, and the status of the main and release worktrees. Do not push or integrate.

**Acceptance criteria:**
- [x] Final scorecard hard gates support promotion, or the feature is explicitly rejected without weakening expectations.
- [x] Report-only recall and latency are recorded honestly.
- [x] Focused, changed-routing outcome, dev, and full gates are recorded on exact SHAs; focused/dev/full pass and the clean-worktree changed no-op is explicit.
- [x] Miller final impact review finds no interface expansion or language-specific traversal branch.
- [x] Verification ledger, plan checklist, and active brief agree on Phase 3 status.
- [x] Final task worktree is clean after the closeout commit; no push, merge, publish, or release occurs.
