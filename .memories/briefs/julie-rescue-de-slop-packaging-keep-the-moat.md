---
id: julie-rescue-de-slop-packaging-keep-the-moat
title: "Julie Rescue: de-slop packaging, keep the moat"
status: active
created: 2026-06-03T14:35:36.960Z
updated: 2026-06-06T13:39:24.934Z
tags:
  - julie-rescue
  - current-status
  - test-economy
  - daemon-teardown
---

## Decision

Save Julie in place. Do not switch to Miller (.NET) or eros (Python). The rescue has confirmed the core bet: Julie's retrieval moat is real enough to keep, and the pain is packaging/runtime/test economics rather than Rust itself.

## Current Status (2026-06-06)

Julie is already well past the original starting point:

- `julie-core`, `julie-index`, `julie-pipeline`, `julie-context`, `julie-tools`, `julie-runtime`, and `julie-test-support` now exist.
- The old HTTP daemon + stdio adapter runtime has been deleted through Phase 3d.3 and merged to `main` at `49b86689`.
- Current docs now describe the in-process stdio server, `$JULIE_HOME/registry.db`, per-workspace `leader.lock`, project-local logs, standalone read-only dashboard, and resident embedding host.

The rescue is not done. The active bottleneck is now the test loop: `cargo xtask test list` still reports `dev` as 37 buckets / ~35 minutes expected, while the latest 3d.3 affected-change gate passed in 958.5s. That is better, but still too slow for normal agent iteration.

## Constraints

- Preserve Julie's moat: semantic/hybrid search, graph-centrality reranking, token-budgeted `get_context`, 34-language breadth, CLI/plugin shipping path.
- Keep behavior working while deleting complexity. Deletion is only a win when caller-facing tool behavior and focused gates stay green.
- Do not reintroduce daemon/adapter process management. The only intended resident extra process is the embedding host.
- Treat `julie-plugin` packaging as release-blocking because it still references deleted adapter/daemon binary names.

## Success Criteria

- Default branch verification is materially below the old 30-minute pain point, ideally under 10 minutes for the normal `dev`/changed loop.
- Slow handler-bound buckets are split, relocated, or demoted to broader release gates with evidence.
- Stale daemon vocabulary is removed from current user-facing docs and gradually retired from internal names (`DaemonDatabase` -> registry role, `daemon` bucket -> registry/runtime role).
- Phase 4 tool consolidation starts only after the test loop is cheap enough to support it.

## References

- `docs/plans/2026-06-06-julie-rescue-current-status.md` — current source of truth for what's done and what's left.
- `docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md` — daemon/adapter teardown plan and verification ledger.
- `docs/plans/2026-06-03-julie-rescue-design.md` — original strategy and rationale.
