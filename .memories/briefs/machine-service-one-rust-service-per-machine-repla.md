---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-10T14:18:52.667Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

## Direction

Build `julie-service`: one process per developer machine, stateless MCP 2026-07-28 over HTTP, a stdio shim, and a plain JSON API, all over the existing request engine. Approved design: `docs/plans/2026-09-09-machine-service-design.md`.

Phase 1 and phase 2 are done on branch `machine-service` (phase 2 gate at `46bb53da`, docs at `63cefbcf` on main). Phase 3 plan: `docs/plans/2026-09-10-machine-service-phase3-plan.md`. Execution is in worktree `/home/murphy/source/julie/.worktrees/machine-service`.

## Non-negotiable rules (design section 4)

- When a task needs a lock, lease, fence, generation, epoch, cursor, claim, pin, coordinator, broker, journal, repair, continuation, or handoff, stop and find a simpler design. Do not power through.
- Two durable roots per checkout (`facts.sqlite`, `tantivy/`), one registry per machine. Derived indexes are deleted and rebuilt, never migrated.
- Delete what you replace in the same change. Phases 2 and 3 must be net negative in lines.
- `src/service/` process model stays under 600 lines.

## Phase 3 status (2026-09-10)

- Tasks 1-10 complete at `88e78c74` (`julie-facts`, graph, snapshot/store, tool ports, vectors).
- Task 11 (workspace commands, startup, seed on facts) is next. A prior implementer died on a Claude session limit with no commit.
- Then Task 12 (edit/health/dashboard/extract), Task 13 (delete `symbols.db`), Task 14 (gates and docs, lead).
- Reviewer choice for this phase: none (user said `approved` with no reviewer name).
- Push and PR authority: missing.

## Known in-scope failure for Task 11

Project-local store predates the Task 10 `encoder` table. Julie CLI currently fails with `no such table: encoder`. Open-mismatch must delete `index_root` and reindex.

## Where work happens

Worktree `/home/murphy/source/julie/.worktrees/machine-service`, branch `machine-service`. Run `razorback:subagent-driven-development` from Task 11. Do not implement on the main checkout.
