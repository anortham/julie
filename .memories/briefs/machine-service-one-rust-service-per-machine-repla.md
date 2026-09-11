---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-11T18:31:55.117Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

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
- **Semantic auto backend merged** into main at `df586f46`. Omitted backend runs semantic for NL queries when vectors are ready. Finding: `docs/findings/2026-09-11-hybrid-auto-backend.md`.
- **Index and registry hygiene merged** into main at `9b0e65b2` (merge commit, worktree removed). Finding: `docs/findings/2026-09-11-index-hygiene.md`. Report: `.memories/autonomous-run-2026-09-11-index-hygiene.md`. The live service (pid 142088 at 13:31) runs main's release binary with the sidecar beside it; the restart kept all 11 index dirs and 27,859 vectors. Service log: `~/.julie/logs/julie-service.log.<date>`.

## Owner's sequence after 6a (set 2026-09-11)

1. **Semantic by default search.** Done and merged. Auto equals the semantic column: 18/23 top-1, 20/23 top-5, p50 15 ms; scorecard MRR 0.848 vs lexical 0.351.
2. **Registry and index hygiene.** Done and merged (see Status). Index dirs are deleted only on a typed facts version mismatch; status never opens through a deleting path; restart waits on the old pid and only the owner removes `service.json`; tests run under `target/test-julie-home`; one background cleanup sweep at service start; the service now has a tracing subscriber and a log file. Deferred from the finding: dashboard error buffer layer never installed, partial vector backfill does not resume after restart, `new_files` repair on every restart, RSS 2.9 GB with eleven workspaces, stray `/tmp` registry row.
3. **Test speed (owner decision 2026-09-11). NEXT.** "We spend 90 percent of our time waiting on tests." Make the suites faster or run fewer of them. Candidates: prebuild cost (126 s cold for nano), the 40 s fixture build for dogfood, `search-quality` copying a 300 MB fixture per test (needs `NEXTEST_TEST_THREADS=6` on tmpfs), duplicate coverage across buckets (`cargo xtask test inventory`), and gate tiers that rerun what `changed` already ran.
4. **Deployment story.** What the machine-service architecture changed for users: install path, user instructions, harness support (Claude Code plugin, Codex, OpenCode, Cursor), and the easiest route to users. Plugin repo needs the two session-start hook files and the skill list. Astra's review: the tracked `.agents/skills/editing` copy still mandates the deleted rewrite and rename tools while the `.claude` copy is updated.
5. **Code cleanup.** Build warnings (clippy reported 326), dead code, and outdated docs and plans that carry no present value.
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

- Service resident memory: 2.9 GB after item 2 with 11 checkouts (was 3.3 GB in 6a; 5.2 GB during hybrid-auto with 13 live plus 37 dead checkouts).
- Backfill runs at about 10 vectors/s round-robin across every open checkout; ten corpus repos take about 45 minutes from cold. A partial backfill does not resume after restart.
- `edit_file` through MCP writes relative to the primary workspace, so an agent working in a worktree cannot use it safely (Opus implementer report, 2026-09-11).
- The sidecar binary comes from `~/source/julie-semantic-sidecar/target/release/`; copy it beside `julie-server` before any semantic run (memory note `sidecar-binary-location`).
- Rust `xtask-eval revival` harness still deferred.

## Where work happens

Start each item in a fresh worktree under `.worktrees/` with `razorback:subagent-driven-development`. Do not implement on the main checkout.
