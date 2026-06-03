# julie-index Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Extract a `julie-index` crate (search + analysis merged) one level above `julie-core`, so editing search/analysis code or their decoupled tests relinks only julie-index's test binary, not the 126k-LOC monolith.

**Architecture:** Same proven Phase 0 playbook — move code down, re-export at old paths via `pub use` shims so path-qualified callers compile unchanged. Sever the single upward edge (`matches_glob_pattern` → `julie_core::glob`), repoint three already-below import families (`database`/`embeddings`-trait/`extractors`), move the embedded `languages/` asset dir into the crate, relocate only the decoupled tests.

**Tech Stack:** Rust, Cargo workspace (resolver 2, edition 2024), tantivy, julie-extractors (git dep), nextest, xtask test runner.

**Architecture Quality:** Approved shape in `docs/plans/2026-06-03-julie-index-extraction-design.md` (Codex-reviewed `461cbd1b`). New `julie-index` crate; new `julie_core::glob`; top-crate `pub use` shims at `src/search/mod.rs`, `src/analysis/mod.rs`, `src/tools/search/{query,mod}.rs`. Caller-facing interface unchanged (zero call-site edits outside in-crate import repoints + the 4 glob call sites). Architecture risk: **low** (pure relocation; smaller back-edge surface than Phase 0). If code reality contradicts this shape, the worker reports a plan mismatch rather than redesigning locally.

**Spec:** `docs/plans/2026-06-03-julie-index-extraction-design.md` — read it before starting any task.

---

## Scope Check

Single subsystem (the search/analysis crate boundary). One plan. Each task leaves the build green; the existing suite is the guard.

**NOT in this plan** (keep the relocation pure):
- julie-extractors 2.1 upgrade — separate task (re-pin git dep + sync `SEMANTIC_INDEX_ENGINE_VERSION`).
- Splitting the 2,249-LOC `src/search/index.rs` god-file — separate opportunistic pass.
- julie-pipeline / julie-tools extraction — later increments.

## File Structure

| Path | Responsibility |
|------|----------------|
| `crates/julie-core/src/glob.rs` (new) | The relocated glob cluster (`matches_glob_pattern` + `compile_patterns` + `CompiledPatterns` + `PatternMatcher` + `split_top_level_commas`). Deps: `globset`, `tracing`. |
| `crates/julie-index/` (new crate) | `search` + `analysis` modules (merged); owns `languages/` asset dir. |
| `crates/julie-index/src/lib.rs` (new) | `pub mod search; pub mod analysis;` + the dep-direction doc invariant. |
| `crates/julie-index/tests/no_upward_deps.rs` (new) | Dep-direction tripwire (mirror julie-core's). |
| `src/search/mod.rs`, `src/analysis/mod.rs` | Become thin `pub use julie_index::{search,analysis}::*;` shims. |
| `src/tools/search/{query,mod}.rs` | `pub use julie_core::glob::matches_glob_pattern;` shim. |
| `Cargo.toml` (workspace) + `crates/julie-index/Cargo.toml` + top `Cargo.toml` | Workspace member + deps wiring. |
| `xtask/test_tiers.toml`, `xtask/src/changed.rs`, `xtask/tests/support/manifest_contract_expected.rs`, `xtask/tests/changed_tests.rs` | Bucket for julie-index + changed-routing + contract mirror, in lockstep. |

---

## Model Routing

**Project source of truth:** repo-root `RAZORBACK.md`.

- **Strategy tier (lead — Opus):** decomposition, inline review, finding triage, the Task 5 gates.
- **Implementation tier (Sonnet workers):** Tasks 1, 3, 4 — bounded, clear acceptance, narrow ownership.
- **Coupled-implementation tier (strongest Sonnet, close lead review):** Task 2 — the cross-file crate move. High blast radius, so the lead reviews hard and the worker reports the MCP evidence + `cargo check` progression.
- **Escalation triggers:** any test that won't decouple (leave it up-stack — don't force it); any non-relocation behavioral diff; a `crate::X` reference in search/analysis that resolves to something not in julie-core/julie-extractors (signals a missed back-edge — stop and report).
- **Mechanical exclusion:** none of these are mechanical — every task owns a compile/test gate.

Team: TeamCreate, Opus lead + Sonnet teammates (per the Phase 0 pattern). No worktrees (collaborative visibility).

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` test tiers + `xtask/test_tiers.toml`.

**Worker red/green scope:** `cargo check -p julie-index` (and `-p julie-core` for Task 1) after each repoint batch; `cargo nextest run -p julie-index <test>` / `-p julie-core <test>` for moved tests; `cargo nextest run --lib <name>` for any top-crate test touched by a shim. Workers run **only** the narrow tests for their task.

**Worker ceiling:** the specific moved/relocated tests + `cargo check`/`cargo nextest run -p <crate>` for the crate they own. Workers do NOT run dev/system tiers.

**Worker gate invariant:** each worker states what its gate proves (e.g. "`cargo nextest run -p julie-index` proves the relocated analysis tests compile + pass inside julie-index's own binary").

**Lead affected-change scope:** `cargo xtask test changed` after each coherent task; `cargo nextest run -p xtask` after any `changed.rs`/`test_tiers.toml`/manifest-mirror edit.

**Branch gate (lead):** `cargo xtask test dev` + `cargo xtask test system` once before handoff. Reuse a passing ledger row only if the scope label + HEAD SHA match exactly.

**Escalation triggers:** see Model Routing.

**Verification ledger:** use `docs/plans/verification-ledger-template.md`; record invariant, command, scope, commit SHA, result, timestamp.

---

## Task 1: Relocate the glob cluster to `julie_core::glob`

**Files:**
- Create: `crates/julie-core/src/glob.rs`
- Modify: `crates/julie-core/src/lib.rs` (add `pub mod glob;`), `crates/julie-core/Cargo.toml` (add `globset`)
- Modify: `src/tools/search/query.rs` (remove the 5 glob symbols; re-export), `src/tools/search/mod.rs` (keep `pub use ...matches_glob_pattern`)

**What to build:** Move the self-contained glob cluster — `matches_glob_pattern`, `compile_patterns`, `CompiledPatterns`, `PatternMatcher`, `split_top_level_commas` (`src/tools/search/query.rs:28–273`) — into a new `julie_core::glob` module so it sits below both `tools` and `search`/`analysis`. Re-export `matches_glob_pattern` at the old path so the ~19 tools-side callers + `src/tests/integration/search_regression_tests.rs` compile unchanged.

**Approach:** `deep_dive`/`get_symbols` the cluster first to confirm boundaries. Cluster deps are only `globset` + `tracing::warn` (julie-core already has tracing). Leave the rest of `query.rs` (line-matching/tokenizing) in tools — it depends on `crate::search::tokenizer` and must NOT move. Do NOT touch search/analysis in this task (they still reach the helper via the existing shim while they're still in the top crate). Add a small julie-core glob unit test (or relocate one glob case from `search_regression_tests.rs`) so the move is covered in julie-core's binary.

**Acceptance criteria:**
- [ ] `julie_core::glob::matches_glob_pattern` is the single definition; `crate::tools::search::matches_glob_pattern` still resolves via shim.
- [ ] `cargo check` green; `cargo nextest run -p julie-core` green; the existing glob tests in `search_regression_tests` still pass (`cargo nextest run --lib test_glob_pattern`).
- [ ] No behavioral change (pure relocation). Committed.

## Task 2: Scaffold `julie-index` and move search + analysis into it

**Files:**
- Create: `crates/julie-index/Cargo.toml`, `crates/julie-index/src/lib.rs`
- Move (`git mv`, preserve subdir depth): `src/search/**` → `crates/julie-index/src/search/**`; `src/analysis/**` → `crates/julie-index/src/analysis/**`; `languages/` → `crates/julie-index/languages/`
- Modify: workspace `Cargo.toml` (add member), top `Cargo.toml` (add `julie-index` dep), `src/search/mod.rs` + `src/analysis/mod.rs` (become `pub use julie_index::{search,analysis}::*;` shims), `src/lib.rs` if it declares the modules

**What to build:** Stand up the merged `julie-index` crate and relocate all of search + analysis into it, with the top crate re-exporting via shims. This is one atomic task — a half-moved crate doesn't compile.

**Approach:** Preserve the `src/search/` and `src/analysis/` subdir layout inside julie-index so the `include_str!("../../languages/*.toml")` paths in `language_config.rs` resolve unchanged after `languages/` moves into the crate (verify with `cargo check -p julie-index` — a wrong path fails to compile). Repoint in-crate imports compiler-driven: `crate::database` → `julie_core::database`; `crate::embeddings::EmbeddingProvider` → `julie_core::embeddings_contract::EmbeddingProvider`; `crate::extractors` → `julie_extractors`; the 2 glob imports (`src/search/index.rs:32`, `src/analysis/early_warnings.rs:9`) + the 1 fully-qualified call (`src/search/index.rs:1246`) → `julie_core::glob`. search↔analysis cross-refs (`crate::search`/`crate::analysis`) now resolve internally to julie-index. Iterate `cargo check -p julie-index` until clean; **if any `crate::X` resolves to something NOT in julie-core/julie-extractors, STOP and report a plan mismatch** (missed back-edge). julie-index Cargo.toml deps: `julie-core`, `julie-extractors` (same pin), `tantivy`, `tracing`, `anyhow`, `serde`/`serde_json`, plus whatever else `cargo check` demands — audit against the old top-crate deps. Tests stay where they are this task (they reach search/analysis through the shims and keep compiling).

**Acceptance criteria:**
- [ ] `julie-index` builds standalone (`cargo check -p julie-index` green) with search + analysis merged; the cycle resolves internally.
- [ ] `languages/` lives in the crate; all `include_str!` language-TOML paths resolve.
- [ ] All top-crate `crate::search::*` / `crate::analysis::*` consumers compile unchanged via shims (`cargo check` green workspace-wide).
- [ ] No upward `crate::{tools,handler,daemon,...}` references remain in julie-index src. Committed.

## Task 3: Relocate the decoupled tests into julie-index's binary

**Files:**
- Move: `src/tests/analysis/**` (all ~10 files) → `crates/julie-index/src/tests/analysis/**` (or `crates/julie-index/tests/`)
- Move: the tool-free/handler-free subset of `src/tests/tools/search/**` (≤35 files, per-file verified) → julie-index
- Modify: `crates/julie-index/Cargo.toml` (dev-dep `julie-core` with `test-support` feature), julie-index `lib.rs`/test module wiring, `src/tests/**/mod.rs` (drop relocated modules)

**What to build:** Move the tests that decouple from both handler and tools into julie-index's own test binary — the relink payoff. Analysis is 100% relocatable (0 handler, 0 tools imports — verified). For search, relocate only files importing neither `JulieServerHandler`/`ManageWorkspaceTool` nor `crate::tools::*`, and verify each has no transitive handler pull via `crate::tests::helpers`/shared fixtures before moving.

**Approach:** Per-file check before each move (`grep`/`get_symbols` the imports). Leave handler/tool-coupled tests (28 of 63 search files) in the top crate — they keep passing via the search/analysis shims. julie-index dev-depends on `julie-core` with the `test-support` feature for db builders (ADR-0006 thin re-export). When in doubt, leave a test up-stack — do not rewrite a coupled test to inflate the count.

**Acceptance criteria:**
- [ ] Relocated analysis + tool-free search tests live in and pass via `cargo nextest run -p julie-index`.
- [ ] Handler/tool-coupled search tests remain in the top crate and still pass.
- [ ] No test dropped or double-counted (relocated count + retained count = original count). Committed.

## Task 4: Dep-direction tripwire + xtask routing for julie-index

**Files:**
- Create: `crates/julie-index/tests/no_upward_deps.rs` (mirror `crates/julie-core/tests/no_upward_deps.rs`)
- Modify: `xtask/test_tiers.toml` (julie-index bucket → `cargo nextest run -p julie-index`), `xtask/src/changed.rs` (route `crates/julie-index/src/**`), `xtask/tests/support/manifest_contract_expected.rs` (mirror — must match `test_tiers.toml` exactly), `xtask/tests/changed_tests.rs` (representative-path assertion)

**What to build:** A tripwire keeping julie-index a clean layer (no `crate::{handler,tools,daemon,indexing_core,watcher,workspace,...}`, no cyclic/upward manifest dep), and xtask wiring so editing julie-index routes to the right bucket. **Apply the Phase 0 lesson:** update the `manifest_contract_expected.rs` mirror and `changed.rs` mapping in lockstep, and add a representative-path test — the exact gap Codex caught in Phase 0 (`16a14272`).

**Approach:** Copy julie-core's tripwire, adjust the crate name + forbidden list. Add a `core-index` (or `julie-index`) bucket. Decide routing: a `crates/julie-index/src/**` edit → the julie-index bucket; if any moved file's behavioral tests stayed up-stack, route to that bucket too (the Phase 0 connection_pool/embeddings/paths pattern). Keep `manifest_contract_expected.rs` byte-identical to `test_tiers.toml` bucket specs.

**Acceptance criteria:**
- [ ] Tripwire passes and FAILs on a planted upward ref (spike-verify like Phase 0).
- [ ] `cargo nextest run -p xtask` green (changed_tests + manifest contract).
- [ ] Editing `crates/julie-index/src/**` routes to the julie-index bucket (representative-path test asserts it). Committed.

## Task 5: Lead — relink-cure proof + branch gates + ledger (Opus, lead-owned)

**Files:** `docs/plans/2026-06-03-julie-index-extraction-results.md` (new); `.memories/` checkpoint.

**What to build:** Prove the relink cure and run the branch gates. NOT a worker task.

**Approach:** Timed touch-and-rebuild: edit a relocated julie-index test → confirm `cargo nextest run -p julie-index --no-run` relinks only julie-index, and `cargo nextest run -p julie --lib --no-run` does NOT recompile the monolith. Record ledger rows. Run `cargo xtask test dev` + `cargo xtask test system`. Live `julie-server` smoke (search / get_symbols / deep_dive) unchanged.

**Acceptance criteria:**
- [ ] Relink-cure ledger row (build/wall, cured vs monolith).
- [ ] dev + system tiers green; xtask green.
- [ ] Live smoke unchanged. Results doc + checkpoint committed.

---

## Execution Notes

- Tasks are largely **sequential** (1 → 2 → 3/4 → 5): a crate extraction is one atomic move, so there's less parallelism than Phase 0. Tasks 3 and 4 can overlap once Task 2 lands (different files: tests vs xtask), but 4's routing should reflect 3's final test locations.
- Green between every task — the existing suite is the guard.
- Use `git mv` for all relocations to preserve history.
- Workers: use Julie's tools (`get_context`/`get_symbols`/`deep_dive`/`fast_refs`) for orientation, not Read/Grep chains; report which MCP calls confirmed what.
