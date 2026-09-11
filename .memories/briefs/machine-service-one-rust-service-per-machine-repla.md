---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-11T12:21:20.893Z
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

- Phases 1, 2, 3, 4, and 6a are landed on `main` at `f57a1655`. Gate findings live in `docs/findings/*-machine-service-phase*-gate.md`. Main is not pushed.
- Phase 5 (edit on the new engine) was absorbed by phase 3 Task 12. The `content` tool and the per-path mutex were never built; `content` is a Miller name and belongs to 6b.
- Phase 6b (Miller tool names, `inspect` and `trace` folds, telemetry rename) is deferred. Decision date: about 2026-09-25, after two weeks of `tool_calls` rows compared to the baseline table in `docs/findings/2026-09-11-machine-service-phase6a-gate.md`.
- Phase 7 (continuous testing, design section 11) needs its own design review before any plan.

## Owner's sequence after 6a (set 2026-09-11)

1. **Hybrid by default search.** `auto` uses hybrid when the semantic index reports ready, else lexical. Gated by `dogfood` and the semantic-value scorecard. Head-to-head evidence: hybrid 77/90 percent top-5 by task class, Julie default 46/20.
2. **Deployment story.** What the machine-service architecture changed for users: install path, user instructions, harness support (Claude Code plugin, Codex, OpenCode, Cursor), and the easiest route to users. Plugin repo needs the two session-start hook files and the skill list.
3. **Code cleanup.** Build warnings (clippy reported 326), dead code, and outdated docs and plans that carry no present value.
4. **Dashboard.** Current state, target state, and functionality.
5. **Troubleshooting on the dashboard.** Logs and error info, plus an easy path for a user to submit a bug report with supporting evidence.

Each item gets its own brainstorm, plan, worktree, and gate finding. Push to origin is still the owner's call.

## Follow-ups (not blocking)

- Service resident memory is about 3.3 GB with eleven workspaces open. Measured, not yet acted on.
- Rust `xtask-eval revival` harness still deferred.

## Where work happens

Start each item in a fresh worktree under `.worktrees/` with `razorback:subagent-driven-development`. Do not implement on the main checkout.
