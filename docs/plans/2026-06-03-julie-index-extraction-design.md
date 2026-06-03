# Julie Rescue Phase 1 — `julie-index` Extraction (search + analysis)

**Date:** 2026-06-03
**Status:** Design — pending user sign-off
**Branch:** `julie-rescue` (continue on it; Phase 0 already merged to `main` at `e3735abe`)
**Parent design:** `docs/plans/2026-06-03-julie-rescue-design.md` (§3.1 DAG, §4 Phase 1)
**Predecessor:** Phase 0 (`julie-core` leaf) — merged, relink cure proven ~5.8×, ADR-0006.

---

## Goal

Extract the **second crate down** in the rescue DAG: `julie-index` = `search/` + `analysis/`
**merged into one crate**, sitting directly above `julie-core`. Editing search/analysis code or
their decoupled tests then relinks only `julie-index`'s test binary, not the 126k-LOC monolith.
Same proven playbook as Phase 0: move code down, re-export at old paths via `pub use` shims so
path-qualified callers compile unchanged.

## Why search + analysis merge (not two crates)

They form a cycle: `src/search/` references `crate::analysis` 4× and `src/analysis/` references
`crate::search` 4× (they cross-reference language-config types — `LanguageConfigs`,
`TestRoleConfig`, etc.). Two crates can't have a dependency cycle, so they become **one crate**,
`julie-index`. The cycle becomes ordinary intra-crate module references.

## Measured dependency footprint (current HEAD, post-Phase-0)

`search/` (7,741 LOC) + `analysis/` (2,667 LOC) reference only these `crate::` roots:

| Root | Refs (search/analysis) | Disposition |
|------|------------------------|-------------|
| `crate::search`, `crate::analysis` | 36 / 6 | **Internal** after merge — no change beyond `crate::` path resolution within julie-index |
| `crate::database` | 9 / 4 | **Down** → repoint to `julie_core::database` (moved in Phase 0) |
| `crate::extractors` | 9 / 4 | **Down** → repoint to `julie_extractors::` (`src/extractors/mod.rs` is a 20-line pure re-export shim of `julie_extractors`) |
| `crate::embeddings::EmbeddingProvider` | 1 / 0 | **Down** → repoint to `julie_core::embeddings_contract::EmbeddingProvider` (trait moved in Phase 0) |
| `crate::tools::search::matches_glob_pattern` | 2 / 1 | **UP — the only real back-edge.** Sever by moving the glob helper down to julie-core (below). |

There are **no** references to `utils`, `language`, `config`, `errors`, `models`, `types`,
`handler`, `indexing_core`, `watcher`, `workspace`, or `daemon`. This is a near-leaf slice — the
only upward edge is one glob helper.

## The one severing job: `matches_glob_pattern` → julie-core

`matches_glob_pattern` lives at `src/tools/search/query.rs:28` and is used by **both** `tools`
(above julie-index) and `search`/`analysis` (julie-index). To satisfy the DAG it must move **below
both**, into `julie-core`.

It is **not** a whole-file move (unlike Phase 0's `connection_pool.rs`). `query.rs` mixes the glob
cluster with unrelated line-matching/tokenizing code that depends on `crate::search::tokenizer`.
The glob cluster is self-contained — it depends only on the external `globset` crate:

- `matches_glob_pattern` (pub fn) → `compile_patterns` → `CompiledPatterns` → `PatternMatcher`,
  plus `split_top_level_commas`. (≈90 lines, 5 symbols, `src/tools/search/query.rs:28–273`.)

**Move:** extract those 5 symbols into a new `crates/julie-core/src/glob.rs`
(`pub mod glob;` in julie-core lib.rs); add the `globset` dependency to julie-core.

**Shim:** `src/tools/search/query.rs` and `src/tools/search/mod.rs` re-export
`pub use julie_core::glob::matches_glob_pattern;` so the ~19 tools-side callers and
`src/tests/integration/search_regression_tests.rs` keep resolving `crate::tools::search::matches_glob_pattern`.

**Repoint:** the 3 julie-index internal call sites (`src/search/index.rs`,
`src/analysis/early_warnings.rs`) switch to `julie_core::glob::matches_glob_pattern`.

## Architecture Quality (Gate Mode)

- **Affected modules:** new `julie-index` crate (search+analysis); new `julie_core::glob`; top-crate
  shims at `src/search/mod.rs`, `src/analysis/mod.rs`, `src/tools/search/{query,mod}.rs`.
- **Caller-facing interface:** unchanged. Every existing `crate::search::*`, `crate::analysis::*`,
  and `crate::tools::search::matches_glob_pattern` path keeps resolving via `pub use` shims — zero
  call-site edits outside the 3 internal glob repoints and the in-crate import repoints.
- **Depth/locality check:** the change is a relocation; behavior is identical. New seam = one crate
  boundary that already exists conceptually in the DAG.
- **Test surface:** decoupled search/analysis tests relocate into julie-index's binary and run through
  the same public functions they use today; handler-coupled tests stay up-stack unchanged.
- **Seams/adapters:** no new adapter. `julie_core::glob` is a pure relocation, not a new abstraction.
- **Rejected shortcuts:** (a) splitting `search/index.rs` during the move — rejected, deferred to a
  separate opportunistic pass to keep this a pure relocation; (b) moving the whole `query.rs` to
  julie-core — rejected, it carries unrelated tools-layer code; only the glob cluster moves.
- **Architecture risk:** low (mirrors the proven Phase 0 mechanism; smaller back-edge surface).

## Test decoupling (the relink payoff)

The relink win is proportional to how many tests decouple from the handler (parent design R5):

- **`analysis` tests:** ~10 files, **0** instantiate `JulieServerHandler`/`ManageWorkspaceTool` →
  fully relocatable into julie-index's test binary.
- **`search` tests:** 63 files; **45 show no direct handler use** → relocate (per-file verified during
  the move — confirm none pull the handler transitively via `crate::tests::helpers` before moving);
  **18 instantiate the handler** → stay in the top crate as integration tests (run at batch boundaries).
- `julie-index` dev-depends on `julie-core` with the `test-support` feature for the db builders the
  relocated tests use (the ADR-0006 thin-re-export pattern), exactly as Phase 0's relocated DB tests do.

**Rule (same as Phase 0):** low-crate tests must not depend on handler/tools. The dep-direction
tripwire is extended to cover julie-index (no `crate::{handler,tools,daemon,...}` in julie-index src).

## Crate wiring

- New `crates/julie-index/` — `[dependencies]`: `julie-core`, `julie-extractors` (git tag, same pin),
  `tantivy`, `tracing`, `anyhow`, `serde`/`serde_json`, plus whatever search/analysis already use
  (audited during planning). `edition = "2024"`, workspace `resolver = "2"`.
- `crates/julie-core/` — add `pub mod glob;` + `globset` dep.
- Top crate — `src/search/mod.rs` and `src/analysis/mod.rs` become `pub use julie_index::{search,analysis}::*;`
  shims (or thin re-export modules) so `crate::search::*` / `crate::analysis::*` keep resolving.

## Acceptance criteria

- [ ] `julie-index` crate exists; `search` + `analysis` code lives in it; merged-crate cycle resolves.
- [ ] `julie_core::glob::matches_glob_pattern` is the single source; `crate::tools::search::matches_glob_pattern`
      still resolves via shim; the 3 julie-index internal call sites repoint to `julie_core::glob`.
- [ ] All `crate::search::*` / `crate::analysis::*` consumers in the top crate compile unchanged (shims).
- [ ] Decoupled analysis (100%) + handler-free search tests relocate into julie-index's test binary;
      handler-coupled search tests remain up-stack and pass.
- [ ] Dep-direction tripwire extended to julie-index (fails on any upward `crate::` ref or cyclic manifest dep).
- [ ] xtask buckets repointed: editing `crates/julie-index/src/**` routes to the right bucket(s), with
      the `manifest_contract_expected.rs` mirror + `changed.rs` mapping updated in lockstep (the exact
      issue Codex caught in Phase 0 — do it right the first time here).
- [ ] Relink cure measured: editing a relocated julie-index test relinks only julie-index's binary,
      not the monolith (ledger row).
- [ ] Full dev + system tiers green; live `julie-server` smoke (search / get_symbols / deep_dive) unchanged.

## Verification Strategy

- **Project source of truth:** `CLAUDE.md` test tiers + `xtask/test_tiers.toml`.
- **Worker red/green:** `cargo nextest run -p julie-index <test>` (and `-p julie-core` for the glob move);
  `cargo check` after each import repoint batch.
- **Worker ceiling:** the specific relocated/moved tests only. Workers do not run dev/system tiers.
- **Lead affected-change:** `cargo xtask test changed` after each coherent batch.
- **Branch gate:** `cargo xtask test dev` + `cargo xtask test system` once before handoff; `-p xtask`
  green after any changed.rs/test_tiers.toml edit.
- **Escalation triggers:** any test that won't decouple from the handler (signals the boundary isn't
  clean — leave it up-stack, don't force it), any non-relocation behavioral diff.

## Out of scope (explicitly deferred)

- Splitting the 2,249-LOC `src/search/index.rs` god-file — separate opportunistic pass, not this slice.
- `julie-pipeline` / `julie-tools` / `julie-runtime` extraction — later Phase 1/2 increments.
- ToolContext facade, daemon teardown, tool taxonomy — Phases 2–4.

## Risks

- **R1 — transitive test coupling.** A "handler-free" search test may pull the handler via shared
  helpers. Mitigation: per-file verification before relocating; when in doubt, leave it up-stack.
- **R2 — `globset` already in julie-core's tree?** Confirm during planning; if absent, add it (it's
  already a transitive dep via tools today, so no new third-party surface).
- **R3 — search's tantivy/dep surface.** julie-index inherits search's full dependency set; audit
  Cargo.toml during planning so the crate builds standalone.
