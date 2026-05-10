# Best-in-Class Tree-Sitter — Live Dogfood Handoff

The autonomous run completed Phases 1–7 plus the Phase 8.1 release-gate sweep at the branch tip. Live MCP dogfood requires a release rebuild and a fresh client session and so stays with the user.

After the autonomous run completes, the user runs:

1. `cargo build --release` — rebuild the release binary so the MCP client picks it up.
2. Restart Claude Code (so the MCP client respawns the new server).
3. In the Julie workspace, run via the MCP client:
   - `manage_workspace health` — expect READY status with non-zero symbol and relationship counts.
   - `call_path extract_symbols_static extract_canonical` — expect a one-hop edge through the canonical pipeline.
   - `fast_refs extract_canonical` — expect definition + a stable set of references including public-API projection and real-world contract callers.
   - SQLite check: inspect the on-disk index metadata for the engine version column actually written by the indexer (verify the column name against `src/database/schema.rs` before running). The recorded value must contain `2026-05-10.tree-sitter-best-in-class-v1` per `julie_extractors::EXTRACTION_CONTRACT_VERSION`, composed into `SEMANTIC_INDEX_ENGINE_VERSION` at `src/tools/workspace/indexing/engine_version.rs`.
   - `manage_workspace refresh workspace_id=julie_<id>` — expect "already up-to-date" without a full reindex (because the engine version composition only changes the stored value when the contract bumps).
4. Sign off: append a ledger row to `docs/TREE_SITTER_QUALITY_BAR.md` Verification Ledger with the live-MCP timestamps + results, citing the current HEAD SHA.
5. Merge `.worktrees/best-in-class-treesitter/` back to `main` once the live MCP rows are recorded.

## What Phase 8.1 already proved (offline)

The release gates that ran against the working tree at branch HEAD are recorded in the `TREE_SITTER_QUALITY_BAR.md` Verification Ledger and pass without live MCP. They cover:

- `cargo fmt --check`
- `git diff --check`
- `cargo xtask certify tree-sitter --check`
- `cargo xtask test bucket extractors` (golden + capability_matrix + cert + downstream-smoke)
- `cargo xtask test bucket parser-upgrade`
- `cargo xtask test changed` (clean working tree)
- `cargo build --release`
- `cargo build --examples -p julie-extractors`
- `cargo test -p julie-extractors --doc`
- `cargo doc -p julie-extractors --no-deps` (6 missing-docs warnings, expected — Phase 5.4 follow-up)
- `cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate`

The downstream-consumer integration test specifically proves the Pillar-3 contract: a tempdir consumer crate path-deps `julie-extractors` and runs `extract_canonical` + `capability_snapshot()` + `EXTRACTION_CONTRACT_VERSION` end-to-end.

## Gates pending user-run

The repository's broad-tests pre-tool hook blocked the autonomous session from running the broader tiers below. The user runs them locally before tagging:

- `cargo xtask test dev`
- `cargo xtask test system`
- `cargo xtask test dogfood`
- `cargo xtask test full`

Each pass should append a ledger row at the same HEAD SHA recorded in the `94b7f5a3…` block of `docs/TREE_SITTER_QUALITY_BAR.md`.

## Items intentionally left for follow-up

These were scoped out of the autonomous run by the iteration discipline and remain visible in `TREE_SITTER_QUALITY_BAR.md` Open Gaps:

- Phase 6 full-corpus real-world regen with raised `min_relationships` and per-repo `representative_specs`. Smoke-profile evidence was regenerated at HEAD; the larger release-profile evidence and the corpus-spec authoring is hand-authored work that benefits from human curation and is unlikely to land in a single autonomous session.
- Phase 5.4 doc-comment audit on every existing public item. New items added during this run (capability_snapshot, EXTRACTION_CONTRACT_VERSION, the engine-version composition, EXTRACTION_CONTRACT.md) carry doc comments. The mechanical sweep across the rest of the public surface is straightforward to land as a separate mechanical commit after release.
