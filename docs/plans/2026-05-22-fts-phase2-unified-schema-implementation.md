# FTS Phase 2 — Unified Schema Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Replace Julie's two-doc-type Tantivy schema with one unified `SearchDocument` schema, collapse the three `execute_*_search` paths into a single mixed-kind query path, simplify the tokenizer to Eros's recipe, and bump the search compat marker to trigger transparent reindex. Close the Eros-bakeoff ranking gap from Julie 267/406 top1 to ≥ 350/406 (stretch ≥ 370).

**Architecture:** Single Tantivy `search_doc` schema with a `kind` discriminator (one row per symbol, one row per file). Seven core FTS fields scored by BM25 in a single sweep, post-ranked by one Eros-recipe reranker over a mixed-kind candidate set. Index-time CamelCase/snake_case splitting lives in a new `pretokenized_code` field; query-time tokenization is simple (lowercase + ascii-fold, no stemming, no affix variants). Migration is transparent via `SEARCH_COMPAT_MARKER_VERSION` bump 3 → 4 (folded into the schema-extension commit): existing workspaces auto-rebuild once on next open.

**Tech Stack:** Rust 1.x · Tantivy · SQLite (canonical store) · tree-sitter extractors (unchanged) · Eros bakeoff harness for acceptance evidence.

**Architecture Quality:** Approved module/interface shape is the unified-schema design at `docs/plans/2026-05-21-fts-phase2-unified-schema-design.md`. Architecture risk is concentrated in three places:
1. Reranker constants — Eros's defaults port directly, but Julie's existing `EXACT_TITLE_BOOST`/`PARTIAL_TITLE_BOOST`/`PATH_BOOST` already match. Empirically validated via bakeoff.
2. Tokenizer simplification — removing CamelCase emission at query time changes recall shape for camel-cased identifiers; mitigation is `pretokenized_code` at index time.
3. Compat marker mechanics — bump triggers `RecreatedIncompatible` on next workspace open. The lead owns acceptance evidence that the reindex completes cleanly under daemon mode.

If code reality contradicts the approved shape, the worker reports a plan mismatch rather than redesigning locally.

---

## Codex Plan-Review (2026-05-22) — fixes folded in

Codex `gpt-5.5 high` reviewed v1 of this plan and returned `needs-rework`. Fixes folded into this v2:

| Finding | Fix |
|---|---|
| **C1.** T1 not additive — adding schema fields changes `compatibility_signature` (`src/search/index.rs:1094,1193`) which triggers `RecreatedIncompatible` *before* T9 marker bump → two rebuilds. | Marker bump 3 → 4 folded into T1. Original T9 removed; subsequent tasks renumbered. One rebuild on next workspace open after T1. |
| **C2.** Schema field-name mismatch: spec uses `name_text/signature_text/doc_text/body_excerpt`; existing schema has `name/signature/doc_comment/code_body`. | Explicit field-name mapping table added in T1. SearchDocument reuses existing schema field names; only `pretokenized_code` and `relationship_text` are net-new. |
| **C3.** T4 breaks live old search — projection emits SearchDocument while old `search_symbols`/`search_files`/`search_content` still active. | `SearchDocument::add_search_doc` writes the **union** of old + new fields (still sets `doc_type="symbol"`/`"file"`, annotations, etc.). Old search paths keep working through T8 cutover. T9 cleans up. |
| **C4.** T8 cannot compile — removing `search_target` from `FastSearchTool` breaks CLI (`src/cli_tools/commands.rs:150-158`) and xtask (`xtask/src/search_matrix.rs:322-332`) which is a workspace member. | T8 is now an atomic cutover commit including CLI + xtask matrix runner + every other `FastSearchTool` constructor. Original T11 (CLI) folded into T8. |
| **C5.** `src/tools/search/line_mode.rs:344-345` still calls deleted `search_content`/`apply_reranker_to_content_results`. | T8 rewires line_mode to the unified path. T9 cleanup verifies no other callers remain. |
| **C6.** `search_target` cleanup misses src/ surfaces: daemon DB (`src/daemon/database.rs:216-223,1164-1222`), dashboard (`search_analysis.rs`/`search_compare.rs`), nl_embeddings, hint_formatter, navigation/formatting, deep_dive error message. | T8 enumerates all surfaces explicitly. Daemon DB column stays (passive historical data); new writes pass empty string — schema unchanged, no migration. Acceptance criterion clarified: "no code references `search_target` as a routing concept" (DB column name as historical-data store is exempt and explicitly documented). |
| **C7.** Eros bakeoff command wrong — CLI is `--candidate` (singular, list-style), and the 406-query corpus is at `~/.eros-eval/eval/multi-lang-corpus/20260521T185725Z-94fe62fedfd5.json` (not the packaged 349-query default). | T0/T13 commands corrected. |
| **I1.** T3 leaves `xtask/src/cli.rs:32-109` `Ablation::{NoStemming, NoCamel, Both}` variants + `xtask/tests/search_matrix_ablation_tests.rs`. | T3 deletes the three Ablation variants and the dedicated test file. Ablation enum + matrix scaffold survives as future tuning vehicle. |
| **I2.** T7 used wrong column names (`source_symbol_id` / `target_symbol_id`); should use existing batch helpers. | T7 corrected to `from_symbol_id` / `to_symbol_id` and references `src/database/relationships.rs:72-134` batch helpers. |
| **I3.** T7 doesn't update watcher call-sites of `apply_uncommitted_documents_from_symbols`. | T7 enumerates `src/watcher/handlers.rs:307-312` and `src/watcher/runtime.rs:465-470`. |
| **I4.** T13 doesn't measure index-size delta. | Added to T13 acceptance criteria. |
| **I5.** Baseline HEAD: spec said `722bdee5` but plan said `70c7c27f`. | Documented: current `main` HEAD is `70c7c27f`. The spec's `722bdee5` predated the Phase 1 merge that landed at `54929142..70c7c27f`. T0 baseline is against `70c7c27f`. Phase 2 HEAD must beat that baseline by ≥ 83 top1 (350 − 267). |
| **N1.** T11 "silently ignore --target": clap auto-errors on unknown. | T11 just removes the flag; clap behavior is correct. |
| **N2.** T4 test forward-references T5 method. | T4 test now uses raw Tantivy assertions only. |
| **N3.** T10 worker acceptance included broad test suites. | T9 (was T10) worker acceptance: by-name tests only. Affected-change/dev-tier runs are lead-owned. |

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` and `docs/TESTING_GUIDE.md` define xtask tier semantics. `RAZORBACK.md` defines model routing and gate ownership.

**Worker red/green scope:** narrowest test that proves the new or changed behavior. Default:
- `cargo nextest run --lib <exact_test_name> 2>&1 | tail -20`
- For compilation-only checks during incremental builds: `cargo check 2>&1 | tail -20`.

**Worker ceiling:** workers do not run `cargo xtask test changed`, `cargo xtask test dev`, or any broader tier. Workers do not own branch-gate or bakeoff verification.

**Worker gate invariant (per task):** stated in each task's acceptance criteria. Typical: "the new test fails before the change and passes after; no other listed tests regress when run by name."

**Lead affected-change scope:** `cargo xtask test changed` after each coherent batch of subagent commits.

**Branch gate (lead-owned):** `cargo xtask test dev` plus `cargo xtask test dogfood` plus `cargo xtask search-matrix run --profile smoke` before the Eros bakeoff acceptance run.

**Replay/metric evidence (lead-owned):** Eros bakeoff harness against the multi-lang corpus referenced in the spec.

- Corpus path: `~/.eros-eval/eval/multi-lang-corpus/20260521T185725Z-94fe62fedfd5.json` (406 queries; `latest.json` symlink points here).
- Command (lead-owned, run from `~/source/eros`):
  ```
  uv run eros eval bakeoff --candidate lancedb-fts --corpus ~/.eros-eval/eval/multi-lang-corpus/latest.json --julie-command auto --limit 5
  ```
- Hard gate: `julie-cli top1 ≥ 350/406` (stretch ≥ 370).
- Report-only: per-category breakdown (exact symbol / symbol intent / file-path / docs-phrase / test-intent / likely-test), p50/p95 latency, on-disk index size delta vs baseline.
- Baseline: current `main` HEAD `70c7c27f` (Phase 1 has merged; the spec's `722bdee5` reference predated that merge). T0 ledgers the exact corpus version + Julie baseline top1.

**Escalation triggers:**
- Two worker attempts fail review on the same task → escalate to coupled-implementation tier or lead.
- Tokenizer change degrades any per-category breakdown by ≥ 10 top1 vs baseline → escalation to strategy tier (reranker tuning sweep using the search-matrix harness).
- Compat-marker reindex fails to trigger on existing workspaces in daemon mode → escalation (lifecycle work).

**Assigned verification failure:** workers stop and report when assigned verification fails. They never paper over a red test by widening the gate.

**Verification ledger:** each task's acceptance commit records `{invariant, command, scope label, commit SHA, result, timestamp}` in this plan's ledger section. For the bakeoff, also record hard-gate metrics (top1, per-category top1) and report-only metrics (latency, index size delta).

---

## Model Routing

**Project source of truth:** `RAZORBACK.md` § Model Routing.

Search ranking, scoring, tokenization, and query semantics are shared-invariant work in this repo. Per RAZORBACK.md, bounded edits in this area bump to medium/high tier; concurrency/contract/correctness work uses high/xhigh.

**Strategy tier:** plan authoring, lead diff review, bakeoff interpretation, reranker tuning decisions.
- Codex: `gpt-5.5 medium/high`
- Claude: Opus
- OpenCode: strongest reasoning model

**Implementation tier (default for bounded worker tasks below):**
- Codex: `gpt-5.5 medium` (bumped from `low` because this area is shared-invariant)
- Claude: Sonnet high (subagent default in Claude Code)
- OpenCode: strong implementation model

**Mechanical tier (docs/skills sweep only):**
- Codex: `gpt-5.4-mini medium`
- Claude: Sonnet (Haiku acceptable for pure text sweeps)
- OpenCode: fast reliable model

**Gate-interpretation reviewer (codex review of plan + implementation):**
- Codex: `gpt-5.5 high`

**Escalation tier (tokenizer correctness, reranker constants, compat-marker lifecycle, repeated worker failure):**
- Codex: `gpt-5.5 high/xhigh`
- Claude: Opus
- OpenCode: strongest reasoning model

**Worker eligibility:** implementation tier eligible for tasks whose write scope is one file or a closely related cluster, acceptance criteria are testable in isolation, no hidden shared invariants. T3 (tokenizer), T6 (reranker), and T8 (atomic cutover) require coupled-implementation tier or lead execution because they own shared invariants and span many files.

**Mechanical exclusion:** mechanical-tier workers do not own failing tests or bakeoff metrics. T11 (docs/skill text sweep) is mechanical only.

**Harness-specific:** dispatched via Claude Code Agent tool, `model: sonnet` for implementation, `model: opus` for lead execution and coupled tasks.

---

## File Structure

**Files created:**
- `docs/plans/2026-05-22-fts-phase2-unified-schema-implementation.md` (this plan)
- New tests as listed per task.

**Files modified (primary):**
- `src/search/schema.rs` — add `pretokenized_code`, `relationship_text` fields; signature picks up the additions automatically.
- `src/search/index.rs` — bump `SEARCH_COMPAT_MARKER_VERSION` 3 → 4 (T1); add `SearchDocument` struct + `add_search_doc` method writing union of old + new fields (T2); in T9 cleanup delete `SymbolDocument`/`FileDocument`/`search_symbols`/`search_files`/`search_content` and the standalone Phase 1 helpers.
- `src/search/tokenizer.rs` — collapse `CodeTokenizer` to a thin simple-tokenizer wrapper + index-time `pretokenized_code` emitter; delete `ablate_stemming`/`ablate_camel_emit` fields and the env-var reads; delete the corresponding `TokenizerCompatibilitySignature` payload entries.
- `src/search/reranker.rs` — single `rerank_unified` over mixed-kind candidates with Eros-recipe field scores (T6); T9 deletes `rerank_symbol_score`, `rerank_content_score` and the per-target helpers.
- `src/search/projection/apply.rs` — emit `SearchDocument`s with the union shape (T4); take `&SymbolDatabase` for `relationship_text` population (T7); T9 drops `SymbolDocument`/`FileDocument` arg types.
- `src/search/projection.rs` — adjust types touched by projection changes.
- `src/search/query.rs` — add `build_unified_query` (T5); T9 deletes the per-target builders if no callers remain.
- `src/tools/search/execution.rs` — add `execute_search_unified` (T5); T8 wires it to `FastSearchTool`; T9 deletes the three per-target functions and the `search_target` field on `SearchExecutionParams`.
- `src/tools/search/target.rs` — deleted in T9.
- `src/tools/search/mod.rs` — T8 deletes `search_target` from `FastSearchTool` and `FastSearchToolSerde`, drops `validated_search_target`, `default_search_target`, and the `missing_index_message(SearchTarget,…)` signature.
- `src/tools/search/text_search.rs` — T5 adds `unified_search_impl`; T9 deletes `definition_search_with_index`, `content_search_with_index`, the `RRF_TO_BM25_SCALE` constant + RRF fusion, the two `apply_reranker_to_*_results` functions.
- `src/tools/search/line_mode.rs` — T8 rewires to the unified path. file-row hits trigger line-content materialization; symbol-row hits go through the symbol formatter.
- `src/handler/search_telemetry.rs` — T8 drops `search_target` from telemetry payload, adds `kind_distribution`.
- `src/handler/tools/fast_search.rs` — T8 rewrites the tool description (no `search_target`).
- `src/cli_tools/subcommands.rs` — T8 removes `--target` flag on the `Search` subcommand.
- `src/cli_tools/commands.rs` — T8 drops `target` plumbing.
- `src/cli_tools/generic.rs` — T8 updates JSON-payload tests to drop `search_target`.
- `src/tools/search/nl_embeddings.rs` — T8 drops the `search_target == "definitions"` branch (the gate becomes "are we looking at a symbol-kind hit?" instead).
- `src/tools/search/hint_formatter.rs` — T8 rewrites hint text to drop `search_target` suggestions.
- `src/tools/navigation/formatting.rs` — T8 rewrites "Try fast_search(query=..., search_target=...)" suggestions.
- `src/tools/deep_dive/mod.rs` — T8 same.
- `src/daemon/database.rs` — T8 stops populating the `search_target` column in `tool_call_history` writes (pass `""`). Schema column stays for historical rows; no migration. Documented explicitly as a passive store.
- `src/dashboard/search_analysis.rs`, `src/dashboard/search_compare.rs` — T8 simplifies dashboards to drop `search_target` faceting where it drove behavior; remaining historical group-bys read the DB column directly (passive use).
- `JULIE_AGENT_INSTRUCTIONS.md` — T11 rewrites `fast_search` section.
- `.claude/skills/search-debug/SKILL.md`, `.claude/skills/impact-analysis/SKILL.md`, `.claude/skills/dead-code-audit/SKILL.md` — T11 deletes `search_target` references.
- `docs/SEARCH_FLOW.md`, `docs/ARCHITECTURE.md`, `docs/INTELLIGENCE_LAYER.md` — T11 sweep.
- `xtask/src/cli.rs` — T3 deletes `Ablation::NoStemming`, `Ablation::NoCamel`, `Ablation::Both` enum variants (env-var-driven). Default `Ablation::None` retained; future tuning variants are out of scope here (they'd be added by T13 if invoked).
- `xtask/src/search_matrix.rs` — T8 drops `search_target` from `FastSearchTool` construction; YAML cases keep the field as a corpus tag (passive use, reported in baseline JSON only).
- `xtask/src/search_ablation.rs` — T9 switches `julie::search::{FileDocument, SymbolDocument}` imports to `SearchDocument`.

**Files deleted:**
- `src/tools/search/target.rs` (T9)
- `src/tests/tools/search/tokenizer_ablation_tests.rs` (T3 — env vars gone, test obsolete)
- `xtask/tests/search_matrix_ablation_tests.rs` (T3 — asserts the deleted env-var-driven Ablation variants)

**Files preserved unchanged (per spec):**
- `xtask/src/search_matrix.rs` reporting structure, `xtask/src/search_matrix_report.rs`, `xtask/src/search_matrix_mine.rs` — matrix runner + `--ablation` CLI scaffold survives as the tuning vehicle.

---

## Acceptance Criteria (from spec, restated; codex-clarified)

1. **No `search_target` as a routing concept in `src/` or `xtask/`.** The daemon DB column name stays (it's a passive store of historical rows; new writes pass empty string), but no code reads it to drive dispatch. Matrix YAML cases retain the field for grouping reports only. `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/skills/`, and `docs/` (outside historical investigation/results) have no `search_target` references.
2. One Tantivy doc type: `SearchDocument` replaces `SymbolDocument` + `FileDocument`. One `add_*` index method (`add_search_doc`).
3. One query path: `execute_search_unified` drives all of `fast_search`. The three per-target functions are deleted.
4. One reranker function applies the Eros-recipe boosts to a mixed-kind candidate set. Phase 1 cross-target helpers folded in; the standalone helpers deleted.
5. `CodeTokenizer` is gone or a thin wrapper. Stemming is removed. CamelCase/snake_case splitting lives only in `pretokenized_code` at index time. `JULIE_ABLATE_STEMMING` and `JULIE_ABLATE_CAMEL_EMIT` are deleted (env-var reads, code, tests, and the xtask `Ablation` variants that used them).
6. `SEARCH_COMPAT_MARKER_VERSION` = 4 (bumped in T1 alongside the schema change). Existing workspaces auto-rebuild once on next open. The lead verifies rebuild trigger in daemon mode.
7. **Eros bakeoff:** `julie-cli top1 ≥ 350/406` on the 406-query corpus at `~/.eros-eval/eval/multi-lang-corpus/latest.json` (stretch ≥ 370). Recorded in the verification ledger with commit SHA.
8. `cargo xtask test dev` and `cargo xtask test dogfood` both green at Phase 2 HEAD.
9. `cargo xtask search-matrix run --profile smoke` green.
10. Index on-disk size growth ≤ 2× baseline (report-only soft signal; if exceeded, the lead invokes T13 to lower the relationship_text cap).

---

## Plan-author Decisions on Spec Pushback Points

1. **Acceptance gate at top1 ≥ 350 (stretch 370).** Binding. Eros lancedb-fts = 374 on this corpus; 350 gives ~6% headroom for Julie-specific overhead. If Phase 2 fails 350, the lead invokes T13 tuning sweep before merge.
2. **`relationship_text` on file-rows is empty in v1.** Simpler, measurable later. Symbol-row relationships carry the dominant signal.
3. **`JULIE_ABLATE_STEMMING` and `JULIE_ABLATE_CAMEL_EMIT` are deleted** along with the dedicated `tokenizer_ablation_tests.rs`, `xtask/tests/search_matrix_ablation_tests.rs`, and the three `Ablation::{NoStemming, NoCamel, Both}` enum variants in `xtask/src/cli.rs`. The `Ablation::None` default and the matrix runner scaffold survive as the future tuning vehicle.

---

## Schema Field Mapping (spec name → existing schema field)

| Spec field (design doc) | Existing schema field (`src/search/schema.rs`) | Net-new in T1? |
|---|---|---|
| `id` | `id` | no |
| `kind` | `kind` | no |
| `name_text` | `name` | no |
| `path_text` | `path_text` | no |
| `signature_text` | `signature` | no |
| `doc_text` | `doc_comment` | no |
| `relationship_text` | `relationship_text` | **yes** |
| `body_excerpt` (≤ 2000 bytes) | `code_body` (truncated at write time) | no |
| `pretokenized_code` | `pretokenized_code` | **yes** |
| `language` | `language` | no |
| `file_path` | `file_path` | no |
| `basename` | `basename` | no |
| `start_line` | `start_line` | no |
| `role` | `role` | no |
| `test_role` | `test_role` | no |
| `doc_type` (transitional, T1–T8 sets it; T9 may remove read-paths but the field stays in schema) | `doc_type` | no |

`code_body` is reused for file rows (storing truncated file content). The existing `content` field also stays — write paths populate `code_body`; `content` is unused after T4 but the field remains to avoid changing schema signature again. Annotation fields (`annotations_exact`, `annotations_text`, `owner_names_text`) are preserved unchanged.

---

## Task Plan

Tasks execute in ID order. T0 is read-only baseline. T1–T7 form the additive build-out (each must `cargo check` clean; old paths remain live because SearchDocument writes the union of old + new fields). T8 is the atomic cutover commit. T9 is the cleanup deletion commit. T10–T11 are remaining surface sweeps. T12 is the branch-gate + bakeoff acceptance. T13 is the contingent tuning sweep.

### T0 — Eros bakeoff baseline against main HEAD

**Files:**
- Read: `~/.eros-eval/eval/multi-lang-corpus/latest.json` (the 406-query corpus referenced by the spec)
- Append to: this plan § Verification Ledger.

**What to build:** A reproducible baseline on `main` at HEAD `70c7c27f`. No code changes. Produces the number Phase 2 must beat.

**Approach:**
1. From `~/source/eros`, run:
   ```
   uv run eros eval bakeoff --candidate lancedb-fts \
     --corpus ~/.eros-eval/eval/multi-lang-corpus/latest.json \
     --julie-command auto --limit 5 --progress
   ```
2. Capture: `total_queries` (expect 406), `julie-cli top1`, `lancedb-fts top1`, per-category breakdown.
3. Save the raw artifact under `~/.local/state/eros/eval/bakeoff/`; copy a slim summary into this plan's verification ledger.

**Acceptance criteria:**
- [ ] Bakeoff completes; artifact path recorded.
- [ ] `total_queries == 406`. If different, halt and reconcile before continuing.
- [ ] Julie top1 baseline number ≤ 280 (sanity bound; if drifted significantly upward, note it).
- [ ] Lancedb-fts top1 ≥ 360 (sanity bound around the 374 reference; if drifted significantly downward, escalate — Eros may have regressed and the gate needs recalibration).
- [ ] Verification ledger row recorded with `{corpus_path, commit_sha=70c7c27f, julie_top1, eros_lancedb_fts_top1, per_category_breakdown, timestamp}`.

**Worker tier:** strategy (interprets baseline; lead-owned).

---

### T1 — Extend schema with `pretokenized_code` + `relationship_text` AND bump compat marker 3 → 4

**Files:**
- Modify: `src/search/schema.rs`
- Modify: `src/search/index.rs` (compat marker constant only)
- Test: `src/tests/tools/search/schema_phase2_fields_test.rs` (new)
- Test: `src/tests/tools/search/compat_marker_v4_test.rs` (new)

**What to build:** Add the two net-new fields. Bump `SEARCH_COMPAT_MARKER_VERSION` in the same commit. Existing workspaces rebuild **once** on next open (schema signature changes + marker version bumps in one transition).

**Approach:**
1. In `fields` module add: `pub const PRETOKENIZED_CODE: &str = "pretokenized_code"; pub const RELATIONSHIP_TEXT: &str = "relationship_text";`.
2. In `create_schema()`, register both as `TEXT` (not stored), `TextFieldIndexing::default()` with `IndexRecordOption::WithFreqsAndPositions` and `tokenizer("simple")` (T3 may rename the tokenizer registration; keep `"simple"` here until then).
3. In `SchemaFields`, add `pub pretokenized_code: Field` and `pub relationship_text: Field`. Populate them in `SchemaFields::new`.
4. Change `const SEARCH_COMPAT_MARKER_VERSION: u32 = 3;` to `4`.

**Acceptance criteria:**
- [ ] `cargo check` clean.
- [ ] `schema_phase2_fields_test`: `schema.get_field("pretokenized_code")` and `schema.get_field("relationship_text")` both resolve.
- [ ] `schema_phase2_fields_test`: `compatibility_signature(&new) != compatibility_signature(&old)` (signature is sensitive to new fields).
- [ ] `compat_marker_v4_test::detects_v3_marker_as_stale`: writing a `marker_version: 3` marker file and opening returns `SearchIndexOpenDisposition::RecreatedIncompatible`.
- [ ] `compat_marker_v4_test::accepts_v4_marker`: opening a freshly-created (`marker_version: 4`) index returns `Compatible`.
- [ ] Worker-scope tests pass.

**Worker tier:** implementation. (Schema change + one-line constant bump; well-bounded.)

---

### T2 — Add `SearchDocument` struct + `add_search_doc` method (union shape)

**Files:**
- Modify: `src/search/index.rs`
- Test: `src/tests/tools/search/unified_doc_index_test.rs` (new)

**What to build:** New `SearchDocument` + `SearchIndex::add_search_doc` that writes the **union** of old + new fields: `id`, `name`, `signature`, `doc_comment`, `code_body` (≤ 2000 bytes), `pretokenized_code` (empty in T2 — populated by T3), `relationship_text` (empty — populated by T7), `language`, `file_path`, `basename`, `kind`, `start_line`, `role`, `test_role`, plus `doc_type = "symbol" | "file"` and annotation fields. Old `SymbolDocument`/`FileDocument`/`add_symbol`/`add_file_content` stay alive — they're called by T4's transitional projection.

**Approach:**
1. Define `pub struct SearchDocument { ... }` near existing doc structs.
2. Constructors: `SearchDocument::for_symbol(symbol: &Symbol) -> Self` (kind = symbol.kind.canonical_str(), doc_type = `"symbol"`), `SearchDocument::for_file(file_info: &FileInfo) -> Self` (kind = `"file"`, doc_type = `"file"`, name = basename without extension).
3. `code_body` truncation: ≤ 2000 bytes on a UTF-8-safe boundary. Use `text.floor_char_boundary(2000)` if stable; otherwise iterate `char_indices` and slice at the last boundary ≤ 2000.
4. `add_search_doc` writes all fields, identical in coverage to `add_symbol_with_context` + `add_file_content` union. Does NOT call commit (callers batch).

**Acceptance criteria:**
- [ ] `cargo check` clean.
- [ ] Indexes a synthetic `SearchDocument` with `kind="function"`; raw Tantivy assertion that `name` field is searchable and the document is retrievable.
- [ ] Indexes a synthetic `SearchDocument` with `kind="file"`; raw Tantivy assertion that `path_text` matches the path and `name` matches the basename.
- [ ] `body_excerpt` truncation: input 4 KB, assert `code_body` field length ≤ 2000 and slice is on a UTF-8 boundary.
- [ ] Worker-scope test passes.

**Worker tier:** implementation.

---

### T3 — Simplify tokenizer + delete env-var ablations and the xtask variants that use them

**Files:**
- Modify: `src/search/tokenizer.rs`
- Modify: `xtask/src/cli.rs` (delete three Ablation variants)
- Delete: `src/tests/tools/search/tokenizer_ablation_tests.rs`
- Delete: `xtask/tests/search_matrix_ablation_tests.rs`
- Test: `src/tests/tools/search/tokenizer_simple_test.rs` (new)
- Test: `src/tests/tools/search/pretokenized_emit_test.rs` (new)

**What to build:**
1. `SimpleCodeTokenizer` doing lowercase + ascii-fold + `max_token_length: 80`. Registered under name `"simple"` so existing schema registrations continue to point at it (or registered under `"simple_code"` and the schema rewires; pick the path that minimizes churn — `"simple"` is already a Tantivy built-in, so a new name `"simple_code"` is cleaner and avoids overriding the built-in).
2. Free function `pub fn pretokenize_code(text: &str) -> String` that, for each whitespace-separated token, emits `original` then space-joined CamelCase split then space-joined snake_case split. Reuses existing `split_camel_case` and `split_snake_case`.
3. Delete `ablate_stemming`/`ablate_camel_emit` fields on `CodeTokenizer`. Delete `env::var("JULIE_ABLATE_STEMMING")`/`env::var("JULIE_ABLATE_CAMEL_EMIT")` reads. Delete the corresponding `TokenizerCompatibilitySignature` payload entries (`ablate_stemming`, `ablate_camel_emit`).
4. In `tokenize_code`, delete the stemmer invocation. The other emission logic stays for now; T4/T8/T9 may simplify further as `CodeTokenizer`'s call sites are rewired.
5. Delete the `Ablation::NoStemming`, `Ablation::NoCamel`, `Ablation::Both` variants in `xtask/src/cli.rs` along with their `set_env_vars`/`parse` arms. `Ablation::None` stays.

**Acceptance criteria:**
- [ ] `cargo check` clean (both `src/` and `xtask/`).
- [ ] `tokenizer_simple_test`: input `"getUserData_v2"` → token stream `["getuserdata_v2"]` (one token, lowercased; no camel/snake splits, no stem).
- [ ] `pretokenized_emit_test`: `pretokenize_code("getUserData_v2")` contains `"getuserdata_v2"`, `"get"`, `"user"`, `"data"`, `"v2"`.
- [ ] `cargo nextest run --lib tokenizer_ablation` reports no matching tests (file is gone).
- [ ] Worker-scope tests pass.

**Worker tier:** coupled-implementation (tokenizer correctness + xtask test deletion; shared-invariant). Lead reviews diff inline. Codex `gpt-5.5 high` if delegated.

---

### T4 — Switch `projection/apply.rs` to emit `SearchDocument`s (with union write)

**Files:**
- Modify: `src/search/projection/apply.rs`
- Modify: `src/search/projection.rs` (signatures touched)
- Test: `src/tests/tools/search/projection_search_doc_test.rs` (new)

**What to build:** When projecting a batch, build `Vec<SearchDocument>` and call `add_search_doc` for each. Because SearchDocument writes the union shape (still sets `doc_type` etc.), old `search_symbols`/`search_content`/`search_files` continue to return correct results after T4.

**Approach:**
1. Internal helper `build_search_docs_from_symbols(symbols: &[Symbol], symbol_contexts: &HashMap<...>) -> Vec<SearchDocument>` populates the union shape. `relationship_text = ""` in T4 (populated by T7). `pretokenized_code` is set via `pretokenize_code(format!("{} {} {}", name, signature, body_excerpt))`.
2. For file rows: one per file, `kind="file"`, `doc_type="file"`, `name = basename_without_extension(path)`, `code_body = first_2000_bytes_utf8(content)`, `pretokenized_code = pretokenize_code(content)`, `relationship_text = ""`.
3. `apply_documents` / `apply_documents_with_context` / `apply_uncommitted_documents_from_symbols` keep their public signatures. Internally each builds `SearchDocument`s and calls `add_search_doc`. The old `add_symbol_with_context`/`add_file_content` paths are no longer called from inside these helpers (but they still exist for backward-compat code that may import them directly; T9 deletes them).
4. Single-write only: the projection writes one `SearchDocument` per symbol/file (not also writing the old shape). The new SearchDocument's union coverage is what keeps old read paths working.

**Acceptance criteria:**
- [ ] `cargo check` clean.
- [ ] `projection_search_doc_test::symbol_indexable`: project a fixture with one symbol; raw Tantivy assertion that the symbol is retrievable via the `name` field.
- [ ] `projection_search_doc_test::file_row_indexable`: project a fixture with one file; raw Tantivy assertion that `kind = "file"` and `basename` matches.
- [ ] `projection_search_doc_test::old_path_still_works`: after projection, calling `index.search_symbols(name, &filter, 5)` returns the projected symbol (proves union shape preserves old-path behavior).
- [ ] Worker-scope test passes; existing projection tests still pass when run by name.

**Worker tier:** coupled-implementation (touches indexing path that the watcher uses). Lead reviews diff inline.

---

### T5 — Add unified query path (`build_unified_query` + `search_unified` + `execute_search_unified`)

**Files:**
- Modify: `src/search/query.rs` (add `pub fn build_unified_query(...)`)
- Modify: `src/search/index.rs` (add `pub fn search_unified(...)`)
- Modify: `src/tools/search/execution.rs` (add `pub async fn execute_search_unified(...)`)
- Modify: `src/tools/search/text_search.rs` (add `pub async fn unified_search_impl(...)`)
- Test: `src/tests/tools/search/unified_query_path_test.rs` (new)

**What to build:** One BM25 sweep across seven core FTS fields, returning a mixed-kind `Vec<UnifiedHit>` carrying `kind` per hit. Old `search_symbols`/`search_files`/`search_content` stay alive.

**Approach:**
1. `build_unified_query(query, fields, normalized_terms) -> BooleanQuery`: BooleanQuery over `name`, `path_text`, `signature`, `doc_comment`, `relationship_text`, `code_body`, `pretokenized_code` with per-field weights from Eros's `_field_score` defaults (`name` heaviest, `pretokenized_code` and `code_body` lighter). No `doc_type` filter — mixed kinds.
2. `SearchIndex::search_unified(query_str, filter, limit) -> Vec<UnifiedHit>`: runs the query, collects `TopDocs::with_limit(limit * NL_RERANK_OVERFETCH_FACTOR)`, materializes hits with kind/name/path/signature/doc/body/pretok/relationships/language/basename/start_line/role/test_role/tantivy_score.
3. `execute_search_unified`: async wrapper, applies rerank (in T5 uses existing `rerank_symbol_score` as a placeholder if T6 not yet merged — when T6 lands, swap to `rerank_unified`).
4. `unified_search_impl`: wraps workspace routing (reuses helpers from `text_search_impl`).

**Acceptance criteria:**
- [ ] `cargo check` clean.
- [ ] `unified_query_path_test::returns_mixed_kinds`: project 3 docs (function, class, file row); single query returns all three in score-ranked order with `kind` preserved.
- [ ] `unified_query_path_test::file_exact_beats_symbol_partial`: index a file with basename `"browser_client.py"` and a symbol with name containing `"browser_client"`; query `"browser_client"`; assert the file row scores ≥ the symbol partial (validates per-kind weighting wiring; final ordering is up to the reranker but the file hit must be present in top-3).
- [ ] Worker-scope tests pass.

**Worker tier:** implementation.

---

### T6 — Add `rerank_unified` absorbing Phase 1 cross-target helpers

**Files:**
- Modify: `src/search/reranker.rs`
- Modify: `src/tools/search/execution.rs` (`execute_search_unified` switches from `rerank_symbol_score` placeholder to `rerank_unified`)
- Test: `src/tests/tools/search/unified_reranker_test.rs` (new)

**What to build:** `pub fn rerank_unified(query: &ParsedQuery, candidates: &[Candidate]) -> Vec<Ranked>`. Eros-recipe field-score boosts on mixed-kind candidates. Phase 1 cross-target helpers (`apply_symbol_title_boost_to_file_results` at `src/search/index.rs:1690`, the title-exact block at `src/tools/search/text_search.rs:494-507`) get folded in. Old `rerank_symbol_score`/`rerank_content_score` and the standalone Phase 1 helpers stay alive until T9 cleanup.

**Approach:**
1. Per-candidate score = `tantivy_score + sum(field_boosts) − role_demotion(c)`.
2. Field boosts (compact-form matching via `compact_alnum_lc`):
   - `name == normalized_query` (full-string match): `+EXACT_TITLE_BOOST (100) + kind_boost(c.kind)`.
   - `kind == "file"` && `query ∈ {basename, stem(basename)}`: `+120`.
   - else `query ∈ {path, basename, stem(basename)}`: `+PATH_BOOST (40)`.
   - Per-term: title exact `+100`, title partial `+50`, path-fragment `+25`, basename exact `+40`, `kind=="file" && term == stem(basename)` `+30`.
3. `role_demotion(c)` already exists at `src/search/reranker.rs:402` — reuse.
4. Wire `execute_search_unified` to call `rerank_unified(&parsed_query, &candidates)`.

**Acceptance criteria:**
- [ ] `cargo check` clean.
- [ ] `unified_reranker_test::exact_name_beats_partial`: candidate with `name == "BrowserClient"` matching `"BrowserClient"` scores ≥ partial by `EXACT_TITLE_BOOST` (≥ 100).
- [ ] `unified_reranker_test::file_basename_exact_beats_other_file_path_fragment`: file-row candidate with basename `"browser_client.py"` matching `"browser_client"` beats a file-row at `"src/utils/helper.py"` by ≥ 80.
- [ ] `unified_reranker_test::vendor_demoted`: candidate with `role == "vendor"` is demoted vs same-score `src` candidate (uses existing `VENDOR_PENALTY`).
- [ ] Worker-scope tests pass.

**Worker tier:** coupled-implementation (reranker constants are shared-invariant). Lead reviews diff inline.

---

### T7 — Populate `relationship_text` from the relationships table (symbol rows only)

**Files:**
- Modify: `src/search/projection/apply.rs`
- Modify: `src/watcher/handlers.rs` (call-site of `apply_uncommitted_documents_from_symbols`)
- Modify: `src/watcher/runtime.rs` (same)
- Test: `src/tests/tools/search/relationship_text_test.rs` (new)

**What to build:** At symbol projection time, query `relationships` for edges where `from_symbol_id == symbol.id OR to_symbol_id == symbol.id`. Collect related symbols' names into a deduplicated space-separated blob, capped at 512 bytes (truncated on whitespace boundary). File rows keep `relationship_text = ""`.

**Approach:**
1. `pub fn collect_relationship_names_bounded(db: &SymbolDatabase, symbol_ids: &[&str], max_bytes_per: usize) -> HashMap<String, String>` in `src/search/projection/apply.rs` (or a new sibling helper). Implementation uses the existing batch helpers at `src/database/relationships.rs:72-134` (`get_relationships_from_batch`, `get_relationships_to_batch`) to fetch in one round trip; joins to `symbols` to get related names; deduplicates and joins with spaces; truncates per symbol at `max_bytes_per` on the last whitespace boundary.
2. `build_search_docs_from_symbols` now takes a `&HashMap<String, String>` (precomputed relationship blobs); `apply_documents_with_context` / `apply_uncommitted_documents_from_symbols` precompute the map once per batch before calling `build_search_docs_from_symbols`.
3. Watcher call sites:
   - `src/watcher/handlers.rs:307-312` (the path that calls `apply_uncommitted_documents_from_symbols`): pass `&db` so projection can fetch relationships. If the watcher already holds a DB handle (it does — `db: Arc<StdMutex<SymbolDatabase>>` at `src/watcher/mod.rs:69`), thread it through.
   - `src/watcher/runtime.rs:465-470`: same.
4. Cap rationale: codex review measured p95 = ~213 bytes, p99 = ~969 bytes, max ~29 KB on Julie's own fixture data. 512 bytes covers the p95 cleanly without ballooning the index for outlier symbols.

**Acceptance criteria:**
- [ ] `cargo check` clean.
- [ ] `relationship_text_test::related_symbol_indexed`: fixture where symbol A calls symbol B; assert A's `relationship_text` contains B's name; query for B's name returns A in the top hits.
- [ ] `relationship_text_test::cap_enforced`: insert a symbol with 200 fake relationships; assert resulting `relationship_text.len() <= 512`.
- [ ] `relationship_text_test::file_rows_empty`: assert file-kind SearchDocuments have `relationship_text == ""`.
- [ ] Worker-scope tests pass.

**Worker tier:** implementation.

---

### T8 — Atomic cutover: wire `FastSearchTool` to `execute_search_unified` and sweep all `search_target` routing callsites

**Files:**
- Modify: `src/tools/search/mod.rs` (delete `search_target` from `FastSearchTool` + serde shadow + `validated_search_target` + `default_search_target` + helpers; wire to `unified_search_impl`)
- Modify: `src/tools/search/execution.rs` (delete `search_target` field on `SearchExecutionParams`)
- Modify: `src/tools/search/line_mode.rs` (replace `index.search_content` + `apply_reranker_to_content_results` calls with unified-path equivalents that handle file-row hits → render lines from file content; symbol-row hits → symbol formatter)
- Modify: `src/handler/search_telemetry.rs` (drop `search_target` payload field; add `kind_distribution: HashMap<String, u32>`; drop `infer_intent`'s `search_target` arg)
- Modify: `src/handler/tools/fast_search.rs` (rewrite `description = ...` string; drop `search_target` examples)
- Modify: `src/tools/search/nl_embeddings.rs` (rewire the `search_target == "definitions"` gate to "candidate kind is symbol-like")
- Modify: `src/tools/search/hint_formatter.rs` (rewrite hint text)
- Modify: `src/tools/navigation/formatting.rs` (rewrite "Try fast_search(...)" suggestions; drop `search_target` references)
- Modify: `src/tools/deep_dive/mod.rs` (same)
- Modify: `src/daemon/database.rs` (stop populating `search_target` in `tool_call_history` writes — pass `""`. Keep the column. Comment the field as historical.)
- Modify: `src/dashboard/search_analysis.rs`, `src/dashboard/search_compare.rs` (drop `search_target` from behavior; reads of the DB column for historical group-by stay as passive data display)
- Modify: `src/cli_tools/subcommands.rs` (remove `--target` flag on `Search` subcommand and its examples)
- Modify: `src/cli_tools/commands.rs` (drop `target` plumbing; tests update; `julie-server search "<q>"` now does one unified call)
- Modify: `src/cli_tools/generic.rs` (drop `search_target` keys in JSON-payload tests)
- Modify: `xtask/src/search_matrix.rs` (drop `search_target` from `FastSearchTool` construction at lines 322-332; YAML cases keep the field; report JSON keeps the column for grouping)
- Test: `src/tests/tools/search/fast_search_unified_cutover_test.rs` (new)
- Test: `src/tests/cli/cli_search_no_target_test.rs` (new — verifies `julie-server search "main"` works without `--target` and rejects `--target` with a clap error)

**What to build:** One atomic commit that makes the new path the only path. After T8, every caller goes through `execute_search_unified`. Old `search_symbols`/`search_files`/`search_content` are unreachable but still exist (deleted in T9).

**Approach:**
1. Remove `pub search_target: String` from `FastSearchTool` + `FastSearchToolSerde`. Remove `default_search_target`, `validated_search_target`, `missing_index_message(SearchTarget, ...)` signature → becomes `missing_index_message(workspace_id: Option<&str>) -> String`.
2. `execute_with_trace` calls `unified_search_impl` directly. The line-mode branch is keyed off `kind == "file"` per-hit, not off a per-call target. When unified returns a mix of symbol and file hits, line_mode materializes file content for file hits and the symbol formatter handles symbol hits.
3. `record_fast_search`: drop `"search_target"` JSON key; add `"kind_distribution": {...}` computed from the response hits.
4. `fast_search.rs`: rewrite tool description (no `search_target`).
5. NL embeddings: `src/tools/search/nl_embeddings.rs:39-47` currently checks `search_target == "definitions"` to decide whether to apply NL embedding refinement. New gate: apply NL embedding refinement when the query parses as NL-like (`is_nl_like_query(query)`); don't gate on the deleted parameter.
6. Hint/navigation/deep_dive formatting: rewrite messages to suggest `fast_search(query="...")` without a target.
7. Daemon DB: in the writer at `src/daemon/database.rs:831-841`, write `""` for the `search_target` column. The column stays NOT NULL — empty string is a valid value. Add a comment that the column is historical and new writes pass `""`.
8. Dashboard: where `search_target` drives logic (e.g., `search_compare.rs` may group queries by target for comparison panels), simplify the dashboard to drop those facets where they no longer make sense, OR keep them as passive read-only displays of historical data. Lead reviews diff to confirm: no behavioral dependency on `search_target` in new code paths.
9. CLI: drop `--target` from `subcommands.rs`. Drop `target` plumbing in `commands.rs`. `cli_tools/generic.rs` JSON-payload tests update to drop the field.
10. xtask: `search_matrix.rs:322-332` constructs `FastSearchTool`; drop `search_target` key. The YAML `case.search_target` field is still parsed and emitted to the report for grouping, but never passed to Julie.

**Acceptance criteria:**
- [ ] `cargo check` clean across `src/` and `xtask/`.
- [ ] `fast_search_unified_cutover_test::mixed_kinds`: `FastSearchTool { query: "BrowserClient", ... }` returns both symbol and file hits when both exist.
- [ ] `cli_search_no_target_test::no_target_flag`: `julie-server search "main" --workspace . --standalone --json` succeeds.
- [ ] `cli_search_no_target_test::target_flag_rejected`: passing `--target definitions` exits non-zero with clap "unknown argument" (or equivalent).
- [ ] `grep -rn "search_target" src/ --include='*.rs'` returns only: (a) the daemon DB column name in `src/daemon/database.rs` (documented), (b) any dashboard read-only display of the historical column.
- [ ] `grep -rn "search_target" xtask/ --include='*.rs'` returns only: matrix YAML case struct field and report serialization.
- [ ] `fast_search_regression_tests` tests pass when run by name (or are updated to drop `search_target` arguments).
- [ ] Worker-scope tests pass.

**Worker tier:** coupled-implementation (public-API contract change spans many files). Lead executes or delegates with Codex `gpt-5.5 high`.

---

### T9 — Delete old paths: `SymbolDocument`, `FileDocument`, three execute paths, per-target rerankers, `SearchTarget` enum, RRF fusion

**Files:**
- Modify: `src/search/index.rs` (delete `SymbolDocument`, `FileDocument`, `add_symbol`, `add_symbol_with_context`, `add_file_content`, `search_symbols`, `search_symbols_relaxed`, `search_content`, `search_files`, `apply_symbol_title_boost_to_file_results`)
- Modify: `src/search/query.rs` (delete `build_symbol_query`, `build_symbol_query_weighted`, `build_content_query_weighted`, `build_file_query` if no callers remain)
- Modify: `src/search/reranker.rs` (delete `rerank_symbol_score`, `rerank_content_score` if no callers remain; keep `rerank_score` if still used)
- Modify: `src/tools/search/execution.rs` (delete `execute_definition_search`, `execute_file_search`, `execute_content_search`)
- Modify: `src/tools/search/text_search.rs` (delete `definition_search_with_index`, `content_search_with_index`, `definition_search_with_index_for_test`, `definition_search_with_index_for_ablation`, `RRF_TO_BM25_SCALE`, `apply_reranker_to_symbol_results`, `apply_reranker_to_content_results`)
- Delete: `src/tools/search/target.rs` (`SearchTarget` enum)
- Modify: `src/search/projection/apply.rs` (drop `SymbolDocument`/`FileDocument` parameter types in `apply_documents`, `apply_documents_with_context`, `apply_uncommitted_documents_from_symbols`)
- Modify: `src/search/projection.rs` (same)
- Modify: `xtask/src/search_ablation.rs` (switch imports from `julie::search::{FileDocument, SymbolDocument}` to `SearchDocument`)
- Modify: every test file in `src/tests/tools/search/` and `src/tests/integration/` that imports `SymbolDocument` or `FileDocument` (adapt to `SearchDocument` or delete if the test exercised per-target dispatch exclusively)

**What to build:** Cleanup commit. After this, the codebase has exactly one schema, one doc type, one query path, one reranker.

**Approach:**
1. Delete surfaces above. Use `cargo check` between deletions to catch dangling references.
2. For tests importing `SymbolDocument`/`FileDocument`: prefer adaptation to `SearchDocument`. Delete only tests that exercised per-target dispatch with no behavioral equivalent under the unified path.
3. If `rerank_symbol_score` / `rerank_content_score` still have non-test callers after step 1, defer their deletion to a follow-up commit within this task.

**Acceptance criteria:**
- [ ] `cargo check` clean.
- [ ] Worker-scope: `cargo nextest run --lib tests::tools::search::unified_query_path_test`, `..tests::tools::search::unified_reranker_test`, `..tests::tools::search::projection_search_doc_test`, `..tests::tools::search::relationship_text_test` all pass.
- [ ] (Lead-owned, not worker) `cargo xtask test changed` green after T9 commit.

**Worker tier:** coupled-implementation (many deletions across files; high blast radius). Lead executes or carefully delegates with diff review.

---

### T10 — xtask matrix runner cleanup

**Files:**
- Modify: `xtask/src/search_matrix.rs` (verify the `case.search_target` field is now passive-only — populated from YAML, emitted in reports, not passed to `FastSearchTool`)
- Modify: `xtask/src/search_matrix_report.rs` (no behavior change expected; verify report format stays compatible with existing dashboards)
- Test: `xtask/tests/search_matrix_smoke_test.rs` (verify `cargo xtask search-matrix run --profile smoke` returns expected counts)

**What to build:** Confirm the matrix harness now treats `search_target` as a grouping tag for report output only, not a dispatch parameter.

**Approach:** Most of the wiring happened in T8. T10 is the verification + any residual cleanup.

**Acceptance criteria:**
- [ ] `cargo check` clean.
- [ ] `grep -n "search_target" xtask/src/search_matrix.rs` shows the YAML field on `SearchMatrixCase` and the field on `SearchMatrixBaselineExecution` (report payload), but no usage in the `FastSearchTool` construction.
- [ ] `cargo xtask search-matrix run --profile smoke` returns expected top-1 counts (lead-owned, not worker — but T10 may run it for confidence).
- [ ] Worker-scope tests pass.

**Worker tier:** implementation.

---

### T11 — Docs/skills/agent-instructions sweep

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`
- Modify: `.claude/skills/search-debug/SKILL.md`
- Modify: `.claude/skills/impact-analysis/SKILL.md`
- Modify: `.claude/skills/dead-code-audit/SKILL.md`
- Modify: `docs/SEARCH_FLOW.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/INTELLIGENCE_LAYER.md`

**What to build:** Every reference to `search_target` as a routing concept outside historical investigation/results docs gets rewritten.

**Approach:**
1. Replace `search_target="definitions"`/`="files"`/`="content"` examples with the unified call shape. Wording: "fast_search returns mixed-kind results; each hit carries `kind`."
2. `docs/SEARCH_FLOW.md` § "Route on search_target" rewritten to describe the unified BM25 sweep + single reranker.
3. `docs/INTELLIGENCE_LAYER.md` example call updated.
4. Historical docs preserved unchanged: `docs/investigation/2026-05-21-fts-*`, `docs/plans/2026-05-19-sqlite-fts5-search-ab-replacement-plan.md`, `docs/plans/2026-05-21-fts-ranking-fixes-phase1.md`, `docs/plans/2026-05-21-fts-phase2-unified-schema-design.md`.

**Acceptance criteria:**
- [ ] `grep -rn "search_target" docs/ .claude/skills/ JULIE_AGENT_INSTRUCTIONS.md | grep -v investigation | grep -v 2026-05-19 | grep -v 2026-05-21-fts-ranking-fixes | grep -v 2026-05-21-fts-phase2-unified-schema-design | grep -v 2026-05-22-fts-phase2-unified-schema-implementation` returns no matches.
- [ ] Worker-scope: no test invoked.

**Worker tier:** mechanical.

---

### T12 — Branch gate + Eros bakeoff acceptance

**Files:**
- Append to: this plan § Verification Ledger.

**What to build:** The full acceptance run. Lead-owned.

**Approach:**
1. `cargo xtask test dev` — green.
2. `cargo xtask test dogfood` — green.
3. `cargo xtask search-matrix run --profile smoke` — green.
4. `cargo build --release`. Restart daemon if running.
5. Capture pre-Phase-2 baseline index size: `du -sk ~/.julie/indexes/<workspace_id>/tantivy/` for the workspace used by the bakeoff (or whichever workspace the user is dogfooding against).
6. Trigger Phase 2 reindex by opening the workspace (compat marker mismatch → `RecreatedIncompatible`). Verify daemon log shows `RecreatedIncompatible` for v3 → v4 transition.
7. Capture post-Phase-2 index size; compute delta as a multiplier.
8. Run Eros bakeoff:
   ```
   cd ~/source/eros && uv run eros eval bakeoff --candidate lancedb-fts \
     --corpus ~/.eros-eval/eval/multi-lang-corpus/latest.json \
     --julie-command auto --limit 5 --progress
   ```
9. Compare:
   - julie-cli `top1 ≥ 350` → PASS. If ≥ 370 → PASS-STRETCH.
   - julie-cli `top1 < 350` → FAIL; invoke T13.
   - Index size growth > 2× → FAIL on the soft signal; lead either lowers `relationship_text` cap to 256 bytes and re-runs, or accepts the growth with explicit rationale in the ledger.

**Acceptance criteria:**
- [ ] `cargo xtask test dev` green at Phase 2 HEAD.
- [ ] `cargo xtask test dogfood` green at Phase 2 HEAD.
- [ ] `cargo xtask search-matrix run --profile smoke` green.
- [ ] Daemon log evidence of clean v3 → v4 reindex transition.
- [ ] Eros bakeoff: `julie-cli top1 ≥ 350/406`. Per-category breakdown recorded.
- [ ] Index size growth ≤ 2× baseline (soft signal; documented in ledger).
- [ ] Verification ledger row recorded.

**Worker tier:** lead.

---

### T13 — Contingent: reranker constant tuning sweep (invoked only if T12 fails the 350 gate)

**Files:**
- Modify: `src/search/reranker.rs` (constant tuning)
- Modify: `xtask/src/cli.rs` (add new `Ablation` variants for the tuning grid)
- Append to: this plan § Verification Ledger.

**What to build:** If T12 misses 350, use the search-matrix harness to sweep a small grid over reranker constants. Pick the best configuration and rerun T12.

**Approach:**
1. Define 3–5 candidate variants over `EXACT_TITLE_BOOST`, `PATH_BOOST`, the file-exact `+120`, and per-kind `kind_boost`. Each as an `Ablation::ConstantSet{N}` enum variant in `xtask/src/cli.rs`.
2. Run `cargo xtask search-matrix run --profile smoke --ablation <variant>` for each. Capture top-1 hits.
3. Pick the configuration with the best smoke top1; set the constants in `src/search/reranker.rs`.
4. Rerun T12.
5. If after one tuning pass top1 is still < 350: escalate to strategy-tier replanning. Do NOT silently lower the gate.

**Acceptance criteria (only relevant if invoked):**
- [ ] Tuning sweep ran; matrix-smoke top1 ≥ prior for the chosen configuration.
- [ ] T12 rerun: bakeoff top1 ≥ 350.

**Worker tier:** escalation (gate-interpretation).

---

## Verification Ledger

(Populated by T0, the lead's affected-change runs, and T12.)

| Task | Invariant | Command | Scope | Commit SHA | Result | Hard-gate metrics | Report-only | Timestamp |
|---|---|---|---|---|---|---|---|---|
| T0 | Baseline reproducible on main HEAD `70c7c27f` | `uv run eros eval bakeoff --candidate lancedb-fts --corpus ~/.eros-eval/eval/multi-lang-corpus/latest.json --julie-command auto --limit 5 --progress` | branch | 70c7c27f | _filled_ | total_queries=?, julie_top1=?, eros_lancedb_fts_top1=? | per-category, latency | _filled_ |
| _… per-task rows appended as commits land …_ |
| T12 | Phase 2 closes the gap | `cargo xtask test dev && cargo xtask test dogfood && cargo xtask search-matrix run --profile smoke && (cd ~/source/eros && uv run eros eval bakeoff --candidate lancedb-fts --corpus ~/.eros-eval/eval/multi-lang-corpus/latest.json --julie-command auto --limit 5 --progress)` | branch + bakeoff | _filled_ | _filled_ | julie_top1 ≥ 350 | per-category, latency, index_size_growth_factor | _filled_ |

---

## Risks (from spec, restated)

1. **Eros bakeoff doesn't reach 350.** Mitigation: T13 tuning sweep. If T13 doesn't close it, the design assumption is wrong; escalate.
2. **Reindex cost on existing workspaces.** Documented in release notes. T1's single transition (schema sig + marker bump together) ensures one rebuild, not two.
3. **Tokenizer simplification regresses CamelCase recall.** Mitigation: `pretokenized_code` field. If T12 per-category bakeoff shows regression, search-matrix measures it pre-merge.
4. **`relationship_text` bloats the index.** Mitigation: 512-byte per-symbol cap. If T12 index-size growth > 2×, lower cap to 256 before merge.
5. **Daemon DB `search_target` column remains as historical data.** Documented in T8. New writes pass `""`. Acceptance interpretation: no code references `search_target` as a routing concept.

---

## Skill Pointers

- @razorback:subagent-driven-development — primary execution path for this plan.
- @razorback:codex-cli — used by the lead at plan-review checkpoint (this v2 reflects codex's review) and at post-implementation review.
- @razorback:verification-before-completion — lead consults before marking T12 complete.
- @razorback:test-driven-development — every worker follows TDD.
- @razorback:finishing-a-development-branch — after T12 PASS, decide integration path.
