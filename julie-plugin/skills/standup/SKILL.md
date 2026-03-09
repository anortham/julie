---
name: standup
description: Generate a standup report from Julie memory across all projects — use when the user asks for a standup, daily update, progress summary, what they've been working on, or needs a report for a team sync
allowed-tools: mcp__julie__recall, mcp__julie__plan
---

# Standup — Cross-Project Status Report

## What This Does

Generates a concise standup by recalling checkpoints across all workspaces and reviewing active plans. Pulls from Julie's developer memory — checkpoints, decisions, incidents, learnings — and synthesizes them into a brief status report.

## How to Generate a Standup

### Step 1: Recall cross-project activity

Pull recent checkpoints from all workspaces. Cross-project recall requires **daemon mode** (`julie daemon start`) — if Julie is running in stdio mode, only the current workspace is available.

**Normal day (yesterday's work):**

```
mcp__julie__recall({ workspace: "all", days: 1, full: true })
```

**Monday standup (cover the weekend):**

```
mcp__julie__recall({ workspace: "all", days: 3, full: true })
```

**Custom range:**

```
mcp__julie__recall({ workspace: "all", from: "2026-03-01", to: "2026-03-07", full: true })
```

**Current session only (stdio mode fallback):**

```
mcp__julie__recall({ days: 1, full: true })
```

### Step 2: Gather plans from ALL sources

**Source 1 — Julie plans** (returned alongside recall output):

The recall response includes the active plan. Also list all plans to check for recently updated ones:

```
mcp__julie__plan({ action: "list" })
```

**Source 2 — Plan files on disk:**

Scan for recent plan documents:

```
docs/plans/*.md
```

### Step 3: Assess plan status from evidence

Cross-reference each plan's Status field against the checkpoints returned in Step 1:

- **Checkpoints reference the plan's tasks?** Progress is real.
- **No checkpoints but plan says "in progress"?** Stalled — flag it.
- **Checkpoints show completed work not in any plan?** Ad-hoc work — include it under a separate heading.

### Step 4: Synthesize the report

Use the format below. Do NOT ask the user what to include — just generate the report from the evidence.

## Report Format

### Multi-project format

When checkpoints span multiple projects, group by project with bullets for accomplishments, blockquotes for next/blocked, and plan references:

```markdown
## Standup — Feb 14, 2026

### project-alpha
- Implemented 4 skill files with behavioral language patterns
- Converted tool handlers from JSON to markdown output
> Next: Test skills with live agent sessions
> Plan: v5.1 skills refresh — 2/4 tasks complete (docs/plans/2026-02-16-v5.1-implementation.md)

### api-gateway
- Fixed rate limiter race condition in Redis cluster mode
- Added integration tests for multi-node scenarios
> Next: Deploy to staging, run load tests
> Blocked: Waiting on DevOps for staging Redis cluster
```

### Single-project format

When all checkpoints are from a single project, drop the project grouping and use section headers:

```markdown
## Standup — Feb 14, 2026

### Done
- Implemented 4 skill files with behavioral language patterns
- Converted tool handlers from JSON to markdown output

### Up Next
- Test skills with live agent sessions
- v5.1 skills refresh: workspace env var implementation (2/4 tasks complete)

### Blocked
- Nothing currently blocked
```

## Synthesis Rules

- **Be concise.** A standup is 2 minutes, not 20. One line per accomplishment.
- **Lead with impact, not activity.** "Shipped auth refresh tokens" not "Worked on auth."
- **Group by theme.** Five checkpoints about "auth" become one bullet: "Implemented auth token refresh with rotation."
- **Highlight blockers prominently.** These are the most actionable items in a standup.
- **Skip noise.** Minor refactors, formatting changes, and config tweaks don't need individual mentions unless they were the main work.
- **Use past tense for accomplishments.** "Shipped," "Fixed," "Implemented" — not "Working on" for done items.
- **Surface decisions.** If a checkpoint captured an architectural decision, mention it briefly — the team may need to know.
- **Include plan progress.** When active plans exist, include a brief progress summary (e.g., "3/5 tasks complete") in the Up Next section.
- **Abbreviated month names** in headers. `Feb 14` not `February 14`.
- **Date range in header** when covering multiple days (`Feb 12–14, 2026`).

## Handling Edge Cases

### No checkpoints found
Report honestly: "No activity recorded in the requested period." Don't fabricate.

### Single project only
Use the single-project format above (Done / Up Next / Blocked sections).

### Too many checkpoints (20+)
Be more aggressive about grouping. Summarize by theme rather than listing individual items. A standup with 15 bullet points defeats the purpose.

### No plans found
That's fine — just skip the plan progress lines. Not every project has active plans.

### Plans in docs/plans/ but no Julie plan
Include them in the forward-looking section. Plans don't need to be in Julie to be useful for standup.

## Critical Rules

- **Do NOT ask the user what to include.** Recall gives you everything. Synthesize it yourself.
- **Do NOT fabricate activity.** Only report what checkpoints actually show.
- **Keep it standup-length.** If your report is more than a screenful, you're being too verbose.
- **Include dates** when covering multi-day ranges so the reader knows the timeline.
- **Check BOTH plan sources.** `.memories/plans/` (via recall) AND `docs/plans/` (via file reading). Missing one source means an incomplete forward-looking view.
- **Infer plan status from evidence.** Don't trust stale Status headers — verify against checkpoint activity.
- **Cross-project recall requires daemon mode** — fall back to current workspace if `workspace: "all"` fails.
