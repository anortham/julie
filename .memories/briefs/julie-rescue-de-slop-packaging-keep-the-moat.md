---
id: julie-rescue-de-slop-packaging-keep-the-moat
title: "Julie Rescue: de-slop packaging, keep the moat"
status: active
created: 2026-06-03T14:35:36.960Z
updated: 2026-06-06T14:29:27.756Z
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
- Current docs describe the in-process stdio server, `$JULIE_HOME/registry.db`, per-workspace `leader.lock`, project-local logs, standalone read-only dashboard, and resident embedding host.
- The `julie-plugin` launcher fix is prepared in `/Users/murphy/source/julie-plugin-single-server` on `fix/single-server-launcher` at `4165fcb` (`fix: launch julie-server directly`). It launches `julie-server` directly and keeps only legacy split-daemon cleanup.
- The first test-economy cut is implemented locally: `dev` is now 27 buckets / 589s expected, protected by an xtask contract that caps it at 600s. The first actual `cargo xtask test dev` pass ran 27 buckets in 389.7s after calibrating the `core-database` timeout. `full` still keeps 44 buckets / 2519s expected.

The rescue is not done. The active bottleneck is now splitting the broad buckets that were removed from `dev`: `tools-workspace`, `tools-search-line`, `tools-editing`, `tools-workspace-targeting`, `tools-search-format-quality`, and `tools-call-path`.

## Constraints

- Preserve Julie's moat: semantic/hybrid search, graph-centrality reranking, token-budgeted `get_context`, 34-language breadth, CLI/plugin shipping path.
- Keep behavior working while deleting complexity. Deletion is only a win when caller-facing tool behavior and focused gates stay green.
- Do not reintroduce daemon/adapter process management. The only intended resident extra process is the embedding host.
- Keep broad verification available in `full` while making `dev` cheap enough for normal agent use.

## Success Criteria

- Default branch verification stays materially below the old 30-minute pain point; current contract is `dev <= 600s` expected and the first actual pass is 389.7s.
- Slow handler-bound buckets are split, relocated, or demoted to broader release gates with evidence.
- Stale daemon vocabulary is removed from current user-facing docs and gradually retired from internal names (`DaemonDatabase` -> registry role, `daemon` bucket -> registry/runtime role).
- Phase 4 tool consolidation starts only after the test loop is cheap enough to support it.

## References

- `docs/plans/2026-06-06-julie-rescue-current-status.md` — current source of truth for what's done and what's left.
- `docs/plans/2026-06-06-julie-test-economy-plan.md` — active plan for the fast `dev` tier and slow bucket splits.
- `docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md` — daemon/adapter teardown plan and verification ledger.
- `docs/plans/2026-06-03-julie-rescue-design.md` — original strategy and rationale.
