# Machine Service Phase 3 Gate

**Date:** 2026-09-10
**Branch:** `machine-service` at `/home/murphy/source/julie/.worktrees/machine-service`
**HEAD:** `255f9b59` (tier redefinition). Product commits through `d77f0810`.
**Verdict:** not passed. Facts, graph, snapshots, and deletion of `symbols.db` are on the branch. Timed gates for `fast`, trimmed `full`, `dev`, `system`, `service-process`, and `dogfood` hold. Module-size budget does not: `crates/julie-runtime/src/workspace/mod.rs` is 735 lines and `src/handler.rs` is 1616 lines (limit 500, excluding tests).

## Section 12 measurements

| Budget | Value | Command | Result |
|---|---|---|---|
| Durable roots | 2 per checkout (`facts.sqlite`, `tantivy/`), 1 registry per machine | `cargo nextest run --lib tests::service::durable_roots`; Miller/Julie `GET /status` plus `find $JULIE_HOME` | pass |
| Fast bucket | under 10 s warm, unit tests only | `cargo xtask test fast` three times at `255f9b59` | pass. Warm 3.3s, 3.4s, 3.3s. Median 3.3s. Declared 9s (2+2+5) |
| Full suite, Linux | under 120 s; exclude model download, real-repo fixtures, Windows | `cargo xtask test full` three times at `255f9b59` (dogfood, extractor-dep, and Windows not in `full`) | pass. Warm 93.1s, 93.8s, 93.2s. Median 93.2s |
| Multi-process tests | one bucket, under 20 s | `cargo xtask test bucket service-process` (via `dev` at `ec3c16cd`) | pass, 3.1 s warm |
| Coordination words | zero without a written exception | `sh scripts/complexity-words.sh main` | SourceEditCoordinator on edit-tool wiring (ADR-0004). Exception recorded |
| Module size | 500 lines per file, excluding tests | `wc -l` on implementation files | fail. `workspace/mod.rs` 735; `src/handler.rs` 1616 |
| Net lines | phase 3 negative | `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` | pass. 145,367 at `63cefbcf` → 106,940 at `fd707d9e` |
| Clean build time | report, then +20% later | `cargo clean && time cargo build` | not measured (would wipe the worktree build cache) |
| Resident memory | Julie and Miller from `/status` | temp `JULIE_HOME`, `workspace index`, `service status --json` | Julie: rss 257,634,304; graph_resident_bytes 85,980,600; 56,230 symbols. Miller: rss 939,204,608; graph_resident_bytes 330,214,728; 220,334 symbols. `vector_count` 0 (`--semantics off`). No `vector_scan_millis` field |

## Docs

`CLAUDE.md`, `AGENTS.md`, `docs/WORKSPACE_ARCHITECTURE.md`, and `docs/SEARCH_FLOW.md` describe `facts.sqlite` and snapshots, not `symbols.db` as product behavior.

## What remains before a pass

1. Split `src/handler.rs` and `crates/julie-runtime/src/workspace/mod.rs` to ≤500 lines.
2. Optional: add `vector_scan_millis` to checkout status; measure with semantics on.
3. Optional: record a clean-build number on a disposable tree.
