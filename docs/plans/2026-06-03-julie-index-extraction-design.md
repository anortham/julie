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

**Repoint (julie-index side):** 2 `use` imports (`src/search/index.rs:32`,
`src/analysis/early_warnings.rs:9`) and 1 fully-qualified call (`src/search/index.rs:1246`) switch to
`julie_core::glob::matches_glob_pattern`; the 3 short-name call sites (`src/search/index.rs:331,825`,
`src/analysis/early_warnings.rs:512`) then resolve through the fixed imports. The cluster also uses
`tracing::warn` — already a julie-core dependency, so the moved helper's only new third-party need is
`globset`.

## Embedded language-config assets — BUILD-BLOCKING (Codex spec-review finding)

`src/search/language_config.rs` compiles ~36 language TOMLs into the binary via
`include_str!("../../languages/*.toml")` (lines 183–225). `include_str!` resolves **relative to the
source file's directory at compile time**, so moving the file into the new crate silently breaks every
one of those paths — a clean-build failure that simple import-repointing does NOT catch.

**Verified:** `languages/` (repo root) is embedded **only** by `language_config.rs` — the one other
`grep` hit (`get_symbols_token.rs`) is a code comment, not a path. No build script, runtime read, or
other crate consumes the directory.

**Fix (required):** move the `languages/` directory into the new crate (`crates/julie-index/languages/`)
so it is a julie-index-owned asset, and confirm the `include_str!` paths resolve from the file's final
crate-internal location — if the module layout keeps `…/src/search/language_config.rs`, the existing
`../../languages/` paths resolve unchanged; if the layout differs, adjust the relative depth. Gate the
move on `cargo check -p julie-index` (a wrong path fails to compile, so this is self-verifying). A
distribution check is unneeded: the TOMLs are compile-time embedded, not shipped as files.

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

The relink win is proportional to how many tests decouple **from both handler and tools** (parent
design R5). The decoupling metric is "tool-free AND handler-free", not just "handler-free" — a test
that imports any `crate::tools::*` symbol cannot live below tools either (Codex spec-review finding,
which corrected an earlier over-optimistic count):

- **`analysis` tests:** ~10 files, **0** import `crate::tools::*` and **0** instantiate the handler →
  fully relocatable into julie-index's test binary (verified).
- **`search` tests:** 63 files; **28 couple to handler or tools** (18 use `JulieServerHandler`/
  `ManageWorkspaceTool`, and a further 10 import `crate::tools::*` helpers like
  `crate::tools::search::query` / `crate::tools::shared`) → stay in the top crate. The relocatable
  ceiling is therefore **≤35**, not 45 — and each must still be per-file verified for *transitive*
  coupling via `crate::tests::helpers` / shared fixtures before it moves.
- `julie-index` dev-depends on `julie-core` with the `test-support` feature for the db builders the
  relocated tests use (the ADR-0006 thin-re-export pattern), exactly as Phase 0's relocated DB tests do.

**Rule (same as Phase 0):** low-crate tests must not depend on handler/tools. The dep-direction
tripwire is extended to cover julie-index (no `crate::{handler,tools,daemon,...}` in julie-index src).
When in doubt, leave a test up-stack — do not rewrite a tool-coupled test just to inflate the relink count.

## Crate wiring

- New `crates/julie-index/` — `[dependencies]`: `julie-core`, `julie-extractors` (git tag, same pin),
  `tantivy`, `tracing`, `anyhow`, `serde`/`serde_json`, plus whatever search/analysis already use
  (audited during planning). `edition = "2024"`, workspace `resolver = "2"`.
- `crates/julie-core/` — add `pub mod glob;` + `globset` dep.
- `crates/julie-index/languages/` — move the repo-root `languages/` asset dir into the crate (see the
  Embedded-assets section); it is julie-index-owned (only search embeds it).
- Top crate — `src/search/mod.rs` and `src/analysis/mod.rs` become `pub use julie_index::{search,analysis}::*;`
  shims (or thin re-export modules) so `crate::search::*` / `crate::analysis::*` keep resolving.

## Acceptance criteria

- [ ] `julie-index` crate exists; `search` + `analysis` code lives in it; merged-crate cycle resolves.
- [ ] `julie_core::glob::matches_glob_pattern` is the single source; `crate::tools::search::matches_glob_pattern`
      still resolves via shim; the julie-index-side imports/calls repoint to `julie_core::glob`.
- [ ] `languages/` moved into `crates/julie-index/`; all `include_str!` language-TOML paths resolve
      (`cargo check -p julie-index` green — self-verifying).
- [ ] All `crate::search::*` / `crate::analysis::*` consumers in the top crate compile unchanged (shims).
- [ ] Decoupled analysis (100%) + the tool-free/handler-free search subset (≤35, per-file verified)
      relocate into julie-index's test binary; handler/tool-coupled search tests remain up-stack and pass.
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

- **R1 — transitive test coupling.** A tool-free/handler-free search test may still pull the handler
  via `crate::tests::helpers` / shared fixtures. Mitigation: per-file verification before relocating;
  when in doubt, leave it up-stack. The ≤35 ceiling is an upper bound, not a target.
- **R2 — embedded `languages/` assets** (Codex finding, now a first-class step above). The
  `include_str!` paths break on the move unless `languages/` moves into the crate. Self-verifying via
  `cargo check -p julie-index`. **Scan for any other `include_str!`/`include_bytes!` in search/analysis
  during planning** — this one is the only one found, but the move-trap class is real.
- **R3 — search's tantivy/dep surface.** julie-index inherits search's full dependency set; audit
  Cargo.toml during planning so the crate builds standalone. `globset` + `tracing` cover the moved glob
  helper (tracing already in julie-core; globset is a new but already-transitive dep).
