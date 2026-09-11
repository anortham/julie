---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-11T14:21:45.284Z
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
- **Semantic auto backend merged** into main at `df586f46` (merge commit, branch and worktree removed). Omitted backend runs semantic for NL queries when vectors are ready. Finding: `docs/findings/2026-09-11-hybrid-auto-backend.md`. The live service still runs the deleted worktree's binary; rebuild main's release, copy the sidecar beside it, and restart before relying on it. A restart deletes and rebuilds every open index until item 2 lands.

## Owner's sequence after 6a (set 2026-09-11)

1. **Semantic by default search.** Done and merged. Auto equals the semantic column: 18/23 top-1, 20/23 top-5, p50 15 ms; scorecard MRR 0.848 vs lexical 0.351.
2. **Registry and index hygiene (owner decision 2026-09-11).** Run `run_cleanup_sweep` in the background at service start; today only `manage_workspace list` runs it. Give every test an isolated `JULIE_HOME`; the CLI bucket writes 36 dead rows per run into the live registry. **Upgraded after the 2026-09-11 incident (checkpoint `6bbb54be`):** live index directories were deleted twice during the day. Suspects: `open_or_recreate` deletes `indexes/<id>/` on any open failure, including a transient busy timeout while a restarting service still writes; force reindex deletes the primary index dir and some tests run against the live home. Also: the service writes no log file, so nothing could confirm which path fired; `readiness.status` says ready before vectors are searchable; `service status` reports `vector_count` 0 for indexes that have vectors on disk after a restart.
3. **Test speed (owner decision 2026-09-11).** "We spend 90 percent of our time waiting on tests." Make the suites faster or run fewer of them. Candidates: prebuild cost (126 s cold for nano), the 40 s fixture build for dogfood, `search-quality` copying a 300 MB fixture per test (needs `NEXTEST_TEST_THREADS=6` on tmpfs), duplicate coverage across buckets (`cargo xtask test inventory`), and gate tiers that rerun what `changed` already ran.
4. **Deployment story.** What the machine-service architecture changed for users: install path, user instructions, harness support (Claude Code plugin, Codex, OpenCode, Cursor), and the easiest route to users. Plugin repo needs the two session-start hook files and the skill list. Astra's review: the tracked `.agents/skills/editing` copy still mandates the deleted rewrite and rename tools while the `.claude` copy is updated.
5. **Code cleanup.** Build warnings (clippy reported 326), dead code, and outdated docs and plans that carry no present value.
6. **Dashboard.** Current state, target state, and functionality.
7. **Troubleshooting on the dashboard.** Logs and error info, plus an easy path for a user to submit a bug report with supporting evidence. Start with a service log file (item 2 found none).

Each item gets its own brainstorm, plan, worktree, and gate finding. Push to origin is still the owner's call.

## Astra review findings on main 2c404bbc (2026-09-11, checkpoint `c66c5a4f`)

Read-only review by three reviewers. Recommends: keep facts plus immutable snapshots plus one semantic child; then in order: lightweight status, canonical bounded store ownership, faithful paging, coherent client guidance, then held-out agent tasks and concurrent-worktree comparison.

- `GET /status` runs `manage_workspace status`, which opens full checkout stores and keeps them in the handler's `ref_store_cache`; they can duplicate runtime-owned stores. The design's memory bound and LRU are absent.
- `RuntimeFactory::acquire` awaits cold initialization while holding the global runtimes write lock.
- Paging `next:` trailers omit `workspace` and non-default filters across search, refs, symbols, and impact.
- The evaluation (23 NL cases, one expected path each) is diagnostic evidence, not a held-out product contest against Miller.
- Treat full-graph re-resolution on edits and shared child query/batch contention as measured follow-ups, not reasons to add infrastructure.

## Follow-ups (not blocking)

- Service resident memory: 3.3 GB in 6a with 11 checkouts; 5.2 GB during hybrid-auto with 13 live plus 37 dead checkouts. Re-measure after item 2 and the status fix above.
- Backfill runs at about 10 vectors/s round-robin across every open checkout; ten corpus repos take about 45 minutes from cold.
- `edit_file` through MCP writes relative to the primary workspace, so an agent working in a worktree cannot use it safely (Opus implementer report, 2026-09-11).
- The sidecar binary comes from `~/source/julie-semantic-sidecar/target/release/`; copy it beside `julie-server` before any semantic run (memory note `sidecar-binary-location`).
- Rust `xtask-eval revival` harness still deferred.

## Where work happens

Start each item in a fresh worktree under `.worktrees/` with `razorback:subagent-driven-development`. Do not implement on the main checkout.
