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

Use when checkpoints span more than one workspace.

```markdown
# Standup — {date}

## {Project A}
- Completed X — {brief context}
- Fixed Y — {root cause in one line}

## {Project B}
- Shipped Z feature — {what it does}
- Investigated flaky test — {outcome}

> **Up next:** {highest priority item across projects}
> **Blocked:** {blockers, if any — omit section if none}
```

### Single-project format

Use when all activity is within one workspace.

```markdown
# Standup — {date}

## Done
- {completed item with brief context}
- {completed item with brief context}

## Up Next
- {next priority}

## Blocked
- {blocker} — {who/what is needed to unblock}
```

Omit the Blocked section entirely if there are no blockers.

## Synthesis Rules

- **Be concise** — standup length, not essay length. One line per item.
- **Group by theme** — if three checkpoints are about the same feature, merge them into one bullet.
- **Highlight blockers** — these are the most important thing in a standup.
- **Skip noise** — routine test runs, minor formatting fixes, and trivial commits are not standup material.
- **Use past tense** for completed work, present tense for blockers and next items.
- **Surface decisions** — if a checkpoint has `type: "decision"`, call it out. Decisions are standup gold.
- **Include plan progress** — "Phase 2 tasks 3-5 complete, 6-7 remaining" is more useful than listing each task.
- **Include dates** — especially for multi-day ranges. "Monday: ..., Tuesday: ..." helps orient the reader.

## Critical Rules

- Do NOT ask the user what to include — generate the report from checkpoint evidence
- Do NOT fabricate activity — if recall returns nothing, say "no checkpoints found for this period"
- Keep it standup-length — if you're writing more than ~15 lines of bullets, you're over-explaining
- Include dates when covering multiple days
- Check BOTH plan sources (Julie plans AND docs/plans/*.md)
- Infer plan status from checkpoint evidence, don't just parrot the status field
- Cross-project recall requires daemon mode — fall back to current workspace if `workspace: "all"` fails
