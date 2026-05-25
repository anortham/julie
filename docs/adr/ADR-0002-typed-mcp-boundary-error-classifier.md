# ADR-0002: Typed MCP-boundary error classifier

## Context

The MCP boundary is the only place where an internal `anyhow::Error` becomes a wire-level `McpError`, and where the choice between `invalid_params` (4xx-equivalent) and `internal_error` (5xx-equivalent) is made. Pre-cleanup, this choice was made differently in every tool wrapper:

- `deep_dive.rs` had a one-off `Self::is_workspace_parameter_error(&message)` method on `JulieServerHandler` that did substring matching on the error string and returned `invalid_params` if it matched.
- `fast_refs.rs`, `fast_search.rs`, `get_context.rs`, `get_symbols.rs`, `blast_radius.rs`, `call_path.rs`, `rename_symbol.rs` all returned `McpError::internal_error(message, None)` unconditionally.

The result: a bad `workspace_id` on `fast_refs` produced a 500-equivalent; on `deep_dive` it produced a 400-equivalent; both came from the same internal `WorkspaceResolutionFailure`. The classification was a property of which wrapper happened to have been updated last, not a property of the error.

Substring matching also coupled the wire-level contract to message wording — any change to the error text could silently re-classify the response.

## Decision

A single function, `classify_tool_failure(tool_name: &str, err: &anyhow::Error) -> McpError`, lives at `src/handler/tools/error.rs`. It is the only place where the `invalid_params` vs `internal_error` decision is made. Every tool wrapper calls it on its error path.

The classifier does a **typed downcast** to `WorkspaceResolutionFailure` via `crate::tools::navigation::resolution::workspace_resolution_failure_kind(&err)`. If the downcast succeeds, the result is `invalid_params` with a `"<tool_name> failed: <message>"` body. Otherwise it is `internal_error` with the same body shape.

`JulieServerHandler::is_workspace_parameter_error` has been removed. There is no substring matching anywhere on the boundary.

## Consequences

**Easier**

- Adding a new tool wrapper: copy the same `Err(e) => classify_tool_failure("<name>", &e)` shape. No decision to re-litigate.
- Extending the typed error: add a variant to `WorkspaceResolutionFailureKind`, decide once whether it should classify as `invalid_params` or `internal_error`, and every tool inherits the new behavior.
- Tests that assert `invalid_params` vs `internal_error` semantics no longer depend on which tool they happen to be calling.

**Harder**

- Any new failure mode that should be `invalid_params` must be expressed as a typed error variant — usually a new `WorkspaceResolutionFailureKind` — not as a substring in the message. This is the intended cost.
- Tool implementations cannot produce custom `McpError` shapes for specific failures; they must produce a typed `anyhow::Error` and let the classifier decide. This keeps the boundary policy in one place.

## Applies To

- `src/handler/tools/error.rs::classify_tool_failure`
- `src/handler/tools/{deep_dive,fast_refs,fast_search,get_context,get_symbols,blast_radius,call_path,rename_symbol}.rs`
- `src/tools/navigation/resolution.rs::{WorkspaceResolutionFailure, WorkspaceResolutionFailureKind, workspace_resolution_failure_kind}`

## Future Agents

- Do not return `McpError::internal_error(...)` or `McpError::invalid_params(...)` directly from a tool wrapper's `Err(e)` arm. Call `classify_tool_failure(<tool_name>, &e)`.
- Do not reintroduce substring matching on error messages to drive classification. If you find a case where the classifier picks the wrong category, add a typed variant; do not pattern-match strings.
- Do not bury the classifier inside individual tool implementations. The boundary policy belongs at the MCP edge, not inside tool logic — tool logic produces typed errors and lets the wrapper translate.
- When adding a new typed error kind, decide its classification up-front and add it to `classify_tool_failure` if the default (`internal_error`) is wrong. Most resolver failures should be `invalid_params`; transient infrastructure failures should be `internal_error`.
