---
id: julie-rescue-de-slop-packaging-keep-the-moat
title: "Julie Rescue: de-slop packaging, keep the moat"
status: active
created: 2026-06-03T14:35:36.960Z
updated: 2026-06-06T20:16:35.189Z
tags:
  - julie-rescue
  - current-status
  - test-economy
  - daemon-teardown
  - bucket-splitting
  - release-v7.13.3
  - dashboard
  - julie-plugin
---

## Decision

Save Julie in place. Do not switch to Miller (.NET) or eros (Python). The rescue has confirmed the core bet: Julie's retrieval moat is real enough to keep, and the pain is packaging/runtime/test economics rather than Rust itself.

## Current Status (2026-06-06)

Julie is now in v7.13.3 release-candidate shape on local `main`, ahead of `origin/main` by 9 commits. The key rescue work is committed locally through `d136921b` (`fix(dashboard): register in-process workspaces`).

What is done:

- The old HTTP daemon + stdio adapter runtime has been deleted through Phase 3d.3 and merged into local `main`; the no-args `julie-server` now serves MCP in-process over stdio with per-workspace leader locking.
- User-facing docs now tell manual MCP users to launch `julie-server` directly because `julie-adapter` and `julie-daemon` are gone.
- `manage_workspace(operation="dashboard")` is implemented and committed; MCP sessions can launch the dashboard from inside the session.
- The dashboard zero-workspaces regression is fixed: in-process MCP handlers now open the workspace registry DB and register loaded primary workspaces, so registry-backed dashboard/list views can see the healthy primary workspace after restart.
- Test economy work has materially reduced the normal branch gate from the old ~30-minute pain point. `dev` is capped at <=600s expected; broad workspace and line-search buckets have been split. Current final dashboard-fix `cargo xtask test changed` passed 28 buckets in 440.6s.
- v7.13.3 metadata and release notes are prepared in Julie. Fresh release build passed and `target/release/julie-server --version` reports `julie-server 7.13.3`.
- `julie-plugin` single-server launcher work is prepared in `/Users/murphy/source/julie-plugin-single-server` on `fix/single-server-launcher` at `4165fcb` (`fix: launch julie-server directly`). It launches `julie-server` directly and keeps only legacy split-daemon cleanup.

What is not done:

- Nothing has been pushed, tagged, published, or released yet.
- The plugin launcher branch still needs to be merged into the real `julie-plugin` release path before publishing v7.13.3.
- The latest Julie `full` gate was run before the dashboard registry fix (`3e75377f`, 48 buckets in 1478.0s). The dashboard fix has strong focused/dev-equivalent evidence, but a final pre-tag `full` run is still the cleanest release proof if time allows.
- Test economy is improved but not finished. Remaining broad-bucket targets include `tools-editing`, `tools-search-format-quality`, `tools-call-path`, and any handler-bound buckets that still force `changed` to fall back to `dev`.

## Constraints

- Preserve Julie's moat: semantic/hybrid search, graph-centrality reranking, token-budgeted `get_context`, 34-language breadth, CLI/plugin shipping path.
- Keep behavior working while deleting complexity. Deletion is only a win when caller-facing tool behavior and focused gates stay green.
- Do not reintroduce daemon/adapter process management. The only intended resident extra process is the embedding host.
- Keep broad verification available in `full` while making `dev` cheap enough for normal agent use.
- Keep MCP tool consolidation out of the active rescue path unless the user explicitly reopens that product decision.
- Do not push, tag, publish, or release without explicit user approval.

## Success Criteria

- v7.13.3 ships as a single-server rescue release: manual configs and the plugin launch `julie-server` directly, with no `julie-adapter` or `julie-daemon` runtime path.
- Dashboard launch and dashboard workspace visibility both work from in-process MCP sessions after restart.
- Default branch verification stays materially below the old 30-minute pain point; current evidence is `cargo xtask test changed` passing 28 buckets in 440.6s for the final dashboard-fix diff and earlier `dev`/`full` gates recorded in `docs/release-notes/v7.13.3.md`.
- Slow handler-bound buckets are split, relocated, or demoted to broader release gates with evidence over future slices.
- Stale daemon vocabulary is removed from current user-facing docs and gradually retired from internal names (`DaemonDatabase` -> registry role, `daemon` bucket -> registry/runtime role).
- Current MCP tool surface stays stable while the rescue focuses on test economics and runtime simplification.

## References

- `docs/release-notes/v7.13.3.md` — current v7.13.3 user-facing release notes and verification evidence.
- `README.md` — manual config guidance now states `julie-adapter`/`julie-daemon` are gone and users must point configs at `julie-server`.
- `docs/plans/2026-06-06-julie-rescue-current-status.md` — rescue status source of truth; update if further rescue slices change the remaining-work list.
- `docs/plans/2026-06-06-julie-test-economy-plan.md` — active test-economy plan for the fast `dev` tier and slow bucket splits.
- `docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md` — daemon/adapter teardown plan and verification ledger.
- `docs/plans/2026-06-03-julie-rescue-design.md` — original strategy and rationale.
