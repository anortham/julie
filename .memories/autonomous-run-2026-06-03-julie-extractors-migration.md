# Autonomous Execution Report - julie-extractors external migration

**Status:** Complete
**Plan:** docs/plans/2026-06-02-julie-extractors-migration-plan.md
**Branch:** feat/julie-extractors-external-migration
**PR:** https://github.com/anortham/julie/pull/21
**Duration:** 1 session (resumed after mid-run context compaction)
**Phases:** 5/5 complete (Tasks A–E)
**Tasks:** 5/5 complete

## What shipped
- **Task A (d6b7e153):** Replaced the vendored in-tree `crates/julie-extractors` (path-dep + workspace member) with the external `{ git = "https://github.com/anortham/julie-extractors", tag = "v2.0.3" }` library crate. Synced `SEMANTIC_INDEX_ENGINE_VERSION` to the new `EXTRACTION_CONTRACT_VERSION` (`2026-06-03.ecmascript-swift-shape-v3`), forcing a one-time reindex. Deleted the in-tree crate; regenerated `Cargo.lock` (4 nested git tree-sitter sub-deps resolve). Renamed synthetic in-tree-crate fixture paths in `call_path_tests.rs`.
- **Task B (49628b93):** Removed the now-upstream `extractors` / `extractor-units` / `parser-upgrade` xtask buckets; added one `extractor-dep-integration` gate (engine-version anchor + real-world extraction smoke) to dev + full tiers. Rerouted `src/extractors/` → that bucket; routed root `Cargo.toml`/`Cargo.lock` to the full `dev` fallback (their original `DEV_FALLBACK_FILES` intent, previously shadowed by the parser-upgrade override). Kept `test_tiers.toml` byte-aligned with all exact-match self-tests (`manifest_contract_*`, `changed_tests.rs`).
- **Task C (848e82ff):** Removed the `cargo xtask certify tree-sitter` command surface — deleted `tree_sitter_certification*.rs`, `tree_sitter_real_world*.rs`, the certify test, and every `CliCommand::Certify` / `CertifyCommand` reference in `cli.rs` / `main.rs` / `lib.rs`.
- **Task D (f8d5f97c):** Deleted the golden corpus (`fixtures/extraction/`, ~170 files) + certification output docs (`LANGUAGE_CERTIFICATION_REPORT.md`, `LANGUAGE_REAL_WORLD_EVIDENCE.{json,md}`). Redirected ~14 contributor docs (CLAUDE.md, AGENTS.md, README, docs/**) to point language/parser work at `anortham/julie-extractors` + the re-pin/engine-version-sync workflow.
- **Site fix (1fb09bf2):** Synced `docs/site/index.html` footer v7.13.1 → v7.13.2 (pre-existing drift that was failing `docs_contract_tests_site_marketing_page_stays_current`, blocking the xtask-runner gate). Committed separately with clear attribution.

## Judgment calls (non-blocking decisions made)
- **Step 2 reindex proof method** — Chose two hermetic tests (`test_incremental_indexing_forces_reindex_when_index_engine_version_is_stale` for the `index.rs` path, `test_primary_workspace_repair_plan_reports_semantic_version_changed` for the `startup.rs` catch-up path) over the plan's proposed release-build + live-daemon `daemon.log` grep. Reason: the hermetic tests are a *stronger, deterministic* proof of the same invariant against the real bumped constant, and the live-daemon approach is both risky (a release rebuild arms the daemon's stale-binary auto-restart, which can terminate this MCP session) and flaky (the live index may already be re-stamped). Documented in the ledger's "Step 2 method note".
- **Codex review diff scoping** — Excluded the wholesale `crates/julie-extractors/**` + `fixtures/extraction/**` deletions (~232K lines) from the review diff, with an explicit instruction to flag any *remaining* live reference to deleted paths. Reason: 232K deletion lines are noise that dilutes adversarial attention; the real risk surface is the dep pin, engine-version wiring, and xtask routing.
- **Branch-gate reuse at new HEAD** — After committing the docs-only ledger (HEAD f8d5f97c → bd8b25f9), proved `git diff f8d5f97c..HEAD` is docs/.memories-only and reused the branch-gate evidence rather than re-running ~20 min of byte-identical compilation. Reason: the reuse rule guards against stale evidence after *code* changes; no compiled/test input changed.
- **Rust low symbol count** — Investigated the 4-symbol Rust extraction (vs 142 for TS/Python) rather than waving it off; confirmed it's correct for the intentionally-tiny 17-line smoke fixtures (`add`, `multiply`, `add` import, `main`), not an extraction regression.
- **Excluded working-tree noise from every commit** — Kept `src/daemon/app/helpers.rs` (pre-existing unrelated mod), `.codex/config.toml` (Codex desktop session), and `.miller/` (other-tool state) unstaged throughout; verified before each commit.

## External review (codex, adversarial)
- **Reviewer:** Codex `gpt-5.5 high`, `--sandbox read-only`, structured `--output-schema`. Focused diff (42 files, ~991+/~4230−).
- **Findings:** 0
- **Verified real, fixed:** 0
- **Dismissed:** 0
- **Flagged for your review:** 0
- **Verdict:** approve. Codex confirmed the dependency swap is coherent, the engine-version literal change invalidates old indexes through the existing stored-vs-current comparison, xtask routing removes the old branches while preserving the root-manifest → dev fallback, and the manifest/self-test snapshots align with the new bucket. Its two next-steps (run the gates; verify reindex-fires against a pre-migration index) were already satisfied with stronger evidence. Lead independently re-verified `xtask/src/changed.rs` routing and concurs.

## Tests
- `cargo xtask test dev`: **35/35 buckets pass** (1087.2s) — incl. `extractor-dep-integration` + `tools-workspace` reindex proof + `xtask-runner` self-tests.
- `cargo xtask test dogfood`: **2/2 pass** (179.2s).
- Reindex proof (both paths): **2/2 pass**.
- End-to-end extraction via `julie-server extract` (new crate, `extract_contract_version=3`): Rust 4 sym, TS 142 sym/9 rel/225 id, Python 142 sym/220 id, Swift 32 sym — Swift stdlib filtering observed (single local `implements` edge; stdlib calls captured as identifiers, zero pending cross-file relationships).
- Only warning across all runs: pre-existing `unused import OsString` in `xtask/src/process.rs` (out of scope, present on merge-base).

## Blockers hit
- None.

## Files changed
- 948 files, +1290 / −232295 (full branch `f0ae1db5..HEAD`). The deletion count is dominated by the wholesale removal of the vendored crate + golden corpus; the meaningful modified surface is ~42 files (see the focused review diff).

## Next steps
- Review PR: https://github.com/anortham/julie/pull/21
- On merge, the first session per workspace performs a one-time full reindex (engine-version drift). Expected; no action needed.
- Future extractor/language work: tag in `anortham/julie-extractors`, then re-pin julie's `julie-extractors` git-dep + sync `SEMANTIC_INDEX_ENGINE_VERSION`. Add the Swift stdlib-filtering behavior change to the next release notes.
