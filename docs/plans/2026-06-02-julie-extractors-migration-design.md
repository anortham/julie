# Design: Migrate julie to consume external julie-extractors 2.0.2

**Date:** 2026-06-02
**Status:** Design — awaiting user review
**Author:** Alan (with Claude)

## Goal

Stop double-maintaining julie's vendored `crates/julie-extractors` (v1.22.0). Make
julie consume the **external** `anortham/julie-extractors` repo (v2.0.2) as the single
source of truth for extraction, so all future language additions and extractor
improvements flow into julie by re-pinning a dependency instead of hand-syncing a fork.

**Chosen integration model:** in-process **library** dependency (git-dep), keeping
julie's own SQLite/Tantivy/daemon/watcher/indexing exactly as they are. Perf is a
**separate follow-up track** (see "Track 2"), not part of this migration.

### Explicitly NOT in scope (and why)
- **The v2.0.x perf wins.** Verified: the rayon parallel-extraction (~2x) lives in
  `julie-extract-cli`, and the SQLite write-path wins live in `julie-extract-artifact`
  — **neither is in the `julie-extractors` library crate julie consumes.** A library
  version bump captures none of them. They are captured later by porting the
  *techniques* into julie's own code (Track 2), not by this migration.
- **The out-of-process artifact model** (spawning `julie-extract`, reading its SQLite
  artifact). That is the long-term north star and a multi-phase rearchitecture; out of
  scope here.

## Why this is low-risk (recon evidence)

A 6-axis read-only recon (2026-06-02) established:

1. **Drop-in API.** Every symbol julie consumes is present in 2.0.2 with an identical
   signature; the defining files (`base/types.rs`, `base/kinds.rs`,
   `base/relationship_resolution.rs`, `language.rs`, `language_spec/`, `manager.rs`,
   `routing_*.rs`) are byte-identical between 1.22.0 and 2.0.2. `EXTRACTION_CONTRACT_VERSION`
   is the **same** string (`2026-05-29.bridge-anchors-v2`). No public struct gained a
   required field, so julie's full struct-literal construction sites (Symbol,
   Relationship, ExtractionResults, AnnotationMarker, StructuredPendingRelationship,
   NormalizedSpan) still compile unchanged.
2. **Mechanism-agnostic consumption.** ~80 `src/` files reach `julie_extractors`, all by
   crate name through two re-export layers (`src/extractors/mod.rs`, `src/language.rs`).
   Swapping `path` → `git` changes **zero** `src/` files.
3. **Zero julie-only logic at risk.** No file exists only in julie's copy; every code
   diff is the external being strictly newer. Migrating even **fixes a latent R
   multibyte-UTF-8 panic** julie still has, and adds JS/TS ECMAScript import resolution
   and `line_starts` O(1) line/column lookup.
4. **The one behavioral change is safe.** Swift 2.0.2 filters known framework/stdlib
   symbols (Codable, SwiftUI, …) so they no longer emit cross-file pending relationships.
   Verified no julie consumer depends on the old behavior: zero Swift relationship/pending
   assertions anywhere in julie's own tests, and no `src/` code references those symbols.
   `real_world_contract.rs` does cover Swift, but it asserts only on **symbol presence**,
   not relationships, so the pending-relationship filtering does not affect it. Pure
   improvement.
5. **Tag exists.** `v2.0.2` is tagged on the remote at `61b225a` (matches the README's
   release commit). The git-dep resolves cleanly. (The local clone is on an unrelated
   branch 15 commits past v2.0.1, which is why a local `git describe` looked stale.)

## Architecture & boundaries

No new julie-internal module boundaries. The change is a **dependency-source shift**
(vendored workspace member → external git-dep) plus a **test-ownership shift** (the
per-extractor golden/capability/certification suites move upstream).

**Drift boundary — required reindex (Codex-caught Critical; resolved via the new tag).**
julie embeds `EXTRACTION_CONTRACT_VERSION` into `SEMANTIC_INDEX_ENGINE_VERSION`
(`src/tools/workspace/indexing/engine_version.rs`). The trap Codex caught: the `v2.0.2`
tag **changed extraction behavior** (Swift stdlib-pending filtering, JS/TS import
resolution) but kept the **same** `2026-05-29.bridge-anchors-v2` contract string, so
pinning v2.0.2 would NOT invalidate existing indexes → stale + new extraction mixed.
**Resolution (decided 2026-06-02, now live):** upstream cut a **new tag `v2.0.3`** (commit
`a9b3839`) that bumps `EXTRACTION_CONTRACT_VERSION` to `2026-06-03.ecmascript-swift-shape-v3`
to cover that behavior change (TODO item #4 in the external repo). julie pins **that** tag, and the
fix becomes clean: julie **syncs its `SEMANTIC_INDEX_ENGINE_VERSION` literal to the new
constant**, which the `engine_version.rs` test enforces as a real RED→GREEN, and which
changes the engine version → forces the one-time reindex. (The earlier fallback — appending
a synthetic `+extractor-dep=<tag>` segment to self-protect against a stale-contract tag —
is no longer needed because the contract string itself now changes. Keep it in mind only
if a future re-pin lands a tag whose contract constant did not move.)

## Track 1 — the migration (this change)

### 1. Dependency swap (`Cargo.toml`)
- `members = [".", "crates/julie-extractors", "xtask"]` → `members = [".", "xtask"]`.
- `julie-extractors = { path = "crates/julie-extractors" }` →
  `julie-extractors = { git = "https://github.com/anortham/julie-extractors", tag = "v2.0.3" }`
  — the contract-bump release (live at commit `a9b3839`), **not** v2.0.2.
  - Use the **real** repo URL `anortham/julie-extractors` (the crate's `repository`
    metadata says `murphy/...`; that field is cosmetic and irrelevant to resolution —
    optional upstream cleanup).
  - Dep key `julie-extractors` matches the package name in the external workspace, so no
    `package = ` override is needed.
- Regenerate `Cargo.lock` (`cargo build`), commit it. The 4 git-sourced tree-sitter
  sub-deps (qmljs, razor, powershell, vb-dotnet) re-resolve under the git dep.
- **`[lints] workspace = true` caveat is moot** for a git-dep: the git checkout includes
  the external workspace root, which defines `[workspace.lints]`. (It would only bite if
  we vendored the crate dir into julie's workspace, which we are not doing.)
- No version conflicts: julie's main crate directly deps only `tree-sitter = "0.26.8"`
  (same version) and already has `rayon = "1.10"`.

### 2. Delete the in-tree crate
- Remove `crates/julie-extractors/` entirely (src, tests, examples, README, Cargo.toml).
- If `crates/` is left empty, remove it.

### 3. `src/` source changes: exactly one, deliberate
- **Re-exports + callers unchanged.** `src/extractors/mod.rs`
  (`pub use julie_extractors::*` + named re-exports), `src/language.rs`
  (`pub use julie_extractors::language::*`), and all direct `julie_extractors::` callers
  reference the crate **by name** → unchanged by the path→git swap.
- **One deliberate edit:** sync the `SEMANTIC_INDEX_ENGINE_VERSION` literal in
  `src/tools/workspace/indexing/engine_version.rs` to the new tag's
  `EXTRACTION_CONTRACT_VERSION` (`extractors=2026-06-03.ecmascript-swift-shape-v3+schema=…`).
  After pinning the new tag, `src/tests/core/engine_version.rs` goes RED (the old literal
  no longer *contains* the new constant); the sync turns it GREEN and changes the engine
  version → forces the reindex.

### 4. Adopt the Swift behavior change (no code change, document only)
- The new stdlib-filtering is adopted automatically with the dep. No julie consumer
  depends on the old behavior (verified). Note it in the migration commit message and in
  release notes so the behavior shift (fewer Swift cross-file pending relationships) is
  on the record.

### 5. xtask: remove the now-upstream extractor gates, add a thin dep-integration gate
- Delete buckets `extractors`, `extractor-units`, `parser-upgrade` from
  `xtask/test_tiers.toml` and from the dev/full tier lists. These run
  `cargo nextest run -p julie-extractors ...`, which only works on a local workspace
  member; once it's a git-dep they cannot run from julie, and they all already exist in
  the external repo's CI.
- **Remove the FULL `certify tree-sitter` command surface (Codex-caught: deletion was
  under-scoped).** It is wired beyond the impl modules — deleting only the modules leaves
  xtask not compiling. Remove all of: the impl modules
  (`xtask/src/tree_sitter_certification.rs`, `_certification_data.rs`,
  `_certification_report.rs`, `tree_sitter_real_world.rs`, `_real_world_report.rs`,
  `xtask/src/tree_sitter_real_world/`); the CLI parsing (`xtask/src/cli.rs:180`,
  `cli.rs:398`); the main dispatch (`xtask/src/main.rs:171`); the lib exports
  (`xtask/src/lib.rs:13`); and the command's tests (`xtask/tests/tree_sitter_certification_tests.rs`).
- **Add a julie-owned `extractor-dep-integration` bucket (Codex-caught: re-pin gate would
  otherwise be deleted).** Today `Cargo.toml`/`Cargo.lock`/the in-tree crate manifest
  route to `parser-upgrade` via `changed.rs` (~line 396, ~line 1020), so a future
  extractor-tag bump gets extractor-level verification. After this migration that routing
  target is gone. Replace it: a new bucket running `engine_version` + `real_world_contract`
  + the product extraction smoke, and re-route `Cargo.toml` / `Cargo.lock` changes to it
  in `changed.rs`. This is the thin "does the dependency I consume still produce what I
  expect?" gate that fires on every re-pin.
- Clean the rest of `xtask/src/changed.rs` diff-routing: remove `is_extractor_path` /
  `is_parser_upgrade_path` branches and the `crates/julie-extractors/` path mappings
  (and matching `changed_tests.rs` fixtures + the xtask manifest-contract test
  expectations) so the manifest-contract self-test stays green.

### 6. Fixtures & docs
- **Delete** `fixtures/extraction/**` (golden source/expected pairs, `capabilities.json`,
  `tree-sitter-real-world-corpus.toml`) — now upstream.
- **Keep the real-world fixtures — and note the keep-set is broader than
  `fixtures/real-world/**` (Codex-caught).** `real_world_contract.rs` exercises ~29
  languages, but two of its paths live **outside** `fixtures/real-world/`:
  `fixtures/qml/real-world/cool-retro-term-main.qml` and
  `fixtures/r/real-world/ggplot2-geom-point.R`. The authoritative keep-set is the **union
  of every `fixtures/...` path literal referenced anywhere in `src/tests/`** (derive it by
  grep at implementation time), not just the contract table — confirmed roots include
  `fixtures/real-world/**`, `fixtures/qml/real-world/**`, `fixtures/r/real-world/**`,
  plus `fixtures/real-world/{router.rs,sample.rs}`. Only delete files in that grep's
  complement.
- **Watch skip-when-empty tests.** Some real-world validation tests (e.g.
  `src/tests/integration/real_world_validation/real_world_refactoring_tests.rs`) **skip**
  when their fixture dir is empty. Deleting their fixtures would turn them into silent
  no-ops (fake green), which violates "a test that can't fail isn't a test." For each:
  either keep its fixtures or delete the test outright — never leave it hollow.
- **Expect possible expected-count updates** in `real_world_contract.rs`. It asserts on
  symbol presence/counts; 2.0.2's extractors (notably the JS/TS rewrite) may shift some
  counts. If so, update the expected values to the new canonical extraction — that is an
  accepted outcome of adopting the upstream extractors, not a regression.
- **Delete** `docs/LANGUAGE_CERTIFICATION_REPORT.md`,
  `docs/LANGUAGE_REAL_WORLD_EVIDENCE.{json,md}`, `docs/ADDING_NEW_LANGUAGES.md`.
- **Redirect** docs that tell contributors to add languages/parsers in-tree
  (`docs/TREE_SITTER_UPGRADES.md`, `docs/DEVELOPMENT.md`,
  `docs/TREE_SITTER_REVIEW_FINDINGS_STATUS.md`, `CLAUDE.md` + `AGENTS.md`, any
  extractor-audit docs): the new workflow is "add the language in
  `anortham/julie-extractors`, release, re-pin julie's git-dep." Keep `CLAUDE.md` and
  `AGENTS.md` in sync (the pre-commit hook enforces this).

### 7. Self-navigation into extractor internals is lost (Codex-caught: accept + update)
After deleting the in-tree crate, julie's **own workspace index no longer contains the
extractor source** (a git-dep lives under `~/.cargo/git`, outside the indexed workspace).
So `deep_dive` / `call_path` into extractor internals from julie's self-index will no
longer resolve — e.g. the dogfood expectation that `call_path extract_symbols_static →
extract_canonical` lands in `crates/julie-extractors/src/pipeline.rs`
(`src/tools/workspace/indexing/extractor.rs:24`, `docs/TREE_SITTER_QUALITY_BAR.md:212`).
Decision: **accept the loss** — you don't index a dependency's source, and a dev who needs
to navigate extractor internals can `manage_workspace(operation="open")` the external repo.
Actions: (a) update the stale docs/evidence that assert in-tree extractor navigation;
(b) at implementation time, grep `src/tests/**` for any assertion on a
`crates/julie-extractors/...` path — if a test asserts it, that test must be updated or
removed (a doc reference is fine to edit; a failing test is a build break).

### Tests julie KEEPS (re-targeted only if needed)
- `src/tests/core/engine_version.rs` — the contract-version anchor. After the engine
  literal is synced to the new tag's `EXTRACTION_CONTRACT_VERSION`, it asserts the version
  still contains that constant (the RED→GREEN that proves the reindex).
- `src/tests/integration/real_world_contract.rs` — julie's own extraction-pipeline smoke
  (now sources symbols from the git-dep; keep its full fixture set per §6, expect possible
  expected-count updates). Joins the new `extractor-dep-integration` bucket.
- All `src/tests/tools/**` — MCP/search/indexing/navigation integration tests. They use
  extractor output but assert on julie's product; none live upstream.

## Track 2 — perf follow-up (separate initiative, designed later)

Captured here so it is concrete, not vague. Two independent sub-tracks; julie already
has `rayon = "1.10"`, so no new dependency:

1. **Parallelize extraction in julie's own indexer.** Wrap the per-file
   read→`extract_canonical`→map phase in `src/indexing_core` (the walker / batch path)
   in a `rayon` par-iter over file chunks, mirroring `julie-extract-cli`'s 512-file
   chunked `par_iter` + serial ordered drain, with a per-file `catch_unwind` boundary so
   one malformed file can't abort the batch.
2. **Port the SQLite write-path techniques into `src/database/bulk`.** Apply
   `defer_foreign_keys` during bulk writes, fold symbol-parent and identifier FK
   resolution into the INSERT (eliminate second-pass UPDATEs), and hoist hot INSERTs into
   batch-scoped `prepare_cached` statements. These are general rusqlite techniques, not
   artifact-specific.

Each sub-track gets its own design + plan + benchmark gate when we pick it up.

## Verification plan (Track 1)

| Step | Command | Gate |
|------|---------|------|
| Compiles | `cargo check` then `cargo build` | clean |
| Contract anchor (bumped) | `cargo nextest run --lib engine_version` | green with new literal |
| New dep-integration bucket | `cargo xtask test bucket extractor-dep-integration` | green |
| Julie's own extraction smoke | `cargo nextest run --lib real_world_contract` | green (update counts if shifted) |
| xtask compiles after certify removal | `cargo check -p xtask` | clean |
| Batch regression | `cargo xtask test dev` (after bucket changes) | green |
| Search/index regression | `cargo xtask test dogfood` | green |
| xtask manifest contract | the xtask self-test that validates `test_tiers.toml` | green |
| Reindex actually fires | rebuild, connect a session against a pre-migration index | engine-version mismatch triggers a full reindex (not a no-op) |
| End-to-end dogfood | reindex julie itself, run `fast_search` / `get_symbols` / `deep_dive` | symbols/relationships still extracted across languages |

The reindex-fires check is the proof that the Critical drift finding is actually fixed.
The end-to-end dogfood is the proof the git-dep produces usable extraction in julie's
live pipeline, not just that it compiles.

## Acceptance criteria

- [ ] `julie-extractors` is a git-dep pinned to the contract-bump tag `v2.0.3` (commit
      `a9b3839`, **not** v2.0.2); `cargo build` is clean and `Cargo.lock` is committed.
- [ ] `crates/julie-extractors/` is deleted and removed from `[workspace] members`.
- [ ] Only the deliberate `engine_version.rs` literal sync changes in julie `src/`
      consumption code; all re-exports + callers compile unchanged.
- [ ] **Reindex forced:** `SEMANTIC_INDEX_ENGINE_VERSION` synced to the new tag's
      `EXTRACTION_CONTRACT_VERSION`; `engine_version.rs` test went RED→GREEN; a pre-migration
      index actually triggers a full reindex on connect (verified, not assumed).
- [ ] Buckets `extractors`, `extractor-units`, `parser-upgrade` removed; **full**
      `certify tree-sitter` command surface removed (impl + cli.rs + main.rs + lib.rs +
      its tests) and `cargo check -p xtask` is clean; `changed.rs` routing cleaned;
      xtask manifest-contract self-test green.
- [ ] **New `extractor-dep-integration` bucket** added (engine_version + real_world_contract
      + extraction smoke) and `Cargo.toml`/`Cargo.lock` re-route to it in `changed.rs`.
- [ ] `fixtures/extraction/**` deleted; fixture keep-set derived from a grep of all
      `src/tests/` fixture paths (includes `fixtures/{real-world,qml/real-world,r/real-world}/**`);
      no skip-when-empty test left hollow; cert docs deleted; contributor docs
      (CLAUDE.md/AGENTS.md/DEVELOPMENT.md/TREE_SITTER_UPGRADES.md) + stale in-tree-navigation
      evidence (TREE_SITTER_QUALITY_BAR.md) redirected.
- [ ] No `src/tests/**` asserts a `crates/julie-extractors/...` path (self-navigation loss
      accepted; any such test updated/removed).
- [ ] `cargo xtask test dev` + `cargo xtask test dogfood` green.
- [ ] End-to-end dogfood: julie reindexes itself and extraction still works across
      languages (spot-check Rust/TS/Python/Swift).
- [ ] Swift behavior change noted in the commit message / release notes.

## Risks & rollback

- **Risk (was Critical, now designed-for): stale indexes don't invalidate.** Resolved by
  pinning the new tag whose `EXTRACTION_CONTRACT_VERSION` changed, syncing julie's engine
  literal to it, and the "reindex actually fires" verification step. Do not skip that step
  — it is the proof. (If a future re-pin lands a tag whose contract constant did NOT move
  despite a behavior change, fall back to a synthetic `+extractor-dep=<tag>` segment.)
- **Risk: fixture deletion turns a skip-when-empty test into a silent no-op.** Mitigation:
  derive the keep-set from a grep of all `src/tests/` fixture paths; for skip-when-empty
  tests, keep fixtures or delete the test — never leave it hollow. (Codex confirmed `src/`
  and `src/tests/**` do **not** reference `fixtures/extraction/**`, so the golden-corpus
  deletion itself is safe; the risk is only the broader real-world keep-set.)
- **Risk: incomplete `certify` removal leaves xtask not compiling.** Mitigation: remove the
  full command surface (cli/main/lib/tests, not just impl modules); `cargo check -p xtask`
  is a gate.
- **Risk: removing an xtask bucket breaks the tier/manifest self-test.** Mitigation: the
  xtask manifest-contract test is in the verification gate; run it explicitly.
- **Risk: git-dep fetch in CI/release.** Low — CI already clones git for 4 tree-sitter
  parsers; release.yml builds binaries only and never invoked `-p julie-extractors`.
- **Rollback:** revert the Cargo.toml/Cargo.lock + engine_version change and restore
  `crates/julie-extractors/` from git history. The whole migration is one reviewable
  change; reverting the commit restores the vendored crate and old engine version exactly
  (a re-revert reindex is the only side effect).

## Independent review

Codex (`gpt-5.5`, xhigh, adversarial design review, 2026-06-02) **confirmed** the two
load-bearing claims against commit `61b225a`: no compile-breaking API mismatch, and the
perf wins genuinely live in `julie-extract-cli` (rayon) and `julie-extract-artifact`
(SQLite write path), not the library crate. Its five findings (1 Critical reindex drift,
2 High, 2 Medium) are all incorporated above. Verdict: needs-attention → addressed.

## Upstream prerequisites

- **✅ DONE (was user-owned/blocking):** the `EXTRACTION_CONTRACT_VERSION` bump to
  `2026-06-03.ecmascript-swift-shape-v3` is committed and tagged as **`v2.0.3`** (commit
  `a9b3839`), verified via `git show v2.0.3:crates/julie-extractors/src/lib.rs`. julie pins
  that tag, not `v2.0.2` (whose tag `61b225a` still carries the old contract string). (This
  was upstream TODO item #4.)
- Optional upstream cleanup: fix the crate's `repository` metadata (`murphy/...` →
  `anortham/...`). Not required for this migration.
