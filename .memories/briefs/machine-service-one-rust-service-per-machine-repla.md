---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-11T22:13:57.580Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

# Machine service: one Rust service per machine replaces per-session Miller and Julie

## Direction

Build `julie-service`: one process per developer machine, stateless MCP over HTTP, a stdio shim, and a plain JSON API, all over the existing request engine. Approved design: `docs/plans/2026-09-09-machine-service-design.md`. Phase list is section 14.

## Non-negotiable rules (design section 4)

- When a task needs a lock, lease, fence, generation, epoch, cursor, claim, pin, coordinator, broker, journal, repair, continuation, or handoff, stop and find a simpler design. Do not power through.
- Two durable roots per checkout (`facts.sqlite`, `tantivy/`), one registry per machine. Derived indexes are deleted and rebuilt, never migrated.
- Delete what you replace in the same change. Each phase must be net negative in lines.
- The tool name, description, server instructions, and skill are one fragile unit. Change them only with telemetry (memory note `keep-proven-tool-surface`).

## Status (2026-09-11)

- Phases 1, 2, 3, 4, and 6a are landed on `main`. Gate findings live in `docs/findings/*-machine-service-phase*-gate.md`. Main is not pushed.
- Phase 5 (edit on the new engine) was absorbed by phase 3 Task 12. The `content` tool and the per-path mutex were never built; `content` is a Miller name and belongs to 6b.
- Phase 6b (Miller tool names, `inspect` and `trace` folds, telemetry rename) is deferred. Decision date: about 2026-09-25, after two weeks of `tool_calls` rows compared to the baseline table in `docs/findings/2026-09-11-machine-service-phase6a-gate.md`.
- Phase 7 (continuous testing, design section 11) needs its own design review before any plan.
- **Semantic auto backend merged** into main at `df586f46`. Finding: `docs/findings/2026-09-11-hybrid-auto-backend.md`.
- **Index and registry hygiene merged** into main at `9b0e65b2`. Finding: `docs/findings/2026-09-11-index-hygiene.md`. Service log: `~/.julie/logs/julie-service.log.<date>`.
- **Test speed merged** into main at `8c4d74b0` (merge commit, worktree removed). Finding: `docs/findings/2026-09-11-test-speed.md`. Report: `.memories/autonomous-run-2026-09-11-test-speed.md`.
- **Deployment story ready for the owner** on branch `deployment-story` (worktree `.worktrees/deployment-story`) and plugin branch `v8-deployment` in `~/source/julie-plugin`. Finding with the owner runbook: `docs/findings/2026-09-11-deployment-story.md`. Not merged, not tagged, not pushed. The live service runs the branch's 8.0.0 release binary.

## Test commands (since item 3)

Three tiers only, each one `cargo nextest run --workspace` call: `cargo xtask test dev` (12 s warm), `dogfood` (15 s with the fixture), `full` (26 s). Edit loop: `cargo nextest run --lib <name>` (3.5 s incremental). No buckets, no `changed`, no `NEXTEST_TEST_THREADS`; `.config/nextest.toml` caps the dogfood set at six threads. Process-fixture tests set `JULIE_SERVICE_IDLE_SECS=2` so spawned services exit.

## Owner's sequence after 6a (set 2026-09-11)

1. **Semantic by default search.** Done and merged. Auto equals the semantic column: 18/23 top-1, 20/23 top-5, p50 15 ms; scorecard MRR 0.848 vs lexical 0.351.
2. **Registry and index hygiene.** Done and merged. Deferred from the finding: dashboard error buffer layer never installed, partial vector backfill does not resume after restart, `new_files` repair on every restart, RSS 2.9 GB with eleven workspaces, stray `/tmp` registry row.
3. **Test speed.** Done and merged. dev 50.5 s to 12.2 s; full 94 s to 26.4 s with dogfood included; dogfood 124 s to 14.6 s; 319 unrun tests now run; xtask 11,470 to 2,233 lines; dogfood store open 8.4 s to 1.4 s (re-export index plus opt-level 1 for julie-index); three 10 s waits removed; process-fixture service leak fixed (129 processes, 8.2 GB). Deferred: `workspace_isolation_smoke` 5 to 14 s floor, `store_open` 5 s lock waits, dogfood six-thread cap, Alamofire graph load 3 s in release, other parity tests call the CLI without `--standalone`.
4. **Deployment story (v8.0.0).** Done on the branch; the owner merges, tags, and pushes with the runbook in the finding. Release archives carry `julie-semantic-sidecar` 0.1.0 (`.github/scripts/pack-release.sh`, sha256 pinned); version 8.0.0 plus `docs/release-notes/v8.0.0.md`; one routing text `JULIE_AGENT_INSTRUCTIONS.md` served on `initialize` and printed by the session hooks; plugin repo with a manifest per harness; six harness checks pass. Design change from the checks: Codex and Antigravity start a plugin's MCP servers inside the plugin directory and send no roots, so both register Julie from their own config and the plugin carries only skills and hooks for them. Product fix: the stdio shim honors `JULIE_WORKSPACE`. Deferred: a one-step Codex or Antigravity path needs a harness signal that names the project; an Antigravity hook (root `hooks.json`); plugin manifests at 7.18.0 until the workflow bumps them.
5. **Code cleanup. NEXT.** Build warnings (clippy reported 326), dead code, and outdated docs and plans that carry no present value.
6. **Dashboard.** Current state, target state, and functionality.
7. **Troubleshooting on the dashboard.** Logs and error info, plus an easy path for a user to submit a bug report with supporting evidence. The service log file from item 2 is the starting point.

Each item gets its own brainstorm, plan, worktree, and gate finding. Push to origin is still the owner's call.

## Astra review findings on main 2c404bbc (2026-09-11, checkpoint `c66c5a4f`)

Read-only review by three reviewers. Recommends: keep facts plus immutable snapshots plus one semantic child; then in order: lightweight status, canonical bounded store ownership, faithful paging, coherent client guidance, then held-out agent tasks and concurrent-worktree comparison.

- `GET /status` runs `manage_workspace status`, which opens full checkout stores and keeps them in the handler's `ref_store_cache`; they can duplicate runtime-owned stores. The design's memory bound and LRU are absent. Item 2 made this path non-destructive; the memory bound is still absent.
- `RuntimeFactory::acquire` awaits cold initialization while holding the global runtimes write lock.
- Paging `next:` trailers omit `workspace` and non-default filters across search, refs, symbols, and impact.
- The evaluation (23 NL cases, one expected path each) is diagnostic evidence, not a held-out product contest against Miller.
- Treat full-graph re-resolution on edits and shared child query/batch contention as measured follow-ups, not reasons to add infrastructure.

## Follow-ups (not blocking)

- Service resident memory: 1.4 GB right after restart, 2.9 GB after item 2 with 11 checkouts open.
- Backfill runs at about 10 vectors/s round-robin across every open checkout; ten corpus repos take about 45 minutes from cold. A partial backfill does not resume after restart.
- `edit_file` through MCP writes relative to the primary workspace, so an agent working in a worktree cannot use it safely (Opus implementer report, 2026-09-11).
- The sidecar binary comes from `~/source/julie-semantic-sidecar/target/release/`; copy it beside `julie-server` before any semantic run (memory note `sidecar-binary-location`).
- A test binary built in a worktree that shares `target/` bakes that worktree's path into `CARGO_MANIFEST_DIR`; touch `src/lib.rs` after switching checkouts.
- A running stdio shim from an older build refuses a newer service until the harness restarts it (item 4 side finding).
- Workers' Julie MCP calls failed with "could not start service: No such file or directory" during item 4 Batch A while the lead's worked; not reproduced.
- Rust `xtask-eval revival` harness still deferred.

## Where work happens

Start each item in a fresh worktree under `.worktrees/` with `razorback:subagent-driven-development`. Do not implement on the main checkout.
