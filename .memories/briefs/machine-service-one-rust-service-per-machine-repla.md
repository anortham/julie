---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-11T12:13:35.836Z
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

- Phases 1, 2, 3, 4, and 6a are landed on `main` at `f57a1655`. Gate findings live in `docs/findings/*-machine-service-phase*-gate.md`. Main is 132 commits ahead of origin and not pushed.
- Phase 5 (edit on the new engine) was absorbed by phase 3 Task 12. It has no plan or gate of its own. The `content` tool and the per-path mutex were never built; `content` is a Miller name and belongs to 6b.
- Phase 6b (Miller tool names, `inspect` and `trace` folds, telemetry rename) is deferred. Decision date: about 2026-09-25, after two weeks of `tool_calls` rows compared to the baseline table in `docs/findings/2026-09-11-machine-service-phase6a-gate.md`.
- Phase 7 (continuous testing, design section 11) needs its own design review before any plan.

## Open decisions for the owner

1. Push `main` to origin.
2. Default search backend: `auto` is lexical. On the head-to-head natural-language rows hybrid scored 77 and 90 percent top-5 by task class, semantic 77 and 100, Miller 54 and 70, Julie default 46 and 20. Recommendation: make `auto` use hybrid when the semantic index reports ready, gated by `dogfood` and the semantic-value scorecard. Not started.

## Follow-ups (not blocking)

- Plugin repo needs the two session-start hook files and the skill list after 6a (`cargo xtask sync-plugin` does not copy hooks).
- Service resident memory is about 3.3 GB with eleven workspaces open. Measured, not yet acted on.
- Rust `xtask-eval revival` harness still deferred.

## Where work happens

All worktrees and side branches were removed on 2026-09-11. Start each new phase in a fresh worktree under `.worktrees/` with `razorback:subagent-driven-development`. Do not implement on the main checkout.
