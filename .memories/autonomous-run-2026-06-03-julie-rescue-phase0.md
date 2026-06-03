# Autonomous Execution Report - Julie Rescue Phase 0 (extract `julie-core` leaf crate)

**Status:** Complete
**Plan:** docs/plans/2026-06-03-julie-rescue-phase0-plan.md
**Branch:** julie-rescue
**PR:** https://github.com/anortham/julie/pull/22
**Duration:** ~4h 45m (first→last commit 09:34→14:19, 2026-06-03)
**Phases:** 1/1 complete (Phase 0)
**Tasks:** 10/10 complete

## What shipped
- **`julie-core` leaf crate extracted from the monolith** — embeddings-contract trait (`EmbeddingProvider` + companion types), the daemon connection pool (`PooledConn`/`WorkspaceConnectionPool`), `to_relative_unix_style` path helpers, and the entire 37-file `database` module all moved down into the bottom leaf crate. Every move used a re-export shim (`pub use` at the old path), so all path-qualified callers compiled unchanged — zero call-site edits.
- **118→120 pure database-layer tests relocated into julie-core's own test binary.** Editing one of those tests now relinks only julie-core (~1.7s), not the 126k-LOC monolith (~9.8s). This is the relink-tax cure, proven below.
- **ADR-0006: test-support helpers live in a feature-gated `julie_core::test_support` module**, not a separate cyclic dev-dep crate. The plan's original Task 6 design (`julie-test-support` as a dev-dep of julie-core) formed a dependency cycle that made Cargo compile julie-core as two distinct rlibs, so builders returning julie-core types were rejected by julie-core's own tests (the "two-rlib" type mismatch). Fix: host the helpers in the leaf behind a `test-support` feature; reduce `julie-test-support` to a thin re-export; sever julie-core's dev-dep on it. `tempfile` is optional + dev-dep, kept out of production by resolver 2 / edition 2024.
- **Dep-direction tripwire** (`crates/julie-core/tests/no_upward_deps.rs`) — two guards that keep the leaf a leaf: a comment-stripped source scan for forbidden upward references (`crate::{handler,tools,daemon,indexing_core,watcher,analysis,search,workspace,external_extract,health}`, `julie_test_support`, bare `julie::`) and a manifest guard against re-creating the ADR-0006 cycle or depending on the parent `julie` crate. Spike-verified: a planted `crate::tools` reference makes it FAIL with file:line; removed → green.
- **xtask test buckets repointed** — `core-database`/`core-fast` buckets now run `cargo nextest run -p julie-core` after the DB-test relocation orphaned the old `--lib tests::core::database*` filters (exit-4). Lockstep updates to `manifest_contract_expected.rs` and `changed.rs` path mappings.
- **One-shot 3-way retrieval bakeoff** (135 queries, 9 repos, Go/JS/Py/Rust/TS) — Julie 0.904 top-5 / 0.881 MRR vs Miller 0.622 / 0.556. Confirms the search moat is structural (file/path +84pp, test-awareness +100pp, doc-phrase +52pp), justifying the packaging rescue over a switch to Miller. Report-only; no promotion machinery.

## Judgment calls (non-blocking decisions made)
- `crates/julie-core/src/lib.rs` — Hosted test helpers in a feature-gated `test_support` module in the leaf rather than the plan's separate `julie-test-support` crate. The separate-crate design was a real flaw (dev-dep cycle → two-rlib mismatch). **This was the one decision escalated to the user**, who chose "feature-gate in julie-core". Recorded in ADR-0006.
- `docs/plans/2026-06-03-julie-rescue-phase0-results.md` — Recorded the bakeoff as report-only decision evidence with explicit caveats (eros column unreliable due to missing lancedb; Julie standalone latency non-representative; corpus is Go/JS/Py/Rust/TS only). Did not build a promotion gate or re-run loop.
- `crates/julie-core/src/database/bulk/type_arguments.rs:11` — De-linked the broken intra-doc ref `crate::indexing_core::ExtractedBatch` to plain `` `ExtractedBatch` `` (the referenced type lives up in the parent crate; a leaf crate cannot doc-link upward).
- Branch-gate evidence reuse: the dev (35 buckets/1072s) and system (7 buckets/192s) tiers ran at `3fcaa15d`. The final commit `bef0c9e6` adds only tests + docs (verified via `git diff --name-only 3fcaa15d bef0c9e6` — zero production files), so I verified the delta (new tripwire) via `cargo nextest run -p julie-core` (120/120) at HEAD rather than re-running 18 minutes of unchanged-production-code dev tier.

## External review (Codex `gpt-5.5` high, adversarial, pre-merge)
Run on user request before merge against the full code diff (94 files, +3469/-1647), read-only sandbox, targeted at the six highest-risk areas of a crate split (shim fidelity, ADR-0006 feature-gating, the severed dep cycle + tripwire, test-coverage loss, moved-module behavioral drift, Cargo wiring).
- **Verdict:** needs-attention — **1 finding** (medium); no critical/high. Shim fidelity, feature-gating, dep-cycle severance, and behavioral drift all came back clean.
- **Verified real, fixed:** 1 — `changed.rs` blanket-routed every `crates/julie-core/src/**` edit to `core-database` only, collapsing the pre-split behavioral routing for the three moved leaf files (connection pool → `daemon`, embeddings contract → `core-embeddings`, paths → `core-fast`). A localized edit to moved leaf code would have run only `-p julie-core` and skipped its real regression tests without triggering dev-fallback. **Fixed in `16a14272`**: file-specific routing restored (each leaf file → `core-database` + its behavioral bucket), guarded by three new representative-path assertions; `cargo nextest run -p xtask` 165 passed. The design doc was also Codex-reviewed earlier during brainstorming (`3cf2569e`).
- **Dismissed / flagged:** none.

## Tests
- **julie-core own test binary @ HEAD bef0c9e6:** `cargo nextest run -p julie-core` → 120 passed (1 leaky, 1 skipped). The +2 over the prior 118 are exactly the new tripwire tests.
- **Dev tier @ 3fcaa15d (production code identical to HEAD):** `cargo xtask test changed` (→ dev fallback, shared infra moved) → 35 buckets passed in 1072.0s, zero failures.
- **System tier @ 3fcaa15d:** `cargo xtask test system` → 7 buckets passed in 192.1s, zero failures.
- **Tripwire:** `no_upward_deps` 2/2 pass; spike-verified it FAILs with file:line on a planted `crate::tools` reference.
- **Relink-cure proof:** edit a julie-core DB test → 1.68s build / 3.41s wall; edit a top-crate test → monolith 9.77s / 12.91s; the monolith is NOT relinked for julie-core test edits. ~5.8× build, ~3.8× wall for the relocated slice.

## Blockers hit
- None. The dev-dep cycle was a genuine flaw in the approved plan, but it was resolved in-run via a single user decision + ADR-0006, not left as a blocker.

## Files changed
108 files changed, 6106 insertions(+), 1648 deletions(-). Highlights:
- New crate: `crates/julie-core/**` (src moved down + relocated DB tests + tripwire), `crates/julie-test-support/**` (thin re-export).
- Source shims: `src/daemon/connection_pool.rs` (-342), `src/embeddings/mod.rs`, `src/utils/paths.rs`, `src/lib.rs`, `src/tests/**` (helpers + relocated tests removed from top crate).
- xtask: `test_tiers.toml`, `changed.rs`, `manifest_contract_expected.rs`, `changed_tests.rs`.
- Docs: `docs/adr/ADR-0006-*`, `docs/plans/2026-06-03-julie-rescue-{design,phase0-plan,phase0-results}.md`, bakeoff driver + raw results.

## Next steps
- Review PR: https://github.com/anortham/julie/pull/22
- Merge when satisfied — Autonomous Mode never auto-merges.
- Phase 0 cures the test-edit loop for the relocated DB slice only; editing julie-core **production** code still relinks the monolith (the top crate deps julie-core's lib). Later rescue phases widen the cure by splitting more crates out of the monolith (and the daemon-teardown work from the rescue design).
- Optional: a pre-merge adversarial review of the full branch diff was not run; request one if desired before merge.
