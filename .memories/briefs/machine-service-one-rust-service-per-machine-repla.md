---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-09T21:12:03.049Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

## Direction

Build `julie-service`: one process per developer machine, stateless MCP 2026-07-28 over HTTP, a stdio shim, and a plain JSON API, all over the existing request engine. Approved design: `docs/plans/2026-09-09-machine-service-design.md`. Phase 1 plan: `docs/plans/2026-09-09-machine-service-phase1-plan.md`.

## Non-negotiable rules (design section 4)

- When a task needs a lock, lease, fence, generation, epoch, cursor, claim, pin, coordinator, broker, journal, repair, continuation, or handoff, stop and find a simpler design. Do not power through.
- Two durable roots per checkout (`facts.sqlite`, `tantivy/`), one registry per machine. Derived indexes are deleted and rebuilt, never migrated.
- Delete what you replace in the same change. Phases 2 and 3 must be net negative in lines.
- `src/service/` process model stays under 600 lines (design section 16; Julie's June 2026 daemon was 11.5k lines and was deleted for it).

## Decisions already made

- One store per checkout, seeded by copying from a sibling worktree. No shared family storage (user accepted 2026-09-09).
- No MCP Tasks extension, no live version handoff, no continuations, no vacuum job, no usearch file, no Python embedding host.
- Native sidecar in `serve` mode is the embedding runtime. Model chosen by scorecard in phase 4.
- Miller's ten-tool contract carries over in phase 6. CT deferred to phase 7 as its own design.
- Reviewer choice for pre-merge review: Codex.

## Phase gates

1. After phase 1: three real hosts (Claude Code, Codex, Cursor) work over HTTP and the shim with no session state and no Tasks extension. Else stop.
2. After phase 2: deletion list in design section 5.4 is empty; net lines negative.
3. After phase 3: every budget in design section 12 holds on the Julie and Miller repos. Else stop and redesign before phase 4.

## Where work happens

Worktree `/home/murphy/source/julie/.worktrees/machine-service`, branch `machine-service`, off `main` at 8b07074b. Run `razorback:subagent-driven-development` with the phase 1 plan.
