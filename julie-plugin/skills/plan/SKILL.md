---
name: plan
description: Create and manage persistent plans in Julie memory — use when starting multi-session work, making architectural decisions, or when the user discusses project direction, roadmaps, or design decisions that need to persist across sessions
allowed-tools: mcp__julie__plan
---

# Plan — Persistent Strategic Plans

## The Core Idea

Plans capture strategic direction that must survive across sessions. When you start multi-session work or make architectural decisions, save a plan immediately — the reasoning is fresh now and won't be available later. When work is done, mark the plan complete so future sessions don't waste effort on finished goals. A stale plan actively misleads; it's worse than having no plan at all.

## The ExitPlanMode Hook

When a plan is approved via `ExitPlanMode`, a hook fires that tells you to save it to Julie. This is the most critical moment in plan management:

- **Do NOT ask permission** — the hook instruction is your permission
- **Save immediately** — extract the title and content from the plan that was just written
- **Always activate** — set `activate: true` so future sessions see the plan
- **Plans represent hours of reasoning** — if you don't save, that work vanishes when the session ends

The hook exists because plans approved through Claude Code's plan mode contain the distilled result of collaborative design work. Losing that output is unacceptable.

## When to Create a Plan

Plans matter most when context needs to survive session boundaries. Without a plan, the next session starts from scratch.

- **Starting multi-session work** — save a plan with goals and approach so the next session knows where to pick up
- **After architectural decisions** — capture the direction immediately so future sessions don't re-derive or contradict it
- **After brainstorming/design sessions** — the approved design should outlive the conversation that produced it
- **New features spanning multiple sessions** — a plan keeps the thread when context compacts or sessions end
- **User describes project direction or roadmap** — that strategic context is exactly what plans are for

## How to Create a Plan

```
mcp__julie__plan({
  action: "save",
  title: "Auth System Overhaul",
  content: "## Goals\n\n- Migrate from session tokens to JWT...\n\n## Approach\n\n...\n\n## Tasks\n\n- [ ] Task 1\n- [ ] Task 2",
  tags: ["feature", "auth"],
  activate: true
})
```

### Plan Content Structure

Structure your plan content with clear sections:

1. **Goals** — what are we trying to achieve?
2. **Approach** — how are we going to do it?
3. **Tasks** — checklist of deliverables (use `- [ ]` / `- [x]`)
4. **Constraints** — anything we need to avoid or work around

### Key Parameters

- **activate: true** — set this so the plan appears at the top of every `recall()` response. Without activation, future sessions won't see the plan and the strategic direction is effectively lost.
- **tags** — categorize for search. Use consistent tags across checkpoints and plans.
- **id** — auto-generated from title via slugification. Only override if you need a specific slug.

## Managing Plan Lifecycle

### Check current plans
```
mcp__julie__plan({ action: "list" })
```

### Get a specific plan
```
mcp__julie__plan({ action: "get", id: "auth-system-overhaul" })
```

### Update a plan (add progress, change scope)
```
mcp__julie__plan({
  action: "update",
  id: "auth-system-overhaul",
  content: "## Goals\n\n...(updated content with progress noted)...",
  status: "active"
})
```

### Mark a plan complete
```
mcp__julie__plan({ action: "complete", id: "auth-system-overhaul" })
```

### Archive a superseded plan
```
mcp__julie__plan({
  action: "update",
  id: "old-plan",
  status: "archived"
})
```

## The Active Plan

Only ONE plan can be active per workspace. The active plan:
- Appears at the top of every `recall()` response
- Guides all work in the project
- Should reflect the current strategic direction

If priorities shift, either update the active plan or archive it and create a new one.

To activate a different plan:
```
mcp__julie__plan({ action: "activate", id: "new-plan-id" })
```

## Storage

Plans are stored as markdown files with YAML frontmatter at `{project}/.memories/plans/{plan-id}.md`. The active plan ID is tracked in `.memories/.active-plan`. This means plans are version-controlled alongside the project and visible to any tool that can read files.

## Plans vs Checkpoints

- **Plans are forward-looking** — they capture where you're going: goals, approach, task lists
- **Checkpoints are backward-looking** — they capture where you've been: progress, decisions, state

Use both. They serve different purposes and complement each other. A plan without checkpoints has no record of progress. Checkpoints without a plan have no strategic context.

## Why These Rules Matter

Plans exist so future sessions know the strategic direction. A plan that isn't activated or maintained actively harms future work — it's worse than having no plan at all.

- **Activate plans** so they appear in `recall()`. An inactive plan is invisible to future sessions, which defeats the purpose of creating it.
- **Mark plans complete when done.** A stale active plan misleads every future session into thinking work is still in progress and wastes effort re-investigating completed goals.
- **Complete or archive abandoned plans.** Orphaned active plans create confusion about what's actually being worked on.
- **Update plans as work progresses.** Check off tasks, note scope changes, record decisions — a plan that doesn't reflect reality isn't useful.
- **Save immediately after ExitPlanMode.** The hook fires for a reason. The plan content is the product of real work. Don't lose it.
