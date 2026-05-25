# Architecture Decision Records

This directory holds short ADR-style notes for load-bearing decisions about Julie's module boundaries, contracts, and seams. ADRs are written when:

- A refactor candidate that changes module/interface shape is accepted.
- A candidate is rejected for a load-bearing reason that future agents should not rediscover.
- A new seam or adapter is established.
- A repeated review finding surfaces a rule that should be encoded.

Skip ADRs for minor cleanup or for changes that Gate Mode (see `razorback:architecture-quality`) classifies as having no architecture impact.

## Index

| # | Title | Scope |
|---|---|---|
| [0001](ADR-0001-strict-workspace-path-resolver-contract.md) | Strict workspace path resolver contract | `resolve_workspace_file_input` rejects outside-workspace input with `FileOutsideWorkspace`; no raw-input fallback |
| [0002](ADR-0002-typed-mcp-boundary-error-classifier.md) | Typed MCP-boundary error classifier | `classify_tool_failure` is the single `invalid_params` vs `internal_error` decision point |
| [0003](ADR-0003-prepared-once-edit-rewrite-invariant.md) | Prepared-once edit/rewrite invariant | `PreparedEdit` / `PreparedRewrite` constructed once per request, consumed by metrics and apply |
| [0004](ADR-0004-per-path-edit-lock-invariant.md) | Per-path edit lock invariant | All in-process writers serialize through `EDIT_LOCKS` keyed on normalized paths |
| [0005](ADR-0005-source-aware-language-detection-precedence.md) | Source-aware language detection precedence | `detect_language_for_source` at indexing time; stored DB row authoritative for resolver/scoring |

## Format

```markdown
# ADR-NNNN: [Decision]

## Context
What structural problem or repeated finding led here.

## Decision
What we will do, or what we are rejecting.

## Consequences
What this makes easier, what it makes harder.

## Applies To
Files/modules/patterns affected.

## Future Agents
What agents should do or avoid when touching this area.
```
