# Backend Param on fast_search, deep_dive, get_context — Implementation Plan (v2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Expose the embedding pipeline through three existing tools by adding an optional `backend` parameter (`lexical` | `semantic` | `hybrid`). Omitting the param preserves today's behavior on every tool. Explicit `semantic`/`hybrid` requests wait for the embedding provider to settle, then run KNN-based retrieval.

**Architecture:** One shared `SearchBackend` enum lives in `src/tools/search/backend.rs` (re-exported from `search/mod.rs`). Each tool gets `backend: Option<SearchBackend>`. Per-tool resolved defaults match today's behavior. `fast_search`'s semantic/hybrid backends are explicitly **symbol-only** (no file rows — file-path queries belong on lexical), but `UnifiedHit.kind` remains the actual symbol kind (`function`, `class`, etc.), not the literal string `"symbol"`. A shared helper `wait_for_embedding_provider_settled(handler, timeout)` is extracted from the existing `nl_embeddings.rs` lazy-init code so explicit semantic requests wait on daemon warm-up; stdio lazy-init keeps today's unbounded `initialize_embedding_provider` behavior unless a separate timeout design is approved. The lexical branch of `execute_search_unified` is unchanged byte-for-byte.

**Tech Stack:** Rust, rmcp, serde, tantivy (BM25), sentence-transformers via Python sidecar (semantic), KNN via `vec_symbols` SQLite virtual table, existing `hybrid_search` + `weighted_rrf_merge` + `find_similar_by_query` primitives.

**Architecture Quality:** Approved shape — one shared enum, per-tool resolved defaults preserve current behavior, fast_search semantic/hybrid is explicitly symbol-only, fallback note fires only when backend was *explicitly* requested. The dispatch seam reintroduced in `execute_search_unified` is narrower than the T8-deleted `search_target` seam: it selects retrieval engine, not Tantivy doc-type, and the lexical branch is byte-identical. Risk: any drift from byte-identical lexical, any workspace-routing shortcut in KNN dispatch, or any file-row output from semantic/hybrid recreates the T8 mess. Mitigation: hard gate on lexical bakeoff + workspace-aware semantic/hybrid helpers + explicit symbol-only contract for semantic/hybrid + dedicated fixture tests for new backends.

---

## Background

`fast_search` currently routes 100% through `execute_search_unified` → `unified_search_hits` → `index.search_unified_with_meta` (pure Tantivy BM25, mixed file+symbol rows). The embedding pipeline (Python sidecar, CodeRankEmbed 768d, `vec_symbols` KNN) is consumed by:

- `get_context::pipeline::run_pipeline_with_options` (calls `hybrid_search` with provider — symbol-only output)
- `deep_dive::data::build_similar` (KNN via `find_similar_symbols`, only at `context`/`full` depth)
- `fast_refs::try_semantic_fallback` (zero-result fallback)

Semantic was deliberately removed from `fast_search` (commits `dfa5c829`, `55b733f2`) because *auto-blending* hurt code-search precision. This plan does **not** restore blending-by-default. It adds an explicit caller-driven opt-in.

**What this plan delivers:** an explicit `backend=semantic` / `backend=hybrid` opt-in for *symbol/concept* discovery on `fast_search` and `get_context`, plus always-on semantic `similar` enrichment on `deep_dive`. **What it does not deliver:** cross-language symbol-graph traversal (the IUser.ts → UserDto.cs example). That requires walking typed relationships with semantic edges as fall-back when names diverge — a richer feature, explicitly deferred to a follow-up plan.

---

## Design Decisions

### Decision 1: Per-tool resolved defaults preserve current behavior

User instruction: `fast_search` `backend` param optional, defaults to `Lexical` so callers may omit it. For the other two tools (no user instruction), omit = current behavior:

| Tool | `backend` omitted resolves to | Why |
|---|---|---|
| `fast_search` | `Lexical` | User instruction; current BM25-only |
| `deep_dive` | `Lexical` | Current behavior (similar at context/full only, cap 5) |
| `get_context` | `Hybrid` | Current behavior (already calls `hybrid_search` with provider) |

### Decision 2: `SearchBackend` is a shared enum at `src/tools/search/backend.rs`

One `pub enum SearchBackend { Lexical, Semantic, Hybrid }` with `Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema`, `#[serde(rename_all = "lowercase")]`. Lives in `src/tools/search/backend.rs` (not `src/tools/shared/` — `src/tools/shared.rs` is a flat file). Re-exported via `pub use self::backend::SearchBackend;` in `src/tools/search/mod.rs`. `deep_dive` and `get_context` import via `crate::tools::search::SearchBackend`.

### Decision 3: `fast_search` `Semantic`/`Hybrid` are **symbol-only**

The post-T12 `fast_search` lexical path returns mixed `UnifiedHit` rows (symbols + files). The existing `hybrid_search` returns `SymbolSearchResults` — symbols only, via `search_symbols_via_unified`. We don't try to bridge them in this plan. Instead:

- `backend=Lexical` (and omit): current behavior — mixed file + symbol rows.
- `backend=Semantic` / `backend=Hybrid`: **symbol-only output**. File rows do not appear. `UnifiedHit.kind` must preserve the actual symbol kind (`function`, `method`, `class`, etc.); "symbol-only" means every result is backed by a symbol (`symbol_id.is_some()` / `kind != "file"`), not `kind == "symbol"`. The tool description and field docs say so explicitly: "Semantic and hybrid backends find symbols by concept; for file-path queries use the default lexical backend."

This is a defensible product call: semantic similarity over file paths is incoherent anyway. The unified-output contract is preserved for lexical (the default and bakeoff-gated path); semantic/hybrid get a narrower, well-defined contract.

### Decision 4: `Semantic` on `get_context` bypasses RRF entirely

`weighted_rrf_merge` with `keyword_weight=0` still admits zero-scored lexical rows when semantic returns fewer than `limit` items (`hybrid.rs:93-98`). So "semantic-only via zero weight" is wrong. Implementation: `backend=Semantic` on `get_context` calls a new `semantic_only_search()` in `src/search/hybrid.rs` that runs `provider.embed_query` → `db.knn_search` → `knn_to_search_results` → filter — **no Tantivy fetch, no RRF merge.** `backend=Hybrid` calls existing `hybrid_search` (current behavior). `backend=Lexical` calls `hybrid_search(provider=None)` (degenerates inside).

### Decision 5: `backend` on `deep_dive` controls only the `similar` enrichment

| backend | `similar` shown? | cap |
|---|---|---|
| omitted / `Lexical` | only at `context`/`full` (current) | 5 (current) |
| `Semantic` / `Hybrid` | at all depths including `overview` | 10 |

`Semantic` and `Hybrid` are identical on deep_dive (both mean "always include semantic similar"). Accepted for enum consistency; if a real lexical-similar (name-only string similarity) variant lands later, `Lexical`-always-on becomes meaningful.

### Decision 6: Explicit semantic/hybrid requests wait for provider settlement

`handler.embedding_provider().await` is **non-blocking** — it just reads the workspace lock. It returns `None` during daemon cold-start even though the provider may settle milliseconds later. The existing `nl_embeddings::maybe_initialize_embeddings_for_nl_definitions` handles this for the implicit-NL path by calling `EmbeddingServiceSettled::wait_until_settled(3s)`.

Implementation:
- **Extract** the settled-wait + stdio-init logic from `maybe_initialize_embeddings_for_nl_definitions` into a reusable `pub(crate) async fn wait_for_embedding_provider_settled(handler: &JulieServerHandler, daemon_timeout: Duration) -> Option<Arc<dyn EmbeddingProvider>>` in `src/tools/search/nl_embeddings.rs`. Existing `maybe_initialize_embeddings_for_nl_definitions` becomes a thin wrapper that calls the extracted helper with the NL-specific gate (`is_nl_like_query`) and 3s daemon timeout.
- The timeout parameter bounds only daemon `EmbeddingServiceSettled::wait_until_settled`. In stdio mode, preserve today's behavior: `workspace.initialize_embedding_provider()` runs in `spawn_blocking` and is awaited without a timeout. Do **not** wrap it in `tokio::time::timeout` unless the plan also defines how to publish or cancel the spawned init result safely.
- **Explicit semantic/hybrid path** (`was_explicit && backend != Lexical`): call `wait_for_embedding_provider_settled(handler, Duration::from_secs(3))` once before dispatching to the KNN path. If `Some(provider)` → proceed with semantic/hybrid. If `None` → emit the explicit-request fallback note and run lexical (see Decision 7).
- **Implicit `get_context` Hybrid default** (`was_explicit == false && default == Hybrid`): do not call the settled-wait helper. Use the existing non-blocking `handler.embedding_provider().await` path so omit-equivalence holds.
- **Omit / `Lexical` path**: no settled-wait. Behavior unchanged.

### Decision 7: Fallback note fires only on **explicit** semantic/hybrid requests

Today's omit-equivalence requires that callers who didn't ask for semantic see no new output. Track whether backend was explicitly set vs resolved-from-default:

- Add `was_explicit: bool` alongside the resolved backend (e.g., `struct ResolvedBackend { value: SearchBackend, was_explicit: bool }` returned by a `SearchBackend::resolve_with_origin(opt, default)` helper).
- Fallback note prepended **only when** `was_explicit && resolved != Lexical` and either provider is unavailable or the target workspace has zero stored symbol embeddings (`SymbolDatabase::embedding_count() == 0`).
- Do not fallback solely because a semantic query returns zero KNN hits when embeddings exist. That is a valid semantic miss, not an embedding-readiness failure.
- Note text: `"Note: semantic/hybrid backend requested but embeddings unavailable — falling back to lexical.\n\n"`.

This preserves omit-equivalence on `get_context` (omit → Hybrid → silent fallback inside `hybrid_search`, today's behavior). For `fast_search`, the note is stored as structured trace state and prepended in `FastSearchTool::execute_with_trace`, because `execute_search_unified` returns structured hits rather than rendered text.

### Decision 8: Bakeoff harness — lexical hard-gate only; new backends use fixture tests

The eros bakeoff CLI/harness has no `--backend` flag and adding one is out-of-scope for this plan. Approach:

- **Lexical default bakeoff** — hard gate, byte-identical to current main (the bakeoff already runs against the no-backend MCP shape).
- **Semantic / hybrid quality** — gated by new hand-curated fixture tests in `src/tests/tools/search/backend_param_tests.rs`. Each fixture asserts a known-embedded symbol surfaces in the top-N for a known semantic query (e.g., a function named `validate_token` surfaces for query `"check authentication"`). At least 3 semantic + 3 hybrid fixtures, with assertions on result IDs (not scores).
- **Out of scope:** Adding `--backend` to the bakeoff harness. Deferred follow-up.

### Decision 9: Sequence relative to FTS Phase 2

Active brief is FTS Phase 2 unified schema (commit `1065525c`). This plan does not touch the Tantivy schema, tokenizer, or `execute_search_unified`'s lexical branch — it adds parallel branches. Safe to land independently. Hard gate on lexical bakeoff prevents accidental drift.

---

## File Structure

**New files:**
- `src/tools/search/backend.rs` — `SearchBackend` enum + `ResolvedBackend` struct + `resolve_with_origin` + per-tool `default_for_*` helpers + lenient deserializer.

**Modified files (interface changes):**
- `src/tools/search/mod.rs` — re-export `SearchBackend` and `ResolvedBackend`; add `backend: Option<SearchBackend>` to `FastSearchTool` (struct, Serde mirror, Default impl).
- `src/tools/search/execution.rs` — add `backend: ResolvedBackend` to `SearchExecutionParams`; dispatch in `execute_search_unified` on resolved value.
- `src/tools/search/text_search.rs` — add workspace-aware `semantic_symbol_hits()` (KNN, returns symbol `UnifiedHit`s preserving actual symbol kind) and `hybrid_symbol_hits()` (wraps existing `hybrid_search`, converts `SymbolSearchResult` → `UnifiedHit` while preserving actual symbol kind).
- `src/tools/search/nl_embeddings.rs` — extract `wait_for_embedding_provider_settled()`; refactor `maybe_initialize_embeddings_for_nl_definitions` to call it.
- `src/search/hybrid.rs` — add `semantic_only_search()` for KNN-only path used by `get_context`.
- `src/tools/deep_dive/mod.rs` — add `backend: Option<SearchBackend>` to `DeepDiveTool`; thread through `deep_dive_query`.
- `src/tools/deep_dive/data.rs` — `build_symbol_context` takes `include_similar: bool, similar_cap: usize`; `build_similar` takes `cap: usize`.
- `src/tools/get_context/mod.rs` — add `backend: Option<SearchBackend>` to `GetContextTool`.
- `src/tools/get_context/pipeline.rs` — thread `ResolvedBackend` through `run` → `run_pipeline_with_options`; dispatch lexical / semantic / hybrid paths.
- `src/handler/search_telemetry.rs` — include `backend_fallback` in manually assembled fast_search trace metadata.
- `src/dashboard/routes/search.rs:191` — update `SearchExecutionParams { ... }` literal to include `backend: ResolvedBackend::lexical_implicit()`.
- `src/dashboard/search_compare.rs:187` — same literal update.
- `src/tools/search/execution.rs:57` — update the `SearchExecutionParams` normalization literal to preserve `backend`.
- `src/cli_tools/commands.rs` — update direct `FastSearchTool` and `GetContextTool` literals with `backend: None`.
- `src/tests/**` — update direct `FastSearchTool`, `GetContextTool`, and `DeepDiveTool` literals with `backend: None` or use `..Default::default()` where the struct supports it.
- `src/handler/tools/fast_search.rs:17` — extend tool description with backend param mention.
- `src/handler/tools/deep_dive.rs:17` — extend tool description.
- `src/handler/tools/get_context.rs:17` — extend tool description.
- `.claude/skills/explore-area/SKILL.md` — mention `backend="semantic"` on `get_context`.
- `.claude/skills/impact-analysis/SKILL.md` — mention `backend="semantic"` on `deep_dive`.
- `JULIE_AGENT_INSTRUCTIONS.md` — add one-paragraph section on backend param.

**Test files:**
- `src/tests/tools/search/backend_param_tests.rs` (new) — fast_search backend variants + semantic/hybrid fixture quality tests.
- `src/tests/tools/search/backend_settlement_tests.rs` (new) — daemon cold-start, stdio standalone, and provider-present-zero-vectors behaviors.
- `src/tests/tools/deep_dive_backend_tests.rs` (new) — backend variants. Do not add new tests to oversized `deep_dive_tests.rs`.
- `src/tests/tools/get_context_tests.rs` (modify if exists, else new) — backend variants including omit-equivalence.
- `src/tests/tools/search/backend_tests.rs` (new) — enum deserializer, `resolve_with_origin`.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` (xtask runner section), `docs/TESTING_GUIDE.md`, `RAZORBACK.md` (gate ownership + worker eligibility).

**Worker red/green scope:** `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`. One test per cycle.

**Worker ceiling:** One nextest filter per change cycle. Workers do NOT run xtask tiers.

**Worker gate invariant:** Each worker reports the behavior invariant their test proves. Tests must assert on values, not non-error.

**Lead affected-change scope:** `cargo xtask test changed` after each batch.

**Branch gate:** `cargo xtask test dev` once before handoff.

**Dogfood gate:** `cargo xtask test dogfood` — required because backend dispatch touches `execute_search_unified`.

**Replay/metric evidence — hard gates:**
1. **Lexical bakeoff byte-identical to main HEAD.** Run the existing eros bakeoff against the no-backend MCP shape pre- and post-change; numbers must match exactly. Stops the merge if any drift.
2. **`cargo check` clean across all dashboard + CLI + test callers.** Specifically verify `src/tools/search/execution.rs:57`, `src/tools/search/mod.rs:411`, `src/dashboard/routes/search.rs:191`, `src/dashboard/search_compare.rs:187`, `src/cli_tools/commands.rs`, and direct tool literals under `src/tests/**` compile after new `backend` fields land.
3. **Semantic + hybrid fixture tests in `backend_param_tests.rs` pass.** Each fixture asserts a specific symbol ID appears in top-N for a known query.
4. **Settlement behavior tests in `backend_settlement_tests.rs` pass.** Daemon cold-start (provider settles within timeout → semantic runs), daemon stale (timeout → fallback note + lexical), stdio standalone with no provider (fallback note + lexical), provider-present-zero-vectors (`embedding_count() == 0` → fallback note + lexical).
5. **Omit-equivalence test.** `fast_search(query="x")` and `fast_search(query="x", backend=None)` produce byte-identical output; same for `get_context` and `deep_dive`.

**Replay/metric evidence — report-only:**
- Telemetry shape change: if `backend_fallback: bool` is added to `SearchTrace`, update `src/handler/search_telemetry.rs` because fast_search telemetry is manually assembled. Ledger the new field's presence in telemetry output; no quality gate.

**Escalation triggers:**
- Any lexical bakeoff drift → stop, escalate to strategy tier.
- Any change to `unified_search_hits` or the lexical branch of `execute_search_unified` beyond what this plan specifies → stop, escalate.
- Embedding-provider lifecycle changes (daemon, sidecar init order) → escalate.
- Mixed-kind output appearing in semantic/hybrid paths (would violate Decision 3) → escalate.

**Assigned verification failure:** Workers stop and report; never update gates or rerun broader scopes.

**Verification ledger:** Use `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, timestamp.

---

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Lead-owned by default.** Per `RAZORBACK.md:119` and `:155`, search ranking/query-semantics work is off-limits to unattended implementation-tier workers. Lead owns Tasks 2, 3, 4 (search dispatch + settlement helper + get_context plumbing). Tasks 1, 5, 6, 7 may be delegated.

**Strategy tier:** Plan review, decomposition, gate interpretation.
- Codex: `gpt-5.5 medium` for routine; `gpt-5.5 high` for final review.
- Claude: Opus.

**Implementation tier:** Bounded mechanical edits with narrow verification.
- Codex: `gpt-5.5 low` for Task 1 (shared enum), Task 7 (tool-description edits).
- Claude: Sonnet.

**Coupled implementation tier:** Cross-file bounded work after lead fixes contract.
- Codex: `gpt-5.5 medium` for Task 5 (deep_dive), Task 6 (dashboard caller updates).
- Claude: Sonnet high.

**Mechanical tier:** Tool-description string updates only; no logic.
- Codex: `gpt-5.4-mini low/medium`.
- Claude: Haiku.

**Gate-interpretation reviewer:** Final adversarial review of bakeoff + fixture tests.
- Codex: `gpt-5.5 high`.
- Claude: Opus.

**Escalation tier:** Any lexical bakeoff drift, any unspecified change to lexical branch, any embedding-provider lifecycle concern, any mixed-kind leak into semantic/hybrid output.
- Codex: `gpt-5.5 xhigh`.
- Claude: Opus.

**Worker eligibility:** Tasks 1, 5, 6, 7 may be delegated. Tasks 2, 3, 4 lead-owned.

**Mechanical exclusion:** Mechanical workers cannot own bakeoff or fixture gates.

**Unsupported harness behavior:** Use `inherit` and note in worker report.

---

## Tasks

### Task 1: `SearchBackend` enum + `ResolvedBackend` *(implementation tier)*

**Files:**
- Create: `src/tools/search/backend.rs`
- Modify: `src/tools/search/mod.rs` (add `pub(crate) mod backend;` and `pub use self::backend::{SearchBackend, ResolvedBackend};`)
- Test: `src/tests/tools/search/backend_tests.rs` (new; add `pub mod backend_tests;` in `src/tests/tools/search/mod.rs`)

**What to build:**
- `pub enum SearchBackend { Lexical, Semantic, Hybrid }` with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]` and `#[serde(rename_all = "lowercase")]`.
- Lenient deserializer accepting case-insensitive variants; unknown values error with message listing valid values.
- `pub struct ResolvedBackend { pub value: SearchBackend, pub was_explicit: bool }` with helpers:
  - `ResolvedBackend::lexical_implicit() -> Self` (for dashboard/CLI defaults).
  - `SearchBackend::resolve_with_origin(opt: Option<SearchBackend>, default: SearchBackend) -> ResolvedBackend` — `was_explicit = opt.is_some()`.
- Per-tool default free functions: `default_for_fast_search() -> SearchBackend` (= `Lexical`), `default_for_deep_dive() -> SearchBackend` (= `Lexical`), `default_for_get_context() -> SearchBackend` (= `Hybrid`).

**Approach:** Pattern after `src/tools/deep_dive/mod.rs:20-39` (`DeepDiveDepth`) for the enum derives + serde shape.

**Acceptance criteria:**
- [ ] Test: `serde_json::from_str::<SearchBackend>("\"lexical\"")` → `Lexical`.
- [ ] Test: `serde_json::from_str::<SearchBackend>("\"SEMANTIC\"")` → `Semantic` (case-insensitive).
- [ ] Test: `serde_json::from_str::<SearchBackend>("\"foo\"")` errors with all three valid values listed.
- [ ] Test: `SearchBackend::resolve_with_origin(None, Lexical).was_explicit == false`.
- [ ] Test: `SearchBackend::resolve_with_origin(Some(Hybrid), Lexical).was_explicit == true && .value == Hybrid`.
- [ ] Test: `default_for_get_context() == Hybrid`.
- [ ] Worker-scope nextest filter passes; committed.

---

### Task 2: Settled-wait helper extraction *(lead-owned)*

**Files:**
- Modify: `src/tools/search/nl_embeddings.rs` (extract helper + refactor caller)
- Test: `src/tests/tools/search/backend_settlement_tests.rs` (new; module declared in `src/tests/tools/search/mod.rs`)

**What to build:**
- Extract from `maybe_initialize_embeddings_for_nl_definitions` (lines 46-185): the daemon `wait_until_settled` block + the stdio per-workspace `initialize_embedding_provider` block → `pub(crate) async fn wait_for_embedding_provider_settled(handler: &JulieServerHandler, daemon_timeout: Duration) -> Option<Arc<dyn EmbeddingProvider>>`.
- New helper returns `Some(provider)` if daemon settled Ready OR stdio init succeeded. It returns `None` if daemon settles Unavailable, daemon stays Initializing past `daemon_timeout`, no workspace exists in stdio mode, or stdio init produces no provider.
- `daemon_timeout` does not bound the stdio `initialize_embedding_provider` call. Preserve existing stdio behavior instead of adding a fake timeout around uncancellable `spawn_blocking` work.
- `maybe_initialize_embeddings_for_nl_definitions` becomes: gate on `is_nl_like_query`, then call `wait_for_embedding_provider_settled(handler, Duration::from_secs(3))` and discard the return (it has already side-effected the workspace state). Behavior must be unchanged.

**Approach:**
- The existing function does two things: settle daemon OR init stdio. Extract preserves both behaviors.
- Existing tests in `nl_embeddings.rs::tests` cover daemon Ready, daemon Unavailable/no-stdio-fallback, and stdio no-workspace — they must still pass unchanged. They do **not** currently cover the real daemon Timeout branch; add that coverage in the new backend settlement tests with a short timeout.

**Acceptance criteria:**
- [ ] Existing tests in `src/tools/search/nl_embeddings.rs::tests` pass unchanged (daemon Ready, daemon Unavailable/no-stdio-fallback, stdio no-workspace).
- [ ] New test: `wait_for_embedding_provider_settled` returns `Some(provider)` within timeout when daemon transitions Ready during the wait.
- [ ] New test: returns `None` when daemon stays in `Initializing` past timeout.
- [ ] New test: returns `Some(provider)` after stdio per-workspace init succeeds.
- [ ] New test: returns `None` when called with no workspace and no daemon service.
- [ ] Worker-scope nextest passes; lead runs `cargo xtask test changed`.

---

### Task 3: `fast_search` backend dispatch + symbol-only semantic/hybrid *(lead-owned)*

**Files:**
- Modify: `src/tools/search/mod.rs` (add field to `FastSearchTool`, `FastSearchToolSerde`, `Default`)
- Modify: `src/tools/search/execution.rs` (add `backend: ResolvedBackend` to `SearchExecutionParams`; dispatch in `execute_search_unified`)
- Modify: `src/tools/search/text_search.rs` (add `semantic_symbol_hits` + `hybrid_symbol_hits`)
- Test: `src/tests/tools/search/backend_param_tests.rs` (new)

**What to build:**
- `FastSearchTool` gains `pub backend: Option<SearchBackend>` with `#[serde(default)]`, mirrored in `FastSearchToolSerde`, defaulted to `None` in `Default::default()`.
- `SearchExecutionParams` gains `pub backend: ResolvedBackend`. Callers resolve via `SearchBackend::resolve_with_origin(self.backend, default_for_fast_search())`.
- `execute_search_unified` dispatch at the top:
  - `Lexical` → existing `run_unified_pass` path (byte-identical, no changes).
  - `Semantic` → `wait_for_embedding_provider_settled(handler, 3s)`. If provider unavailable and backend explicit, set `trace.backend_fallback = true` on the lexical fallback result and let `FastSearchTool::execute_with_trace` prepend the fallback note. If provider exists, run the workspace-aware semantic pass below.
  - `Hybrid` → same settle path; on provider available, run the workspace-aware hybrid pass below.
- Add `run_semantic_symbol_pass` / `run_hybrid_symbol_pass` in `execution.rs` or `text_search.rs` that mirror `run_unified_pass`'s workspace loop. For each `SearchExecutionWorkspace`, open that workspace's pooled DB and search index, run the semantic/hybrid symbol query against that workspace only, convert hits, tag each `SearchHit` with `workspace.workspace_id`, then merge/sort/truncate across workspaces.
- `semantic_symbol_hits`: `provider.embed_query(query)` → `db.knn_search(&vec, limit * 4)` (over-fetch for filter survival) → `knn_to_search_results` → apply `SearchFilter` predicates → truncate to `limit` → convert to `UnifiedHit`.
- `hybrid_symbol_hits`: calls `hybrid_search(query, filter, limit, index, db, Some(provider), Some(SearchWeightProfile::fast_search()))` → converts each `SymbolSearchResult` to `UnifiedHit`.
- `symbol_result_to_unified_hit` must preserve `SymbolSearchResult.kind`. It must never set `kind = "symbol"`. Populate `id`, `name`, `signature`, `doc_comment`, `file_path`, `language`, `start_line`, `role`, `test_role`, and `tantivy_score` from `SymbolSearchResult`; derive `basename` from `file_path`; leave non-symbol-only FTS fields empty.
- Semantic/hybrid output rows are symbol-backed only: no row has `kind == "file"` and every `SearchHit.symbol_id` is `Some(_)`.
- Add `pub backend_fallback: bool` field to `SearchTrace`; set true when explicit fallback was emitted. Update `src/handler/search_telemetry.rs` so `backend_fallback` appears in the manually assembled fast_search telemetry trace.
- Explicit fallback to lexical happens when provider is unavailable or the target workspace has `embedding_count() == 0`. Do not fallback merely because a semantic query returns zero KNN hits when embeddings exist.

**Approach:**
- The lexical branch must be byte-identical. The dispatch wraps `run_unified_pass` for Lexical and adds parallel arms for Semantic/Hybrid.
- For `UnifiedHit` construction in `semantic_symbol_hits` and `hybrid_symbol_hits`: invert `unified_hit_to_symbol` (`text_search.rs:129`) but keep the original symbol kind. Likely a new small helper `symbol_result_to_unified_hit(SymbolSearchResult) -> UnifiedHit` near `unified_hit_to_symbol`.
- Tool description (Task 7): extend, do not replace.

**Acceptance criteria:**
- [ ] `FastSearchTool { query: "foo".into(), ..Default::default() }` has `backend: None`.
- [ ] **Omit-equivalence test:** `fast_search(query="x")` byte-identical to `fast_search(query="x", backend=None)`.
- [ ] **Lexical-only test:** `fast_search(query="path/to/file.rs", backend="lexical")` returns file rows (kind=="file" appears).
- [ ] **Semantic symbol-only test:** `fast_search(query="check authentication", backend="semantic")` returns symbols only (assert: no `kind=="file"` row, every hit has `symbol_id.is_some()`, actual symbol kinds like `function` are preserved, and known-embedded `validate_token`-like symbol ID appears in top results).
- [ ] **Hybrid symbol-only test:** `fast_search(query="X", backend="hybrid")` returns only symbol-backed rows and preserves actual symbol kinds.
- [ ] **Target workspace test:** semantic/hybrid `fast_search(workspace=<target>)` reads the target workspace DB/index and reports the target workspace id on every hit.
- [ ] **Filter survival test:** `fast_search(query="X", backend="semantic", language="rust")` returns no non-rust paths.
- [ ] **Explicit-fallback note test:** when provider is None and `backend="semantic"` explicit, output begins with the fallback note + contains lexical results.
- [ ] **Zero-vector explicit-fallback test:** when provider exists but target workspace `embedding_count() == 0`, explicit semantic/hybrid prepends fallback note and returns lexical results.
- [ ] **Valid semantic miss test:** when `embedding_count() > 0` but KNN returns no hits for a query/filter, explicit semantic returns the semantic miss without fallback note.
- [ ] **Implicit-omit test:** when provider is None and `backend` omitted (Lexical), output has NO fallback note.
- [ ] **Invalid backend test:** `backend="foo"` → MCP schema error.
- [ ] **Hard gate:** Eros bakeoff `top1` for no-backend invocations matches current main HEAD baseline.
- [ ] Worker-scope nextest passes; lead runs `cargo xtask test dev` + `cargo xtask test dogfood` + bakeoff.

---

### Task 4: `get_context` backend dispatch with semantic-only KNN path *(lead-owned)*

**Files:**
- Modify: `src/tools/get_context/mod.rs` (add `backend: Option<SearchBackend>`)
- Modify: `src/tools/get_context/pipeline.rs` (thread `ResolvedBackend`; dispatch lexical/semantic/hybrid)
- Modify: `src/search/hybrid.rs` (add `semantic_only_search`)
- Test: `src/tests/tools/get_context_tests.rs` (modify or create — verify file existence first)

**What to build:**
- `GetContextTool` gains `pub backend: Option<SearchBackend>` with `#[serde(default)]`. Field doc: `"Search backend: \"lexical\" (BM25 only), \"semantic\" (KNN only, symbol-focused), or \"hybrid\" (default, RRF merge). Omit for hybrid."`.
- `pipeline::run` resolves via `SearchBackend::resolve_with_origin(tool.backend, default_for_get_context())` and threads `ResolvedBackend` into `run_pipeline_with_options`.
- `run_pipeline_with_options` dispatch (replaces the unconditional `hybrid_search` call at `pipeline.rs:76`):
  - `Lexical` → `hybrid_search(query, filter, 30, index, db, None, profile)` — degenerates to keyword inside `hybrid_search` (already gracefully handled at `hybrid.rs:213-216`).
  - `Hybrid` → existing call: `hybrid_search(... Some(provider) ...)`.
  - `Semantic` → call new `semantic_only_search(query, filter, 30, db, provider)` → returns `SymbolSearchResults` from KNN alone, no Tantivy fetch, no RRF.
- `semantic_only_search` (new in `src/search/hybrid.rs`): `provider.embed_query(query)` → `db.knn_search(&vec, limit * 4)` → `knn_to_search_results` → apply `matches_filter` predicate → truncate to `limit` → wrap in `SymbolSearchResults { results, relaxed: false }`.
- For explicit `Semantic` / `Hybrid`, call `wait_for_embedding_provider_settled(handler, 3s)` in `pipeline::run` (before `spawn_blocking`) instead of bare `handler.embedding_provider().await`. If provider is unavailable or explicit semantic/hybrid targets a workspace with `embedding_count() == 0`, prepend fallback note to final output and fall through to lexical.
- For implicit omit → `Hybrid`, keep today's non-blocking behavior: call `handler.embedding_provider().await` and pass the result to `hybrid_search`. If it is `None`, `hybrid_search` silently degrades to lexical and no fallback note appears.

**Approach:**
- `pipeline.rs:215, 248` currently calls `handler.embedding_provider().await`. Replace with a small resolver: explicit semantic/hybrid uses the settled-wait helper; implicit hybrid uses the current non-blocking provider read; lexical passes `None`.
- `weighted_rrf_merge` is unchanged — `semantic_only_search` bypasses it entirely. This avoids the zero-weight-still-includes-lexical bug.

**Acceptance criteria:**
- [ ] **Omit-equivalence test:** `get_context(query="x")` byte-identical to `get_context(query="x", backend=None)` AND to current pre-change output for the same query.
- [ ] **No-provider omit test:** with provider unavailable, `get_context(query="x")` (omit) produces NO fallback note (matches today's silent degradation).
- [ ] **No-provider explicit semantic test:** with provider unavailable, `get_context(query="x", backend="semantic")` prepends fallback note + returns lexical results.
- [ ] **Zero-vector explicit semantic test:** with provider available but target workspace `embedding_count() == 0`, explicit semantic/hybrid prepends fallback note + returns lexical results.
- [ ] **Semantic-only purity test:** `get_context(query="...", backend="semantic")` with provider available seeds pivots only from KNN top-N (assert on the search-result/pivot seed set, not graph-expanded neighbors). Use a fixture where BM25 top results don't overlap KNN top results.
- [ ] **Lexical-only test:** `get_context(query="...", backend="lexical")` returns BM25-only hits.
- [ ] **Hybrid-default-matches-omit test:** when the provider is already available, `backend="hybrid"` output is identical to `backend=None` (omit).
- [ ] **Explicit-hybrid-cold-start test:** when provider starts unavailable and settles during the wait, `backend="hybrid"` uses provider-backed hybrid while omitted backend keeps today's non-blocking/silent behavior.
- [ ] Existing get_context tests pass unchanged.
- [ ] Worker-scope nextest passes; lead runs `cargo xtask test dev` + `cargo xtask test dogfood`.

---

### Task 5: `deep_dive` backend gating *(coupled implementation tier)*

**Files:**
- Modify: `src/tools/deep_dive/mod.rs:52-68` (add `backend: Option<SearchBackend>`)
- Modify: `src/tools/deep_dive/mod.rs:143-217` (thread through `deep_dive_query`)
- Modify: `src/tools/deep_dive/data.rs` (`build_symbol_context` gains `include_similar, similar_cap`; `build_similar` gains `cap`)
- Test: `src/tests/tools/deep_dive_backend_tests.rs` (new focused module; add `pub mod deep_dive_backend_tests;` in the relevant test module registry)

**What to build:**
- `DeepDiveTool` gains `pub backend: Option<SearchBackend>` with `#[serde(default)]`.
- `deep_dive_query` resolves via `SearchBackend::resolve_with_origin(self.backend, default_for_deep_dive())` and passes:
  - `include_similar = (resolved.value != Lexical) || depth == "context" || depth == "full"`
  - `similar_cap = if resolved.value == Lexical { 5 } else { 10 }`
- `build_symbol_context` gains `include_similar: bool, similar_cap: usize` params, threaded to the `build_similar` call site at `data.rs:319-324`.
- `build_similar` (`data.rs:612`) takes `cap: usize` instead of the hardcoded `SIMILAR_LIMIT: usize = 5`.
- No settled-wait needed here — `find_similar_symbols` already handles missing embeddings gracefully (returns empty). Output formatter suppresses empty `similar` sections, which is the desired silent behavior.

**Approach:**
- `deep_dive_query` already takes `incoming_cap, outgoing_cap`. Add `include_similar, similar_cap` in the same shape.
- Update existing call sites (overload auto-select branch at `mod.rs:169`, etc.) to pass the new params.
- Tool description (Task 7): mention backend param.

**Acceptance criteria:**
- [ ] **Omit-equivalence test:** `deep_dive(symbol="X")` byte-identical to `deep_dive(symbol="X", backend=None)` AND to current pre-change output.
- [ ] **Overview + lexical default test:** `deep_dive(symbol="X", depth="overview")` has NO `similar` section.
- [ ] **Overview + explicit semantic test:** `deep_dive(symbol="X", depth="overview", backend="semantic")` includes a `similar` section.
- [ ] **Full + lexical test:** `deep_dive(symbol="X", depth="full")` caps similar at 5.
- [ ] **Full + semantic test:** `deep_dive(symbol="X", depth="full", backend="semantic")` returns up to 10 similar symbols (assert count > 5 when ≥10 embedded neighbors exist in fixture).
- [ ] **Hybrid == Semantic on deep_dive test:** `backend="hybrid"` produces identical output to `backend="semantic"`.
- [ ] **No-embeddings test:** `deep_dive(symbol="X", backend="semantic")` with no embeddings produces NO `similar` section (silent — formatter suppresses empty).
- [ ] Existing deep_dive tests pass unchanged.
- [ ] Worker-scope nextest passes; lead runs `cargo xtask test changed`.

---

### Task 6: Caller surface update *(implementation tier)*

**Files:**
- Modify: `src/tools/search/execution.rs:57` (`SearchExecutionParams` normalization literal)
- Modify: `src/tools/search/mod.rs:411` (`SearchExecutionParams` literal)
- Modify: `src/dashboard/routes/search.rs:191` (`SearchExecutionParams { ... }` literal)
- Modify: `src/dashboard/search_compare.rs:187` (same)
- Modify: `src/cli_tools/commands.rs` (direct `FastSearchTool` / `GetContextTool` literals)
- Modify: direct tool literals under `src/tests/**` found by `rg -n "FastSearchTool \\{|GetContextTool \\{|DeepDiveTool \\{" src/tests src/cli_tools`

**What to build:**
- Every `SearchExecutionParams { ... }` literal must compile after Task 3 adds `backend`. The normalized-params copy in `execute_search` preserves `params.backend`; `FastSearchTool::execute_with_trace` passes the resolved backend; dashboard literals use `backend: ResolvedBackend::lexical_implicit()` so the dashboard remains lexical-only (matches today; dashboard has no backend UI in scope).
- Every direct `FastSearchTool`, `GetContextTool`, and `DeepDiveTool` struct literal must include `backend: None` or use `..Default::default()` where available. This is compile-surface plumbing only; CLI backend flags remain out of scope.

**Approach:**
- Use `rg -n "SearchExecutionParams \\{|FastSearchTool \\{|GetContextTool \\{|DeepDiveTool \\{" src` to enumerate literals before editing.
- Worker-scope verification: `cargo check` clean.
- No new behavior tests needed — this task is compile-surface plumbing.

**Acceptance criteria:**
- [ ] `cargo check` clean across the workspace.
- [ ] `src/tools/search/execution.rs:57` preserves `params.backend` in the normalization literal.
- [ ] `src/tools/search/mod.rs:411` passes `SearchBackend::resolve_with_origin(self.backend, default_for_fast_search())`.
- [ ] Dashboard call sites updated with `backend: ResolvedBackend::lexical_implicit()`.
- [ ] CLI/test direct tool literals updated with `backend: None` or equivalent defaulting.
- [ ] No dashboard behavior change (dashboard still produces lexical-only output).
- [ ] Worker-scope verification: `cargo check`; committed.

---

### Task 7: Update tool descriptions + skills + agent instructions *(mechanical tier)*

**Files:**
- Modify: `src/handler/tools/fast_search.rs:17` — extend description: `" Optional 'backend' param ('lexical' default, 'semantic' for symbol concept queries, 'hybrid' for symbol BM25+KNN merge). Semantic and hybrid are symbol-only — for file-path queries use the default lexical backend."`.
- Modify: `src/handler/tools/deep_dive.rs:17` — extend: `" Optional 'backend' param ('lexical' default; 'semantic' or 'hybrid' includes similar-symbol enrichment at all depths up to 10 results)."`.
- Modify: `src/handler/tools/get_context.rs:17` — extend: `" Optional 'backend' param ('hybrid' default, 'lexical' for keyword-only, 'semantic' for KNN-only)."`.
- Modify: `.claude/skills/explore-area/SKILL.md` — ≤3 sentences mentioning `backend="semantic"` on `get_context` for concept queries.
- Modify: `.claude/skills/impact-analysis/SKILL.md` — ≤3 sentences mentioning `backend="semantic"` on `deep_dive` for similar-symbol enrichment after a concrete symbol is resolved. Do not call this cross-language symbol discovery.
- Modify: `JULIE_AGENT_INSTRUCTIONS.md` — one-paragraph section under tool catalog explaining when to use `backend="semantic"`.

**What to build:** Documentation only.

**Approach:**
- Read each file with `get_symbols` first.
- Keep text short — MCP tool descriptions carry canonical specs.
- Per `CLAUDE.md` plugin section: after editing `.claude/skills/`, run `cargo xtask sync-plugin --dry-run` and report diff for lead to apply.

**Acceptance criteria:**
- [ ] All three MCP tool descriptions extended.
- [ ] Both skills updated.
- [ ] `JULIE_AGENT_INSTRUCTIONS.md` updated.
- [ ] `cargo xtask sync-plugin --dry-run` diff reported for lead review.
- [ ] `cargo check` clean. No new tests.

---

## Sequencing

1. **Task 1** first (shared enum + ResolvedBackend; blocks 3, 4, 5, 6).
2. **Task 2** in parallel with Task 1 (settled-wait helper; blocks 3, 4).
3. **Task 6** immediately after Task 3 adds the new struct fields. It is mechanical but must be in the same integration batch to keep `cargo check` green.
4. **Tasks 3, 4** lead-owned, sequential (both touch shared `SearchExecutionParams`/`run_pipeline_with_options` paths — sequential avoids merge conflicts).
5. **Task 5** in parallel with Tasks 3/4 after Task 1 (independent files).
6. **Task 7** last (depends on final tool descriptions + behaviors).

Final lead actions after merge:
- `cargo xtask test dev` (full dev tier).
- `cargo xtask test dogfood`.
- Eros bakeoff lexical comparison: hard gate.
- Verification ledger update.
- External review per user reviewer choice.

---

## Out of Scope (Explicit)

- **Cross-language symbol-graph traversal** (the IUser.ts → UserDto.cs example). This plan delivers concept *discovery* via expanded `similar` enrichment, not edge traversal. Real cross-language tracing needs a graph walk over typed relationships with semantic edges as name-divergence fallback — a richer follow-up plan.
- **Auto-detection / intent routing.** Backend is caller-driven only; no `"auto"` value.
- **Mixed file+symbol output for semantic/hybrid on `fast_search`.** Decision 3: explicitly symbol-only. Bridging the unified mixed-kind contract with hybrid_search's symbol-only return is a follow-up.
- **`fast_refs` backend param.** Already has `try_semantic_fallback`; adding `backend` there is a follow-up.
- **CLI flag exposure (`julie-server search`, `deep-dive`, `context`).** Mechanical follow-up to mirror the MCP shape. Compile-only updates to existing CLI struct literals are in scope for Task 6.
- **Eros bakeoff harness `--backend` flag.** Per Decision 8: deferred. Quality gated by fixture tests instead.
- **Dashboard backend UI.** Dashboard stays lexical-only.
- **Tool restructure toward eros's verb-noun shape.** "Option C" from prior conversation; explicitly deferred.

---

## Risks

| Risk | Mitigation |
|---|---|
| Lexical bakeoff regresses (any byte difference in default path) | Hard gate; escalation if numbers move |
| Semantic backend surfaces low-quality results, agents pick wrongly | Default stays lexical; description tells agents when to use semantic; symbol-only contract narrows the surface |
| Embedding provider unavailable → noisy fallback notes | Note fires only on explicit semantic/hybrid request; omit case is silent |
| Daemon cold-start → semantic request gets fallback during warmup | `wait_for_embedding_provider_settled(3s)` blocks briefly; settled-wait tests cover the window |
| Provider present but target workspace has zero stored vectors | Check `embedding_count() == 0`; explicit fallback note fires; lexical results returned |
| Provider present and vectors exist but semantic query has no matching KNN hits | Treat as valid semantic miss; do not fallback solely on empty KNN |
| FTS Phase 2 in-flight changes conflict | Plan does not touch unified Tantivy schema or lexical branch |
| Mixed-kind output leaks into semantic/hybrid | Decision 3 + symbol-only fixture tests; escalation trigger if violated |
| Telemetry consumers miss `backend_fallback` | Update `src/handler/search_telemetry.rs`; fast_search telemetry is manually assembled |
| New backend fields break direct struct literals | Task 6 audits `SearchExecutionParams`, CLI literals, and direct tool literals under `src/tests/**`; add `cargo check` gate |

---

## Memory Notes

After approval and execution, save feedback memories documenting:
- Per-tool resolved-default mapping rationale (Decision 1).
- Explicit-only fallback-note rule (Decision 7).
- Symbol-only contract for fast_search semantic/hybrid (Decision 3).
- Codex review caught: non-blocking `embedding_provider().await`, hybrid_search symbol-only, RRF zero-weight bug, shared.rs flat file, dashboard callers.
