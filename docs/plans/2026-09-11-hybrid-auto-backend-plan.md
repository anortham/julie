# Hybrid Auto Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** When `fast_search` gets no `backend`, run hybrid for natural-language-shaped queries whenever the workspace has ready vectors, and lexical otherwise, with no visible change for every other query shape.

**Architecture:** The decision lives in `SearchBackend::resolve`, which gains the query and returns `Hybrid` with `explicit: false` for NL-shaped, non-path queries. `execute_search_unified` already handles missing vectors and `Required` for non-lexical backends; it gains one fall-through so a zero-hit auto-hybrid pass runs lexical instead of returning empty. The auto path reads the already-live provider (`embedding_provider`) and never waits for lazy init, because the production `ensure_embedding_provider` ignores its timeout and can block up to 30s; only an explicit `semantic`/`hybrid` request pays that wait. Telemetry gains a `backend_auto` field so phase 6b can count how often auto picked hybrid.

**Tech Stack:** Rust (`julie-tools`, `julie-index`), Python 3 harnesses under `docs/eval/`.

**Architecture Quality:** Approved shape: one resolver function owns the auto rule; the execution path gains no new branch beyond the zero-hit fall-through. Risk: the hybrid pass is symbol-only, so an NL query that used to return line hits from prose now returns symbol hits. The scorecard and head-to-head rerun in Task 2 measure that trade.

## Global Constraints

- Design: `docs/plans/2026-09-11-hybrid-auto-backend-design.md`. Owner decision 2026-09-11: NL-shaped queries only; revisit after live dogfooding.
- Auto picks hybrid only when `julie_index::search::scoring::is_nl_like_query(query)` is true and `crate::search::query::looks_like_file_or_path_query(query)` is false. No other classifier.
- Explicit `backend=lexical|semantic|hybrid` behavior does not change. `regions` never reaches `execute_search_unified` and does not change.
- A silent auto fallback to lexical never sets `trace.backend_fallback` and never adds a note to the output.
- Output label for auto-hybrid hits is the existing `(hybrid)` from `strategy_id = "fast_search_hybrid"`.
- `JULIE_AGENT_INSTRUCTIONS.md` stays at most 1,900 characters. `.claude/hooks/julie-routing-block.md` stays at most 4,000 bytes. Any wording change to one is mirrored to the other.
- Tool descriptions change only in the sentences that describe the omitted-backend default, in all three places: `src/request_engine/catalog.rs:170`, `src/handler/tools/fast_search.rs:19`, `crates/julie-tools/src/search/params.rs:43`.
- Commit messages: conventional commits, one commit per task, on branch `hybrid-auto` in `/home/murphy/source/julie/.worktrees/hybrid-auto`.
- No pushes, no releases, no edits outside this worktree except the evidence result files Task 2 writes under `docs/eval/`.
- Test rules from `CLAUDE.md`: workers run exact tests only, at most two runs per change. The lead runs `cargo xtask test changed` and the tiers.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` sections "RUNNING TESTS" and "Canonical Test Tiers"; `xtask/test_tiers.toml`.

**Worker red/green scope:** `cargo nextest run --lib <exact_test_name>` for the tests in `src/tests/tools/search/backend_param_tests.rs`.

**Worker ceiling:** the exact test names written in Task 1. Workers never run `cargo xtask test …` or an unfiltered `cargo nextest run`.

**Worker gate invariant:** Task 1: `auto_nl_query_runs_hybrid_when_vectors_are_ready` proves the default changed; `explicit_lexical_never_runs_hybrid` and `auto_identifier_query_stays_lexical_with_vectors` prove nothing else did.

**Lead affected-change scope:** `cargo xtask test changed` after Task 1 lands; then `cargo xtask test bucket tools-search-hybrid`; `cargo test -p xtask --test docs_contract_tests` if any instruction file changed.

**Branch gate:** `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo xtask test dev`, `cargo xtask test dogfood`, `cargo xtask test full`. Run in a detached gate worktree `.worktrees/hybrid-auto-gate` with `target` symlinked per memory note `gate-checkout-target-symlink`.

**Security scope:** none declared.

**Replay/metric evidence:** hard gates: every test named in this plan passes; `dogfood` passes; scorecard hybrid beats lexical by at least 20 percent relative MRR (bar from `docs/eval/semantic-value/README.md`); head-to-head Julie `auto` top-5 within one row per class of the 6a hybrid column (`retrieval.concept` 10/13, `retrieval.implementation` 9/10); `inspect.symbol` 23/23. Report-only: `fast_search` auto p50 latency and bytes on the head-to-head rows; count of auto-hybrid rows in the run.

**Escalation triggers:** any change under `crates/julie-tools/src/search/` runs `cargo xtask test dogfood`. Any change to `src/handler/` runs `cargo xtask test system`.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-11-hybrid-auto-backend-ledger.md` using `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse evidence only when scope label and HEAD match exactly.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Auto-hybrid resolution, fall-through, telemetry, docs, tests | None - serial | Modify `crates/julie-tools/src/search/backend.rs`, `crates/julie-tools/src/search/tool_execution.rs`, `crates/julie-tools/src/search/execution/mod.rs`, `crates/julie-tools/src/search/params.rs`, `src/handler/tools/fast_search.rs`, `src/request_engine/catalog.rs`, `src/handler/search_telemetry.rs`, `src/tests/tools/search/backend_param_tests.rs`, `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/hooks/julie-routing-block.md`. | Yes | Task 2 measures the built binary. |
| Task 2: Scorecard, head-to-head, finding, ledger | None - serial | Create `docs/eval/semantic-value/results/<timestamp>.{json,md}`, `docs/eval/head-to-head/results/<timestamp>.{json,md}`, `docs/findings/2026-09-1X-hybrid-auto-backend.md`, `docs/plans/2026-09-11-hybrid-auto-backend-ledger.md`. | Yes | Needs Task 1's binary. Lead-run: needs the release binary, the sidecar, and the ten corpus workspaces on this machine. |

Commit mode for every task: `serial-worker-commit`.

---

## Task 1: Auto-hybrid resolution, fall-through, telemetry, docs, tests

**Files:**
- Modify: `crates/julie-tools/src/search/backend.rs:19-24` (`resolve`), `crates/julie-tools/src/search/tool_execution.rs:161` (the one `resolve` caller), `crates/julie-tools/src/search/execution/mod.rs:97-141` (non-lexical branch), `crates/julie-tools/src/search/params.rs:43` (`backend` field doc), `src/handler/tools/fast_search.rs:19` (tool attribute description), `src/request_engine/catalog.rs:170` (catalog description), `src/handler/search_telemetry.rs:51` (add `backend_auto`), `JULIE_AGENT_INSTRUCTIONS.md:13`, `.claude/hooks/julie-routing-block.md:3,48`
- Test: `src/tests/tools/search/backend_param_tests.rs` (extend; fixtures `semantic_workspace_with_embeddings` at line 70 and `semantic_workspace_without_vectors` at line 102 already exist)

**Interfaces:**
- Consumes: `julie_index::search::scoring::is_nl_like_query(&str) -> bool` (`crates/julie-index/src/search/scoring.rs:450`); `crate::search::query::looks_like_file_or_path_query(&str) -> bool` (`crates/julie-tools/src/search/query.rs:32`); `snapshot_has_embeddings`, `run_symbol_backend_pass` in `execution/semantic.rs`.
- Produces: `SearchBackend::auto_prefers_hybrid(query: &str) -> bool` (public, so the CLI and tests can ask the rule); `SearchBackend::resolve(requested: Option<Self>, query: &str) -> ResolvedSearchBackend`; telemetry field `backend_auto: bool`.

**Contract inputs:** design doc sections "Decision" and "Change shape".
**File ownership:** as listed in the contract table.
**Serialization required:** Yes.
**Dependency reason:** Task 2 measures this task's binary.

**What to build:**

1. In `backend.rs`, add `pub fn auto_prefers_hybrid(query: &str) -> bool` returning `is_nl_like_query(query) && !looks_like_file_or_path_query(query)`. Change `resolve` to take `query: &str`; when `requested` is `None` and `auto_prefers_hybrid(query)`, return `value: Hybrid, explicit: false`; otherwise unchanged.
2. In `tool_execution.rs:161`, pass `&self.query`.
3. In `execution/mod.rs`, inside the non-lexical branch after `run_symbol_backend_pass` returns: if `!params.backend.explicit && execution.hits.is_empty()`, do not return; let control fall to the lexical pass below. Keep `backend_fallback = params.backend.explicit` as the value for the fall-through case. Read the branch in full with `deep_dive execute_search_unified depth=full` before editing; the `Required` bails stay exactly where they are.
4. In `search_telemetry.rs`, add `"backend_auto": params.backend.is_none()` beside `backend_fallback` in both metadata builders (`fast_search_metadata` at line 11 and `fast_search_metadata_with_regions` at line 18 share the closure; add it once where `backend_fallback` is emitted).
5. Docs. Replace the omitted-backend sentence in all three descriptions with: `omitted backend runs hybrid symbol search for natural-language queries when embeddings are ready, else lexical mixed file+symbol hits (with labeled semantic fallback candidates on identifier-like zero-hit queries)`. Keep every other sentence. In `JULIE_AGENT_INSTRUCTIONS.md:13` and `julie-routing-block.md:3`, change the `backend` clause to: `` `backend` lexical, semantic, or hybrid; omitted picks hybrid for natural-language queries when vectors are ready ``. In `julie-routing-block.md:48`, replace `Omit backend for normal search.` with `Omit backend: natural-language queries run hybrid when vectors are ready, else lexical.` Check `wc -c JULIE_AGENT_INSTRUCTIONS.md` stays at most 1,900 and the routing block at most 4,000 bytes. If the instruction file would exceed 1,900, shorten the clause to `` `backend` lexical, semantic, hybrid; omitted is hybrid for prose queries `` and mirror it.
6. Tests in `backend_param_tests.rs`, each run with `cargo nextest run --lib <name>`:
   - `auto_prefers_hybrid_for_nl_queries`: assert true for `"where does the app create the router"`; false for `"SearchBackend"`, `"parse_query score_candidate"`, `"src/search/backend.rs"`, `"find backend.rs"`, `""`.
   - `auto_nl_query_runs_hybrid_when_vectors_are_ready`: `semantic_workspace_with_embeddings`, `backend: None`, query `"semantic backend target function"`. Assert `execution.trace.strategy_id == "fast_search_hybrid"` and `backend_fallback == false`, and the rendered text contains `(hybrid)`.
   - `auto_nl_query_stays_lexical_without_vectors`: `semantic_workspace_without_vectors`, same query. Assert `strategy_id == "search_unified"`, `backend_fallback == false`, text does not contain `NOTE: backend=`.
   - `auto_nl_query_falls_through_to_lexical_on_zero_hybrid_hits`: `semantic_workspace_with_embeddings`, `backend: None`, query `"unrelated prose that matches nothing embedded"` with `file_pattern: Some("*.rs")`. The static provider embeds every query to the target vector, so make the hybrid pass empty through `language: Some("python")` (no Python symbols in the fixture) and assert `strategy_id == "search_unified"`. If the fixture makes zero hybrid hits impossible, add a second source file in the fixture and adjust; do not weaken the assertion.
   - `auto_identifier_query_stays_lexical_with_vectors`: `semantic_workspace_with_embeddings`, `backend: None`, query `"semantic_backend_target"`. Assert `strategy_id != "fast_search_hybrid"`.
   - `explicit_lexical_never_runs_hybrid`: `semantic_workspace_with_embeddings`, `backend: Some(SearchBackend::Lexical)`, NL query. Assert `strategy_id == "search_unified"`.
   - `required_semantics_on_auto_nl_query_without_vectors_reports_not_ready`: `semantic_workspace_without_vectors`, `backend: None`, `semantics: Some(SemanticMode::Required)`, NL query. Assert the error contains `SEMANTICS_NOT_READY`.

**Approach:** Write the seven tests first and confirm each fails for the right reason (the resolve signature change makes the file fail to compile until step 1; write the tests against the new signature). Then steps 1 through 5. Run `cargo check` after each step. Follow razorback:test-driven-development. Use `deep_dive` on `execute_search_unified` and `run_symbol_backend_pass` before editing.

**Acceptance criteria:**
- [x] The seven tests pass with `cargo nextest run --lib <name>`.
- [x] `cargo check` and `cargo fmt --check` pass.
- [x] `wc -c JULIE_AGENT_INSTRUCTIONS.md` is at most 1,900; `wc -c .claude/hooks/julie-routing-block.md` is at most 4,000.
- [x] All three tool descriptions and both instruction files say the same thing about the omitted backend.
- [x] Telemetry JSON carries `backend_auto`.
- [x] One commit: `feat(search): omitted backend runs hybrid for natural-language queries when vectors are ready`.

---

## Task 2: Scorecard, head-to-head, finding, ledger

**Files:**
- Create: `docs/findings/2026-09-1X-hybrid-auto-backend.md`, `docs/plans/2026-09-11-hybrid-auto-backend-ledger.md`, result files the harnesses write under `docs/eval/semantic-value/results/` and `docs/eval/head-to-head/results/`.

**Interfaces:**
- Consumes: `python3 docs/eval/semantic-value/run_scorecard.py --binary <path> --backend lexical --backend hybrid`; `python3 docs/eval/head-to-head/run_matrix.py --julie-bin <path> --skip-miller --require-semantics`; `target/release/julie-server service status`.

**Contract inputs:** design doc section "Gate".
**File ownership:** as listed.
**Serialization required:** Yes.
**Dependency reason:** Needs Task 1's release binary. Lead-run.

**What to build:**

1. `cargo build --release` in this worktree. Confirm `julie-semantic-sidecar` sits beside `target/release/julie-server` (copy it from the main checkout's `target/release/` if missing; memory note `sidecar-binary-location`). Run `target/release/julie-server service restart` and confirm `service status` reports the embedding runtime available.
2. Confirm all ten corpus workspaces have vectors: `service status` or one `fast_search backend=semantic semantics=required` per repo through the CLI. Wait for backfill if any repo reports zero vectors; do not run the matrix early.
3. Run the scorecard with `--backend lexical --backend hybrid`. Record the result path and the relative MRR delta.
4. Run the head-to-head with `--skip-miller --require-semantics`. Record `auto` top-1 and top-5 per class, p50 latency and bytes for `fast_search`, and how many search rows the trace labels `fast_search_hybrid`.
5. Write the finding: what changed, the two tables, the latency line, the count of auto-hybrid rows, and any row where auto lost to the 6a hybrid column with the reason. Include the resident memory line from `service status`.
6. Write the ledger with every row from Task 1's worker runs, the lead's `changed`, `tools-search-hybrid`, `dev`, `dogfood`, `full`, fmt, clippy, and the two harness runs.

**Acceptance criteria:**
- [x] Scorecard: hybrid relative MRR at least 20 percent over lexical. If it fails, stop and report; do not tune weights in this plan.
- [x] Head-to-head: `auto` top-5 within one row per class of 10/13 and 9/10; `inspect.symbol` 23/23.
- [x] `dogfood`, `dev`, `full`, fmt, clippy pass in the gate worktree at Task 1's SHA.
- [x] Finding and ledger committed: `docs(search): hybrid auto backend evidence and ledger`.
