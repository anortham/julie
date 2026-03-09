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
## Plan Status — "<plan title>" — <date>
Source: Julie active plan
3/5 items complete (60%)
```

When reporting on docs/plans:
```
## Plan Status — "<plan title>" — <date>
Source: docs/plans/2026-02-16-v5.1-implementation.md
2/8 tasks complete (25%)
```

Always include the source location and completion fraction for instant comprehension.

### Completed
Items with clear checkpoint evidence of completion. Include the approximate date completed and any notable details.

- [x] JWT refresh token rotation — completed Feb 12, shipped with full test coverage
- [x] Rate limiter Redis support — completed Feb 13, integration tests passing

### In Progress
Items with recent checkpoints showing active work but not yet complete.

- [ ] Session management API — 3 checkpoints this week, endpoints implemented but missing error handling
- [ ] Admin dashboard auth — started Feb 13, basic scaffold in place

### Not Started
Plan items with no checkpoint activity at all. Flag these — they might be blocked, deprioritized, or forgotten.

- [ ] API documentation — no activity found
- [ ] Load testing — no activity found (may be blocked by staging environment)

### Drift Assessment
Work captured in checkpoints that doesn't map to any plan item. This isn't automatically bad — emergent work happens — but it should be called out.

**Unplanned work detected:**
- Bug fix: file corruption on concurrent writes (2 checkpoints, Feb 11)
- Dependency upgrade: fuse.js v7 migration (1 checkpoint, Feb 12)

These consumed ~1 day of effort outside the plan scope.

### Overall Health

Render the health assessment as a `### Overall:` header with the verdict, followed by a direct, honest summary.

| Health | Meaning |
|--------|---------|
| On track | Most items progressing, minimal drift, no blockers |
| Minor drift | Some unplanned work, but core goals advancing |
| Significant drift | More unplanned work than planned work, or key items stalled |
| Stalled | Little to no progress on plan items, or major blockers |

Be direct. "The plan called for 5 deliverables this sprint. Two are done, one is in progress, and two haven't been touched. You're behind, and the unplanned auth bug ate a day." That's more useful than "progress is being made."

## Multiple Plans

When both a Julie active plan AND docs/plans files exist, report on each separately with clear source attribution. Present the Julie plan first (it's the "active" strategic direction), then docs/plans (implementation plans).

If plans overlap or conflict, flag it: "The Julie active plan focuses on auth, but docs/plans shows active work on a performance benchmark suite. These may be competing priorities."

### Suggest Plan Updates

If the assessment reveals drift or completed items, suggest concrete plan updates:

```
mcp__julie__plan({ action: "update", id: "<plan_id>", content: "<updated markdown>" })
```

## Critical Rules

- **Do NOT sugarcoat.** If the plan is behind, say so. The user needs accurate information, not comfort.
- **Do NOT fabricate progress.** Only claim completion for items with actual checkpoint evidence.
- **DO flag scope drift.** Unplanned work is a leading indicator of timeline slip.
- **DO suggest plan updates.** If a plan is clearly outdated, recommend updating it — via `mcp__julie__plan({ action: "update" })` for Julie plans, or by editing the file directly for docs/plans.
- **Match checkpoints to plan items carefully.** A checkpoint about "auth" doesn't automatically satisfy a plan item about "auth" — read the descriptions and match on actual content.
- **Include time estimates when possible.** "3 checkpoints over 2 days" gives the user a sense of effort invested.
- **Check BOTH plan sources.** Missing a source means an incomplete assessment.
- **Attribute sources clearly.** The user should always know whether a plan came from `.memories/plans/` or `docs/plans/`.
- If no plan exists at all, say so and offer to help create one.
