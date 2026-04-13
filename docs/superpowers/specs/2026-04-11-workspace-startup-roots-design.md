# Workspace Startup and Roots Design

**Date:** 2026-04-11  
**Status:** Approved  
**Scope:** CLI startup hints, adapter preamble, daemon session binding, MCP roots support, and multi-root session behavior

## Problem

Julie still decides the session workspace too early.

Today, `src/main.rs` and `src/cli.rs` resolve one startup path from `--workspace`, `JULIE_WORKSPACE`, or process `cwd`. The adapter then sends only `WORKSPACE:<path>` to the daemon. By the time the daemon receives the session, it cannot tell whether that path was an explicit override or a weak guess inherited from a GUI process with a bad `cwd`.

That eager binding model breaks two things:

1. GUI MCP clients can hand Julie an incorrect startup `cwd`, including `/`.
2. Julie binds the workspace before MCP initialization completes, so it cannot use client-advertised roots when that decision is made.

The result is a startup architecture that worked for CLI-driven sessions but is wrong for broader MCP client support.

## Current Behavior and Mismatches

1. **Workspace intent is collapsed too early.** Adapter IPC preserves a path but not the source of that path.
2. **The daemon binds primary workspace state before MCP lifecycle context exists.** Client capabilities and roots are unavailable at the point of binding.
3. **`cwd` is treated as authoritative when it should be a fallback hint.** Explicit overrides and client roots need stronger priority.
4. **Multi-root workspaces have no session model.** There is no policy for primary root selection, secondary root activation, or root changes during a session.
5. **Current default tool behavior should remain stable.** Adding multi-root support must not silently widen default searches or change what `workspace="primary"` means.

## Goals

1. Preserve the source of startup workspace information as session metadata.
2. Keep explicit overrides authoritative.
3. Support MCP roots for clients that advertise them.
4. Support full multi-root sessions, including root changes during an active session.
5. Resolve workspace binding at request time when startup information is weak.
6. Keep default tool semantics stable: no hidden widening from one primary workspace to all roots.
7. Reuse existing workspace activation and routing concepts where they already fit.

## Non-Goals

1. Do not rewrite every tool around new multi-root semantics.
2. Do not change explicit override behavior for `--workspace` or `JULIE_WORKSPACE`.
3. Do not make secondary roots part of implicit default search scope.
4. Do not push client-specific hacks into core policy for VS Code, Codex, or any other one client.
5. Do not move workspace loading policy into `workspace_pool`; it remains a loader and cache layer.

## Design

### Startup Hint Instead of Early Authority

Replace the current startup path handoff with a typed startup hint.

The startup hint contains:

- `path: PathBuf`
- `source: Cli | Env | Cwd`

The adapter IPC preamble changes from:

- `VERSION:<n>`
- `WORKSPACE:<path>`

to:

- `VERSION:<n>`
- `WORKSPACE:<path>`
- `WORKSPACE_SOURCE:<cli|env|cwd>`

This preserves intent. A path from `cli` or `env` is an explicit user choice. A path from `cwd` is a startup hint only.

### Precedence Rules

Workspace authority follows this order:

1. `--workspace`
2. `JULIE_WORKSPACE`
3. MCP roots
4. process `cwd`

This keeps manual overrides authoritative and allows roots to replace bad GUI `cwd` values without breaking CLI workflows.

### Session Workspace State

Each session owns a resolver state object rather than assuming a bound primary workspace exists from startup.

The state tracks:

- `startup_hint`
- `client_supports_roots: bool`
- `roots_dirty: bool`
- `last_roots: Vec<PathBuf>`
- `primary_workspace_id: Option<String>`
- `primary_workspace_root: Option<PathBuf>`
- `secondary_workspace_ids: HashSet<String>`

This state belongs in the handler, because the handler has both MCP request context and workspace activation access.

### Request-Time Resolution

Julie should treat primary workspace binding as request-bound when startup source is weak.

Behavior:

1. During `initialize`, record whether the client advertises roots support.
2. Do not call `roots/list` from `on_initialized`.
3. Before any tool path that needs the primary workspace, run a resolver step.
4. If startup source is `cli` or `env`, bind that path as primary and stop.
5. If startup source is `cwd`, prefer cached roots state when available.
6. If roots are supported and the cache is missing or dirty, call `context.peer.list_roots()` inside the active client request.
7. If roots are returned, select the first root as primary.
8. If roots are unavailable, empty, or request-time roots lookup fails, fall back to startup `cwd`.

This model fixes the current failure mode without relying on independent post-startup server requests.

### Multi-Root Policy

Julie needs an explicit policy, not an implied one.

- **Primary root:** the first root returned by the latest `roots/list` snapshot.
- **Secondary roots:** all remaining roots in the snapshot.
- **Session activation:** secondary roots are activated as session-visible workspaces using the existing active-workspace model.
- **Default tool behavior:** `workspace="primary"` and omitted workspace params still mean the primary workspace only.
- **Explicit targeting:** secondary roots are reachable through existing workspace-targeted tool flows.

This preserves predictable default behavior while still making all roots available to tools that opt into explicit workspace targeting.

### Root Change Handling

`notifications/roots/list_changed` marks session state dirty.

Julie does not rebind in the middle of a tool call.

On the next request boundary:

1. Refresh roots with `roots/list`.
2. Update `last_roots`.
3. Recompute primary and secondary root sets.
4. If primary changed, switch the session primary binding.
5. Activate any new secondary roots.
6. Remove no-longer-present secondary roots from the session active set.

Per-request work continues against the workspace snapshot it started with. Rebinding happens only between requests.

### Stable Default Tool Semantics

Multi-root support does not mean hidden fan-out.

- Omitted workspace parameters remain scoped to the current primary workspace.
- Existing workspace-aware tools keep their current explicit targeting model.
- Secondary roots become active session workspaces, not implicit query scope.

This avoids surprising search inflation and keeps current user expectations intact.

### Refactor Boundaries

The refactor is contained and should not become a broad rewrite.

#### CLI and Adapter

- `src/cli.rs` returns a typed startup hint instead of a bare path.
- `src/main.rs` passes that startup hint through startup.
- `src/adapter/mod.rs` includes `WORKSPACE_SOURCE` in the daemon preamble.

#### Daemon

- `src/daemon/mod.rs` parses the startup hint and stops forcing eager primary binding when source is `cwd`.
- `src/daemon/workspace_pool.rs` remains the shared loader and cache. It does not become a policy engine.

#### Handler

- `src/handler.rs` owns the session resolver state.
- Add one central request-time resolver path that ensures the primary workspace is bound before primary-scoped tools run.
- Reuse existing workspace activation helpers for secondary roots.

This boundary is the right split:

- CLI decides startup source.
- Adapter preserves it.
- Daemon carries it.
- Handler applies session policy.
- Workspace pool loads and caches workspaces.

## Migration Strategy

1. Introduce `WorkspaceStartupHint` in CLI startup resolution.
2. Extend the adapter-to-daemon preamble with `WORKSPACE_SOURCE`.
3. Update daemon session creation to accept an unbound primary workspace when startup source is `cwd`.
4. Add session resolver state to `JulieServerHandler`.
5. Record client roots capability during MCP initialization.
6. Add a central request-time resolver for primary workspace access.
7. Route primary-scoped tool entry points through that resolver.
8. Add root snapshot refresh logic plus `roots_dirty` handling for `roots/list_changed`.
9. Activate secondary roots into the existing session active-workspace set.
10. Update workspace and lifecycle docs to match the new model.

## Files Likely to Change in Implementation

- `src/cli.rs` (typed startup hint and source tracking)
- `src/main.rs` (startup hint handoff)
- `src/adapter/mod.rs` (IPC preamble changes)
- `src/daemon/mod.rs` (session setup and delayed primary binding)
- `src/handler.rs` (session resolver state, roots capability tracking, request-time binding)
- `src/daemon/workspace_pool.rs` (loader usage only, no policy shift)
- primary-scoped tool entry points in `src/tools/*` that currently assume a bound primary workspace
- `docs/WORKSPACE_ARCHITECTURE.md` and related docs (startup and roots behavior)

## Risks and Mitigations

1. **Risk: Request-time binding leaks into many tool paths.**  
   **Mitigation:** Add one central resolver function and route primary workspace access through it instead of sprinkling checks across tools.

2. **Risk: Multi-root support changes default search semantics by accident.**  
   **Mitigation:** Preserve `primary` as the implicit default and require explicit workspace targeting for non-primary roots.

3. **Risk: Session state becomes inconsistent across root changes.**  
   **Mitigation:** Refresh only at request boundaries and treat each tool call as operating on a stable snapshot.

4. **Risk: Client support is uneven across MCP hosts.**  
   **Mitigation:** Treat roots as capability-driven. Clients that do not advertise roots still work through explicit overrides or `cwd` fallback.

5. **Risk: Policy drifts into cache or loader layers.**  
   **Mitigation:** Keep binding policy in the handler and keep `workspace_pool` focused on loading and sharing workspace state.

## Summary Recommendation

Proceed with a focused startup and session-binding refactor, not an isolated roots patch.

- Preserve startup source metadata.
- Keep explicit overrides authoritative.
- Resolve primary workspace at request time when startup information is weak.
- Use MCP roots when the client advertises them.
- Support multi-root sessions through one primary workspace plus explicit secondary workspace activation.
- Handle root changes at request boundaries.
- Preserve current default tool semantics.

This gives Julie a startup model that works for CLI tools, GUI MCP clients, and future wider adoption without treating bad `cwd` as authoritative.
