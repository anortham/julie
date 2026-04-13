---
id: workspace-startup-roots-2026-04-11
title: Workspace Startup and Roots
status: active
created: 2026-04-12T02:46:10.307Z
updated: 2026-04-12T02:46:10.307Z
tags:
  - planning
  - startup
  - roots
  - mcp
  - workspace
---

# Workspace Startup and Roots Implementation Plan

Source of truth: `docs/superpowers/plans/2026-04-11-workspace-startup-roots.md`

## Goal
Refactor Julie startup and session binding so explicit `--workspace` and `JULIE_WORKSPACE` remain authoritative, GUI `cwd` becomes a weak hint, and MCP roots can bind the primary workspace plus activate secondary roots.

## Architecture
- Preserve `WorkspaceStartupHint { path, source }` from CLI through adapter and daemon IPC.
- Allow daemon sessions to start unbound when the source is `cwd`.
- Move binding policy into handler-owned `SessionWorkspaceState`.
- Resolve the primary workspace at request time via `roots/list` when the client advertises roots.
- Mark the session dirty on `notifications/roots/list_changed` and refresh on the next request boundary.
- Keep default tool semantics stable: omitted workspace params still mean primary only.

## Task Breakdown
1. Add shared startup hint types and adapter header serialization.
2. Extract daemon IPC-session parsing and carry startup hints through session bootstrap.
3. Add handler-owned session workspace state and mutable primary binding helpers.
4. Implement request-time primary resolution, defer auto-indexing until the primary is resolved, and wire primary-scoped tool wrappers through the resolver.
5. Handle `roots/list_changed`, keep secondary roots active, and update workspace architecture docs.
6. Run focused startup/roots tests plus `cargo xtask test dev` and `cargo xtask test system`.

## Key Files
- `src/workspace/startup_hint.rs`
- `src/cli.rs`
- `src/main.rs`
- `src/adapter/mod.rs`
- `src/daemon/ipc_session.rs`
- `src/daemon/mod.rs`
- `src/handler/session_workspace.rs`
- `src/handler.rs`
- `src/startup.rs`
- `src/tools/workspace/commands/index.rs`
- `src/tests/adapter/handshake.rs`
- `src/tests/daemon/ipc_session.rs`
- `src/tests/daemon/session_workspace.rs`
- `src/tests/daemon/roots.rs`
- `docs/WORKSPACE_ARCHITECTURE.md`

## Notes
- The detailed, step-by-step implementation plan with concrete code snippets and test commands lives in the markdown plan file.
- No git commit has been created for the plan document in this session.

