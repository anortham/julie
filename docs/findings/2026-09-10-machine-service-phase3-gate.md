# Machine Service Phase 3 Gate

**Date:** 2026-09-10
**Branch:** `machine-service` at `/home/murphy/source/julie/.worktrees/machine-service`
**Verdict:** not passed. Code for facts, graph, snapshots, and deletion of `symbols.db` is on the branch. `dev`, `system`, `service-process`, and `dogfood` are green. `full` is not green. Fast is not yet unit-only under 10 s. Miller `/status` is not yet measured.

## Section 12 measurements

| Budget | Value | Command | Result |
|---|---|---|---|
| Durable roots | 2 per checkout (`facts.sqlite`, `tantivy/`), 1 registry per machine | `cargo nextest run --lib tests::service::durable_roots` | pass |
| Fast bucket | under 10 s warm, unit tests only | `cargo xtask test fast` | not yet redefined; current `fast` includes `service` (~15 s) |
| Full suite, Linux | under 120 s; exclude model download, real-repo fixtures, Windows | `cargo xtask test full` | fail: empty julie-tools module `search_annotation_search_tests` (exit 4). Command removed from `test_tiers.toml`; full not re-run |
| Multi-process tests | one bucket, under 20 s | `cargo xtask test bucket service-process` | pass, 3.1 s warm at `ec3c16cd` |
| Coordination words | zero without a written exception | `sh scripts/complexity-words.sh main` | SourceEditCoordinator on edit-tool wiring (ADR-0004). Exception recorded |
| Module size | 500 lines per file, excluding tests | inspection | `workspace/mod.rs` 735, `handler.rs` 1616 still over |
| Net lines | phase 3 negative | `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` | 145,367 at `63cefbcf` → 106,940 at `fd707d9e` |
| Clean build time | report, then +20% later | `cargo clean && time cargo build` | not measured |
| Resident memory | Julie and Miller from `/status` | open + `GET /status` | not measured |

## Docs

`CLAUDE.md`, `AGENTS.md`, `docs/WORKSPACE_ARCHITECTURE.md`, and `docs/SEARCH_FLOW.md` now describe `facts.sqlite` and snapshots, not `symbols.db` as product behavior.

## What remains before a pass

1. Re-run `cargo xtask test full` after empty-command removal.
2. Redefine `fast` as unit-only buckets with declared sum ≤ 10 s. Time it three times.
3. Measure Miller by opening `/home/murphy/source/miller` in a temporary `JULIE_HOME` and reading `/status`.
4. Measure clean build time.
5. `cargo xtask test dev` at the final docs/measurement commit.
