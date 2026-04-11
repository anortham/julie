---
id: global-workspace-targeting-implementation
title: Global Workspace Targeting Implementation
status: completed
created: 2026-04-11T01:06:40.364Z
updated: 2026-04-11T11:47:28.725Z
tags:
  - workspace-targeting
  - daemon
  - planning
---

# Global Workspace Targeting Implementation Plan

Goal: implement explicit workspace activation for daemon sessions, with freshness-gated open semantics and stable post-activation routing by workspace_id.

## Summary
- Add session-scoped active workspace tracking to `JulieServerHandler`, backed by `WorkspacePool` so non-current workspaces can be activated and detached per session.
- Introduce `manage_workspace(operation="open", path|workspace_id)` as the single front door for resolve/index-refresh/activate.
- Require daemon-mode tool routing to target active workspaces only, while keeping existing tool routing semantics on canonical `workspace_id`.
- Demote legacy reference relationships to optional pairing metadata, remove pair-driven auto-attach, fix global index removal path assumptions, and update list/stats output to report known workspaces.
- Update `docs/WORKSPACE_ARCHITECTURE.md` and `JULIE_AGENT_INSTRUCTIONS.md`, then verify with `cargo xtask test dev` and `cargo xtask test system`.

## Planned Task Order
1. Handler active-workspace set and teardown over all active IDs.
2. New `manage_workspace open` command and tests.
3. Activation-gated daemon routing in `resolve_workspace_filter`.
4. Pairing metadata cleanup, list/stats semantics, and remove-path fix.
5. Docs refresh and branch-level verification.

## Implementation Notes
- The detailed execution plan is saved in `docs/superpowers/plans/2026-04-10-global-workspace-targeting.md`.
- The plan assumes `handle_ipc_session` is updated to pass an `Arc<WorkspacePool>` into `JulieServerHandler::new_with_shared_workspace` so session activation can reuse the shared pool.
- The explicit open flow should reuse existing index/refresh machinery where possible, rather than duplicating indexing logic.
- Daemon-mode routing should fail with a helpful activation error when a workspace is known but not active for the current session.

