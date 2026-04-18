# Agent Tool Surface Design

**Date:** 2026-04-17
**Status:** Design, ready for implementation planning

## Goal

Tighten Julie's MCP tool surface around the coding loop that agents use most, remove low-value admin distraction from that surface, replace the weakest symbolic edit primitive, and add a narrow path-finding tool that answers a common debugging question without bloating token spend.

## Background

Recent dashboard metrics show a clear usage pattern:

- `fast_search`: 1474 calls
- `get_symbols`: 1347 calls
- `deep_dive`: 1028 calls
- `edit_file`: 380 calls
- `get_context`: 284 calls
- `fast_refs`: 168 calls
- `manage_workspace`: 130 calls
- `edit_symbol`: 16 calls
- `query_metrics`: 11 calls

The top four tools account for most agent activity. The default coding loop is clear:

1. find code with `fast_search`
2. inspect file structure with `get_symbols`
3. understand a symbol with `deep_dive`
4. patch the file with `edit_file`

This design follows that signal instead of fighting it.

The discussion that led to this design also produced a guardrail that matters more than any single tool: Julie's value proposition is token savings. New tool work must reduce total token spend across a real workflow, not add more knobs, more modes, or more verbose output.

## Design Rules

1. **One tool, one job.** Each tool should answer one question or perform one mutation.
2. **Tight defaults.** Default output should cover the common case. Richer output must be opt-in.
3. **Collapse common chains only.** A new tool earns its slot when it replaces a common multi-call workflow with less total token spend.
4. **Hints beat orchestration.** Tools may suggest the next call. They should not absorb adjacent jobs into a kitchen-sink response.
5. **Admin surfaces stay out of the core loop.** Dashboard and daemon administration can exist without sitting in the day-to-day MCP belt.

## Current Decisions

### 1. Leave `manage_workspace` alone

`manage_workspace` is useful enough and already gets the job done. This pass does not change its behavior, schema, or routing.

The MCP tool list is flat, so there is no literal "hide this tool" mechanism. The positioning change is in documentation and guidance only:

- keep `manage_workspace` available
- stop featuring it as part of the default single-workspace coding loop
- present it as a cross-workspace or daemon administration tool when relevant

No implementation changes are planned for `manage_workspace` in this pass.

### 2. Remove `query_metrics` from the MCP surface

`query_metrics` is low-usage and duplicates data that the dashboard already reads directly from storage. The dashboard metrics route queries the daemon database without going through the MCP tool surface.

Decision:

- remove `query_metrics` from the public MCP tool list
- keep the metrics storage pipeline and dashboard data sources
- cover any lost visibility with dashboard improvements instead of another MCP admin tool

This trims prompt clutter, tool-list noise, and token cost for agents that do not need operational telemetry during ordinary coding.

### 3. Replace `edit_symbol` with `rewrite_symbol`

`edit_symbol` is better than its usage count first suggests. It has strict symbol resolution, file freshness checks, dry-run previews, disambiguation, and atomic writes. The weak point is its edit engine. It still operates on indexed line bounds with three coarse operations:

- `replace`
- `insert_after`
- `insert_before`

That shell is sound, but the blade is blunt. Agents still trust `edit_file` more often because text replacement feels more predictable than line-range symbol surgery.

Decision:

- replace `edit_symbol` with a new `rewrite_symbol` tool
- keep the narrow, symbol-scoped workflow
- swap the edit engine from line-range surgery to AST-backed live-file rewriting

#### `rewrite_symbol` principles

- Resolve the symbol first.
- Reparse the current file, do not trust stale indexed line spans as the primary edit anchor.
- Rewrite the live syntax node or a node-derived span.
- Keep `dry_run=true` as the default.
- Emit a compact unified diff preview.
- Do not bundle references, path tracing, or batch edits into this tool.

#### `rewrite_symbol` operations for v1

- `replace_full`
- `replace_body`
- `replace_signature`
- `insert_before`
- `insert_after`
- `add_doc`

This is intentionally small. It covers the common symbolic edit cases without drifting into a refactoring framework.

#### `rewrite_symbol` request shape

```json
{
  "symbol": "AuthService::validate",
  "operation": "replace_body",
  "content": "fn validate(&self, token: &str) -> Result<User> { ... }",
  "file_path": "src/auth/service.rs",
  "workspace": "primary",
  "dry_run": true
}
```

Notes:

- `file_path` remains optional for disambiguation.
- `workspace` is optional and follows Julie's multi-workspace conventions.
- `content` stays explicit. The tool performs the edit, not code generation.

#### `rewrite_symbol` output rules

- Dry run returns a compact diff preview with limited context.
- Apply returns a short success summary plus the same compact diff.
- Error output stays narrow and specific: symbol not found, ambiguous match, parse failure, unsupported operation for symbol kind, stale index fallback failure.

#### Why not diff-match-patch as the main engine

`diff-match-patch` remains the right engine for `edit_file`, where the unit of work is arbitrary text. It should not be the main authority for `rewrite_symbol`.

For symbol rewriting, the primary flow should be:

1. resolve symbol
2. parse live file
3. locate live syntax node
4. rewrite node-derived span

If node recovery needs a soft anchor after minor drift, DMP can serve as a bounded fallback. It should not be the core edit model.

### 4. Add `call_path`

Julie had broader tracing work in the past, but the old public tool was cut after low adoption. This design does not revive that broad tracing surface. It introduces a narrower, cheaper tool for a common coding question:

- how does A reach B
- is there any path from this handler to that side effect
- am I chasing a real dependency chain or noise

Decision:

- add `call_path(from, to, max_hops=6)` as a shortest-path tool
- do not add outward tracing in v1
- do not fold path tracing into `deep_dive`

#### Why shortest path first

Shortest path answers a bounded question with bounded output. That makes it a good fit for Julie's token-saving goal.

Outward tracing has weaker boundaries:

- it grows fast
- it overlaps with `deep_dive` and `fast_refs`
- it tends to answer one question with many pages of output

That mode can be revisited later if real usage proves a need. It does not belong in v1.

#### `call_path` request shape

```json
{
  "from": "LoginButton::onClick",
  "to": "insert_session",
  "max_hops": 6,
  "workspace": "primary"
}
```

#### `call_path` response shape

```json
{
  "found": true,
  "hops": 4,
  "path": [
    {
      "from": "LoginButton::onClick",
      "to": "AuthService::login",
      "edge": "call",
      "file": "src/components/LoginButton.tsx:25"
    },
    {
      "from": "AuthService::login",
      "to": "SessionStore::create",
      "edge": "call",
      "file": "src/services/auth_service.ts:48"
    },
    {
      "from": "SessionStore::create",
      "to": "insert_session",
      "edge": "call",
      "file": "src/storage/session_store.rs:72"
    }
  ]
}
```

Output rules:

- one shortest path only in v1
- compact hop list, not a tree
- edge labels stay terse: `call`, `construct`, `reference`, `dispatch`
- if no path is found, return `found: false` and a short diagnostic, not a giant near-miss dump

### 5. Improve the dashboard to absorb `query_metrics` removal

Removing `query_metrics` from the MCP surface means the dashboard must carry the admin and observability story cleanly.

This pass should improve the dashboard, not add another agent-facing telemetry tool.

Expected dashboard follow-through:

- workspace filter plus all-workspaces aggregate view
- latency trends, not only point-in-time averages
- success-rate and context-efficiency panels
- if `doc_coverage` and `dead_code` remain useful, surface them in a dashboard admin area or another non-MCP admin surface

The design does not require code-health metrics to stay public in the agent tool surface.

### 6. Tune lower-use tools without bloating them

Lower-use tools still matter. The fix is not more modes. The fix is better output shape and better workflow placement.

#### `fast_refs`

Problem:

- overshadowed by `deep_dive`
- flat location output is less helpful for impact checks

Follow-up direction:

- summary-first output
- group by file and reference kind
- call out likely test files when possible
- offer expanded locations only when requested

#### `get_context`

Problem:

- broad output can feel expensive when the task is already symbol-specific

Follow-up direction:

- keep it as an orientation tool
- tighten default neighbor budget
- add a short "next useful calls" footer based on pivots

#### `rename_symbol`

Problem:

- agents avoid wide renames when the preview feels risky or vague

Follow-up direction:

- lead dry-run output with a safety summary
- include file count, reference count, skipped files, parse failures
- keep full diff details available after the summary

These are follow-up improvements, not blockers for the primary tool-surface work.

## Planned Files

Primary implementation work is likely to touch:

- `src/handler.rs`
- `src/tools/metrics/mod.rs`
- `src/tests/tools/metrics/`
- `src/tools/editing/edit_symbol.rs` or a new `src/tools/editing/rewrite_symbol.rs`
- `src/tests/tools/editing/`
- `src/tools/trace_call_path/` or a new `src/tools/call_path/`
- `src/tests/tools/` for new `call_path` coverage
- `src/dashboard/routes/metrics.rs`
- dashboard templates or frontend assets that render metrics
- dashboard tests
- `JULIE_AGENT_INSTRUCTIONS.md`
- `docs/site/index.html`
- `docs/site/script.js`

Implementation planning may refine this list once the exact tool-registration and dashboard paths are confirmed.

## Constraints

- Follow TDD for each implementation step.
- Use the narrowest failing test during red and green.
- Do not turn any tool into a multi-step orchestrator.
- Keep default output compact.
- Do not change `manage_workspace` behavior in this pass.
- Do not ship outward tracing in v1.
- Do not make DMP the primary engine for `rewrite_symbol`.

## Out of Scope

- `manage_workspace` redesign
- a batch `workspace_edit` tool
- outward or tree-shaped tracing
- merging path tracing into `deep_dive`
- keeping `query_metrics` as a hidden MCP tool for ordinary agents
- broad dashboard redesign outside the metrics and admin views needed to cover `query_metrics` removal

## Acceptance Criteria

- [ ] Design doc committed to `docs/plans/2026-04-17-agent-tool-surface-design.md`.
- [ ] `query_metrics` is removed from the public MCP tool list.
- [ ] Dashboard metrics views cover the operational visibility that ordinary users lose when `query_metrics` goes away.
- [ ] `manage_workspace` behavior is unchanged.
- [ ] `rewrite_symbol` exists as the public symbol-edit tool with `dry_run=true` by default.
- [ ] `rewrite_symbol` supports `replace_full`, `replace_body`, `replace_signature`, `insert_before`, `insert_after`, and `add_doc`.
- [ ] `rewrite_symbol` reparses the live file and does not rely on stored line ranges as its primary edit anchor.
- [ ] `rewrite_symbol` returns compact diff previews and compact apply results.
- [ ] `call_path(from, to, max_hops=6)` exists as a bounded shortest-path tool.
- [ ] `call_path` returns one compact path or a short no-path result, not an outward trace dump.
- [ ] Session-start guidance, docs, and site examples reflect the updated tool surface.
- [ ] Any follow-up tuning for `fast_refs`, `get_context`, and `rename_symbol` stays scoped to output-shape and guidance improvements, not feature sprawl.

## Risks

- **AST rewrite edge cases across 34 languages.** `rewrite_symbol` must degrade cleanly when a language parser cannot support a requested operation on a given symbol kind.
- **Path quality depends on relationship coverage.** `call_path` will only be as strong as the indexed call and reference graph for each language.
- **Dashboard scope creep.** Replacing `query_metrics` with dashboard improvements must not turn into a generic analytics rewrite.
- **Docs drift.** If docs and session-start guidance are not updated in the same pass, agents will keep reaching for removed or replaced tools.

## Deferred Questions

- If `doc_coverage` and `dead_code` stay useful, should they live in a dashboard admin view, a CLI, or another internal surface?
- After `call_path` ships, does real usage justify an outward trace tool, or does `deep_dive` plus `fast_refs` already cover that need?
- After `rewrite_symbol` ships, should `edit_file` remain the default write primitive in guidance, or should symbolic rewrites become the preferred route when a symbol target is known?
