---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-12T12:47:54.885Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

# Machine service: one Rust service per machine replaces per-session Miller and Julie

## Direction

Ship v8 as one Rust machine service with stateless MCP over HTTP, a stdio shim, and a JSON API over the shared request engine. The approved design remains docs/plans/2026-09-09-machine-service-design.md.

## Approved architecture constraints

- Every scoped MCP call uses an explicit absolute workspace path or registered ID. No shim cwd or JULIE_WORKSPACE inference; terminal CLI cwd convenience remains.
- Each checkout owns facts.sqlite and Tantivy; one registry and one service exist per machine.
- Preserve the OS-held service singleton and existing mutation gate. No PID election, leases, fencing, brokers, additional durable coordination stores, or derived-index migrations.
- Preserve tool names until the phase-6b decision after enough real telemetry, no earlier than approximately 2026-09-25. Continuous testing requires its own design decision.

## Current source and publication state

Deployment work was merged locally at cda68bed; Julie main is eabf93cb and plugin main db4dd894. Previous cda68bed evidence: full gate 2221 development + 13 CLI + 69 dogfood tests; plugin 22 tests. This is historical verification, not evidence for future changes.

GitHub checks during the assessment found no v8.0.0 release; latest was v7.18.1. Plugin public main remains an orphan distribution history. Publication order and a retained source reference for any pinned reusable workflow must be explicit. No push, tag, publication, deployment, or release is authorized.

## New planning direction

The owner requested actionable plans from the Julie-versus-Miller assessment. The roadmap is docs/plans/2026-09-11-revival-roadmap.md, with five linked proposed implementation briefs: correctness, retrieval contracts, runtime lifecycle, deployment/guidance, and replacement qualification. They were independently challenged by Sol and reconciled by Astra. The owner authorized Plan 1 on 2026-09-11. Its task worktree is .worktrees/revival-correctness on fix/revival-correctness. Plan 1 is implemented and verified locally through source commit 9faf6fb4: watcher recovery, request isolation, truthful eligible-vector coverage, restart catch-up, health parity, and shared HTTP/MCP mode validation. Full gate passed 2245 development + 13 CLI + 69 dogfood tests; the isolated native protocol probe passed 50-to-51 recovery with retained vector bytes, concurrent Off/Required, rapid edits, checkout isolation, and stdio routing. See .memories/autonomous-run-2026-09-12-revival-correctness.md. The final report commit receives a fresh exact-HEAD gate before handoff. Plans 2–5 remain proposals, and publication remains unauthorized.

Proposed next priority is freshness and request isolation, then precise retrieval and one production lifecycle, then installation qualification and measured adoption. General dashboard expansion remains outside these plans. Navigation-default promotion and full Miller retirement are separate decisions; symbol operations, existing web bridge coverage, external content and continuous testing receive explicit evidence-based dispositions rather than silent deferral or automatic ports.

## Findings that constrain execution

- Plan 1 restores watcher rescan consumption, preserves unreadable files, and rebuilds projections after partial publication. Keep the existing mutation gate and newer recovery obligations intact.
- Plan 1 keeps request semantics local and shares validated HTTP/MCP mode parsing. Retain the deterministic same-handler race regression.
- Plan 1 resumes partial embeddings. Eligibility comes from pipeline filters and variable budgets over the current snapshot, with distinct blob/ordinal/encoder storage keys; health and Required readiness share this calculation.
- Active RuntimeFactory has global cold-init locking and no runtime eviction. Old lifecycle machinery is not production. Safe teardown must join blocking embedding writers, not just abort outer async handles.
- Reference sites lose spans/identity; scoped content retrieval and paging/completeness need better contracts.
- Correction to the assessment: call_path(mode="web") already exists. Extend and qualify it; do not build another bridge subsystem.
- Nine final MCP schemas already require workspace. The actual guidance gaps are management examples, stale loaded instructions, and first-512 placement.
- Versioned installs must preserve dev-link's explicit maintainer override. Current win-test helper does not support Julie. Four native artifact/client routes still need qualification.

## References

- docs/findings/2026-09-11-julie-miller-replacement-review.md
- docs/plans/2026-09-11-revival-roadmap.md
- docs/findings/2026-09-11-v8-architecture-audit.md
- docs/findings/2026-09-11-machine-service-phase6a-gate.md
