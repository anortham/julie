---
name: recall
description: Restore context from Julie developer memory — use when starting a new session, after context loss, searching for past work, or when the user asks what happened previously, wants to find old decisions, or needs cross-project context
allowed-tools: mcp__julie__recall, mcp__julie__plan
---

# Recall — Restore Developer Memory

## When to Use

Call `mcp__julie__recall` to restore context from previous sessions. Recall runs automatically at session start (via the SessionStart hook), but users can also invoke `/recall` for targeted queries.

Default parameters (last 5 checkpoints, no date window) cover most cases.

## Common Scenarios

- **New session, need prior context** — `mcp__julie__recall()` with defaults
- **After context compaction** — recall to restore lost state
- **Searching for past work** — `mcp__julie__recall({ search: "auth refactor", full: true })`
- **Recent activity window** — `mcp__julie__recall({ since: "2h" })` or `mcp__julie__recall({ days: 3 })`
- **Date range query** — `mcp__julie__recall({ from: "2026-03-01", to: "2026-03-05" })`
- **Cross-project standup** — `mcp__julie__recall({ workspace: "all", days: 1 })` (daemon mode only)
- **Just need the plan** — `mcp__julie__recall({ limit: 0 })`
- **Filter by plan** — `mcp__julie__recall({ planId: "plan-abc123" })`

## Parameter Reference

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | 5 | Max checkpoints to return. Set to 0 for active plan only |
| `search` | string | null | BM25 full-text search over memories (not fuzzy — real ranked search) |
| `full` | boolean | false | Return full descriptions + git metadata |
| `days` | integer | null | Look back N days |
| `since` | string | null | Time filter: "2h", "30m", "3d", "1w", or ISO timestamp |
| `from` | string | null | Date range start (YYYY-MM-DD or ISO timestamp) |
| `to` | string | null | Date range end (YYYY-MM-DD or ISO timestamp) |
| `planId` | string | null | Filter to checkpoints under a specific plan |
| `workspace` | string | "current" | "current" or "all" (cross-project, daemon mode only) |

## Search Behavior

The `search` parameter uses BM25 full-text search via Tantivy — not fuzzy matching. This means:

- Exact and stemmed terms are matched and ranked by relevance
- Multi-word queries find checkpoints containing those terms (OR semantics)
- Results are scored and returned in relevance order
- Searching is efficient even with many checkpoints

## Parameter Examples

**Standard recall (most common):**
```
mcp__julie__recall()
```

**More history:**
```
mcp__julie__recall({ limit: 15, full: true })
```

**Specific search:**
```
mcp__julie__recall({ search: "database migration", full: true })
```

**Recent activity:**
```
mcp__julie__recall({ since: "4h" })
mcp__julie__recall({ days: 1 })
```

**Date range:**
```
mcp__julie__recall({ from: "2026-03-01", to: "2026-03-07" })
```

**Cross-project standup (daemon mode only):**
```
mcp__julie__recall({ workspace: "all", days: 1 })
```

**Plan only, no checkpoints:**
```
mcp__julie__recall({ limit: 0 })
```

## Interpreting Results

The response includes:

- **Active Plan** — The current development plan (if one is activated). Contains title, content, status, and tags.
- **Checkpoints** — Chronological list of saved progress snapshots. Each has a summary, timestamp, and (with `full: true`) detailed description and git metadata.
- **Workspace Summaries** — Only returned with `workspace: "all"` in daemon mode. Shows activity across multiple projects.

## Processing Large Result Sets

When recall returns many checkpoints:

1. **Group by date** — Identify work sessions and their boundaries
2. **Identify themes** — What topics or features recur across checkpoints?
3. **Highlight blockers** — Surface any unresolved issues or decisions
4. **Find the thread** — Trace the progression of a feature or investigation
5. **Surface decisions** — Note architectural choices, rejected approaches, and rationale

## Cross-Project Recall

The `workspace: "all"` parameter enables cross-project recall, but it requires Julie to be running in **daemon mode** (via `julie daemon start`). When running as a direct MCP stdio server, only the current workspace is available.

## After Recall

Trust recalled context. Checkpoints were saved by the agent during active work — don't re-verify information from them. Use the recalled state as your starting point and continue from where the previous session left off.

If the active plan has tasks, pick up from the next incomplete task. If a checkpoint describes work in progress, continue that work.
