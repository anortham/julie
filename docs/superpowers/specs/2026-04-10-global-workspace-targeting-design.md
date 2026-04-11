# Global Workspace Targeting Design

**Date:** 2026-04-10  
**Status:** Approved  
**Scope:** Daemon workspace activation, freshness gating, watcher lifecycle, and tool routing semantics

## Problem

Julie still treats cross-workspace access through a legacy reference-workspace relationship. That model couples routing to an old projectA -> projectB pairing assumption and creates inconsistent behavior across tools and session lifecycle paths.

The approved direction is global workspace targeting: any session can target any known workspace by path or workspace ID, then route by workspace_id after activation.

## Current Behavior and Mismatches

1. **Routing semantics are mixed.** Some paths assume reference workspace relationships, while other paths route through workspace_id.
2. **Legacy nested-layout mental model still leaks into code paths.** Stale assumptions remain around add/remove and detach flows, including likely removal-path bugs tied to the old nested layout mental model.
3. **Freshness is not enforced as a hard contract before all query and edit usage.** Editing tools depend on fresh index state, so best-effort freshness is unsafe.
4. **Watcher intent is blurred.** Existing behavior can drift toward broad known-workspace coverage instead of session-scoped active-workspace coverage.
5. **Session teardown scope is too narrow in some paths.** Teardown logic can focus on the primary workspace, while active non-primary workspaces also need detachment.

## Goals

1. Replace reference workspace as a routing concept with global workspace targeting.
2. Allow any session to target any workspace by path or workspace ID.
3. Ensure missing workspaces are indexed before use.
4. Enforce freshness checks before a workspace is query-safe or edit-safe.
5. Activate target workspaces into a session-level active-workspace set.
6. Watch active session workspaces in daemon mode, not every known workspace.
7. Keep tool routing semantics stable: route by workspace_id after activation.
8. Keep persistent pairings, favorites, recents, and named sets as optional convenience metadata only.

## Non-Goals

1. Do not convert every tool into a hidden path resolver that triggers indexing on arbitrary input paths.
2. Do not remove workspace_id-based tool routing after activation.
3. Do not redefine convenience metadata as routing authority.
4. Do not fix unrelated build warnings in this effort.

## Design

### Global Workspace Identity

- **Known workspace:** Indexed and registered in daemon metadata.
- **Active workspace:** Loaded into a given session working set and under watcher coverage for that session.
- **Target workspace:** The workspace selected for a tool call.
- **Current workspace:** The workspace bound to the current project context for the session.

The primary routing identity remains `workspace_id`. Path input is a front-door discovery and activation input, not a long-term routing key.

### Ensure/Open Workspace Flow

Introduce one explicit front-door operation for cross-workspace targeting:

1. Accept path or workspace ID as target input.
2. Resolve to a known workspace if it exists.
3. If missing, index and register it.
4. Run freshness check and catch-up indexing when stale.
5. Mark query-safe and edit-safe readiness only after freshness passes.
6. Activate workspace into the caller session active-workspace set.
7. Return canonical `workspace_id` for subsequent tool routing.

This central path prevents ad hoc per-tool path routing and prevents hidden indexing behavior spread across tool handlers.

### Freshness Contract for Query-Safe and Edit-Safe Use

A workspace is not ready for querying or editing until freshness has been checked.

- Query and edit operations require freshness-gated activation.
- If stale, run catch-up indexing before query/edit access.
- Editing paths inherit this gate as mandatory behavior due to index dependency.

This contract turns freshness into a correctness boundary, not a best-effort optimization.

### Session Active-Workspace Set

Each session owns an active-workspace set.

- Activation adds the workspace to that set.
- Tool calls target a workspace selected from that set, then route via `workspace_id`.
- Session teardown detaches all active workspaces, not only the primary workspace.

### Watcher Lifecycle in Daemon Mode

Watcher coverage follows session activity.

- Watch active session workspaces.
- Do not watch every known workspace.
- Attach watcher coverage on activation.
- Detach watcher coverage when a workspace is no longer active for any session.

This keeps watcher load bounded and aligned with live usage.

### Tool Contract: Keep Routing by workspace_id After Activation

No tool-level routing rewrite is needed after activation.

- Tools continue receiving and routing with `workspace_id`.
- Cross-workspace path or ID input is resolved in the explicit front-door operation.
- Tool handlers remain focused on tool behavior, not workspace discovery/index orchestration.

### Terminology Changes

Adopt the following vocabulary in user-facing docs and implementation comments:

- Use **current workspace**, **known workspace**, **active workspace**, and **target workspace**.
- Avoid presenting **reference workspace** as the primary concept.
- If legacy naming remains in APIs during migration, treat it as compatibility naming, not model semantics.

### Optional Convenience Metadata

Persistent pairings (old projectA -> projectB), favorites, recents, and named sets may remain as convenience metadata.

- These can improve UX for quick selection.
- They do not grant access.
- They do not define routing semantics.
- They do not replace activation and freshness checks.

## Migration Strategy

1. Add explicit ensure/open front-door activation path for target workspace by path or workspace ID.
2. Route cross-workspace entry points through that path.
3. Enforce freshness gate before query/edit readiness flags are set.
4. Introduce session active-workspace set usage across connect, call, and teardown flows.
5. Move watcher attach/detach decisions to active-workspace membership semantics.
6. Preserve legacy reference metadata as optional convenience data only.
7. Audit and remove stale reference-workspace assumptions, with focused checks on removal paths that still reflect old nested layout assumptions.

## Files Likely to Change in Implementation

The following files are likely implementation touchpoints when this design is executed:

- `src/handler.rs` (central activation boundary, freshness gating, and tool entry wiring)
- `src/daemon/mod.rs` (session connect and disconnect flow, activation of non-current workspaces, teardown of all active workspaces)
- `src/daemon/workspace_pool.rs` (active-workspace loading, session counts, and watcher attach and detach hooks)
- `src/daemon/watcher_pool.rs` (watcher lifecycle tied to active-workspace membership)
- `src/daemon/database.rs` (workspace registry plus optional convenience metadata storage)
- `src/tools/workspace/commands/index.rs` (index-if-missing and catch-up orchestration)
- `src/tools/workspace/commands/registry/add_remove.rs` (legacy metadata behavior and removal-path cleanup)
- `src/tools/navigation/resolution.rs` and workspace-targeting entry points in `src/tools/*` (route by workspace_id after activation)
- `docs/WORKSPACE_ARCHITECTURE.md` and related user docs (terminology and flow updates)

## Risks and Mitigations

1. **Risk: Hidden stale assumptions survive migration.**  
   **Mitigation:** Add a targeted audit for legacy reference-workspace logic, with priority on remove/detach flows that still mirror old nested layout assumptions.

2. **Risk: Watcher churn or leaks across session boundaries.**  
   **Mitigation:** Tie watcher lifecycle to reference-counted active-workspace membership across sessions and verify teardown for all active workspaces.

3. **Risk: Tool handlers bypass front-door activation path.**  
   **Mitigation:** Consolidate path/ID resolution in one ensure/open function and forbid direct per-tool path-triggered indexing.

4. **Risk: Freshness checks become optional in edge paths.**  
   **Mitigation:** Enforce freshness-gated query-safe/edit-safe preconditions at the centralized activation boundary.

## Summary Recommendation

Proceed with global workspace targeting as the primary model.

- Use one explicit ensure/open activation front door for path or workspace ID input.
- Enforce freshness before query/edit readiness.
- Route tools by `workspace_id` after activation.
- Drive watcher behavior from session active-workspace membership.
- Treat legacy pairings and related metadata as optional convenience data, not routing semantics.
- Prioritize cleanup of stale reference-workspace assumptions, with focused attention on removal-path behavior inherited from the old nested layout mental model.
