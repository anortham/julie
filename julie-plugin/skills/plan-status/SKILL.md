---
name: plan-status
description: Assess progress against the active Julie plan using checkpoints and plan data — use when the user asks about project progress, how things are going, what's been accomplished, or wants a status check against their plan
allowed-tools: mcp__julie__recall, mcp__julie__plan
---

# Plan Status — Progress Assessment

## What This Does

Pulls plans from multiple sources and recent checkpoints, then assesses how actual work aligns with planned goals. Gives the user an honest, unsweetened picture of where things stand.

## How to Assess Plan Status

### Step 1: Gather plans from ALL sources

**Source 1 — Julie plans:**

```
mcp__julie__recall({ limit: 0 })
```

This returns the active plan without checkpoints. If no active plan exists, use:

```
mcp__julie__plan({ action: "list" })
```

to find available plans, then fetch the relevant one with:

```
mcp__julie__plan({ action: "get", id: "<plan_id>" })
```

**Source 2 — Project plan docs:**

Scan `docs/plans/*.md` and any other plan-like files in the repository for supplementary goals and milestones.

### Step 2: Recall recent checkpoints

```
mcp__julie__recall({ days: 7, limit: 20, full: true })
```

Use `full: true` to get complete descriptions and git metadata — you need the detail to map work to plan items accurately.

If the plan spans more than a week, widen the window:

```
mcp__julie__recall({ days: 14, limit: 50, full: true })
```

Or scope to a specific plan:

```
mcp__julie__recall({ planId: "<plan_id>", full: true })
```

### Step 3: Cross-reference and assess

Map each checkpoint to the plan goal it advances. Classify every plan item into one of:

- **Done** — checkpoint evidence confirms completion
- **In Progress** — checkpoints show partial work
- **Blocked** — work started but stalled (flag the blocker if identifiable)
- **Not Started** — zero checkpoint activity against this goal
- **Scope Drift** — work that doesn't map to any plan item

## Report Format

### Header

```
## Plan Status: <plan title>
**Date:** <today>  |  **Source:** <Julie plan / docs/plans/ / both>  |  **Progress:** <completed>/<total> items
```

### Sections

**Completed**
Items with checkpoint evidence of completion. Include the checkpoint that confirms it.

**In Progress**
Items with partial work. Note what's done and what remains.

**Blocked**
Items where work stalled. Identify the blocker if possible.

**Not Started**
Plan items with no corresponding checkpoint activity.

**Scope Drift**
Work captured in checkpoints that doesn't map to any plan item. This isn't necessarily bad — flag it so the user can decide whether to update the plan.

**Overall Health**

| Health | Meaning |
|--------|---------|
| On track | Most items progressing, no significant drift |
| Minor drift | Some unplanned work, but core goals advancing |
| Significant drift | Substantial work outside the plan, or multiple stalled items |
| Stalled | Little to no progress on plan items |

### Suggest Plan Updates

If the assessment reveals drift or completed items, suggest concrete plan updates:

```
mcp__julie__plan({ action: "update", id: "<plan_id>", content: "<updated markdown>" })
```

## Critical Rules

- Do NOT sugarcoat — if progress is poor, say so plainly
- Do NOT fabricate progress — only count items with checkpoint evidence
- DO flag scope drift — unplanned work deserves visibility
- DO suggest plan updates when the plan no longer reflects reality
- Match checkpoints to plan items carefully — don't force a fit
- Check BOTH plan sources (Julie plans and docs/plans/ files)
- Attribute sources clearly — note whether each item came from a Julie plan or a doc file
- If no plan exists at all, say so and offer to help create one
