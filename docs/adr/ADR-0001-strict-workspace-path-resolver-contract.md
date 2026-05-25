# ADR-0001: Strict workspace path resolver contract

## Context

`resolve_workspace_file_input` is the single entry point for "the user gave us a file path; turn it into something we can use." It is called by `get_symbols` (primary and target), `rename_symbol` scope normalization, and any new tool that takes a file path.

Pre-cleanup, the resolver returned a struct whose `relative_query_path` field was itself a `Result<String>`, and the three callers `or_else`-ed the failure into raw string normalization (e.g. `file_path.replace('\\', "/")`). The fallback hid two distinct conditions inside one shape:

- the path simply doesn't exist on disk yet (legitimate — workspace may not have been indexed)
- the path resolves to a real file outside the workspace root (almost always a caller bug)

A real file outside the workspace would flow through, produce empty results, and look like "no symbols found" — never as the input error it actually was. The Candidate 5 plan goal had been "no raw input fallback," but the rollout left the fallback in place.

## Decision

`resolve_workspace_file_input` returns `Result<WorkspaceFileInputResolution>`. When the resolved absolute path is not contained in the workspace root, it returns `Err` wrapping `WorkspaceResolutionFailure { kind: WorkspaceResolutionFailureKind::FileOutsideWorkspace, message: "file path is outside the workspace: <input>" }`. Callers propagate via `?`. The MCP-boundary classifier (see ADR-0002) maps this to `McpError::invalid_params`.

Inside-workspace paths that simply don't exist on disk continue to return `Ok(...)` with `existed = false`. That is a valid index-time condition, not a caller error.

The one intentional exception is `symbols/target_workspace.rs`: when the **target workspace root lookup itself** fails (the workspace id is valid but its root can't be loaded), the call falls back to treating the input as the literal path because the boundary it would enforce is unknown. That branch is documented inline.

## Consequences

**Easier**

- Callers stop owning path invariants. `?` is the only error-handling shape they need.
- A real file outside the workspace is now indistinguishable, at the boundary, from any other invalid input — it surfaces as `invalid_params`.
- New tools that take a file path get the contract for free by calling `resolve_workspace_file_input`.

**Harder**

- A user who passes an absolute path pointing at another repo gets a hard error instead of an empty result. This is the correct behavior, but it is a behavior change.
- The `target_workspace` exception is now load-bearing. If the workspace root lookup ever moves under a typed error itself, that exception can collapse into the normal `?` flow.

## Applies To

- `src/utils/paths.rs::resolve_workspace_file_input`
- `src/tools/navigation/resolution.rs::WorkspaceResolutionFailureKind::FileOutsideWorkspace`
- `src/tools/symbols/primary.rs`
- `src/tools/symbols/target_workspace.rs`
- `src/tools/refactoring/rename.rs::normalize_scope_file_path`
- Any new tool that takes a file path from the user

## Future Agents

- Do not add `or_else` / `unwrap_or` fallbacks at call sites of `resolve_workspace_file_input`. If you find yourself wanting one, the resolver contract is the place to extend, not the call site.
- When adding a new tool that takes a file path, route it through `resolve_workspace_file_input` and let the error bubble. The MCP-boundary classifier already maps `FileOutsideWorkspace` to `invalid_params`.
- Do not catch `WorkspaceResolutionFailure` inside tool implementations to give a "nicer" message. The classifier is the only place where that mapping lives.
- If you need a "path may or may not exist yet" semantics (e.g. for an editing tool that creates files), use the `existed` field on the success branch — do **not** weaken the outside-workspace check.
