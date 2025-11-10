---
name: plan-tracking
description: Track working plans and tasks using Julie's mutable plan system. Activates when planning work, tracking progress, or managing multiple tasks. Provides working memory with atomic updates and one active plan enforcement.
allowed-tools: mcp__julie__plan, mcp__julie__fast_search
---

# Plan Tracking Skill

## Purpose
Use Julie's **mutable plan system** for working memory - track current tasks, update progress, manage active work.

## When to Activate
- Starting multi-step work (needs planning)
- Tracking task progress
- Switching between different efforts
- Organizing complex features
- Managing "what am I working on?"

## The Plan System

**Working Memory vs Knowledge Base:**
- **Checkpoints** = Immutable knowledge (what was done)
- **Plans** = Mutable working memory (what needs doing)

**One Active Plan Rule:**
- Only ONE plan can be "active" at a time
- Others are "archived" or "completed"
- Enforces focus, prevents context switching

---

## Plan Operations (6 Actions)

### 1. Save - Create New Plan

```
Starting work → create plan

plan({
  action: "save",
  title: "Add Search Feature",
  content: "## Tasks\n- [ ] Design API\n- [ ] Implement backend\n- [ ] Add tests",
  activate: true  // optional, defaults to true
})

→ Creates plan_add-search-feature.json
→ Auto-activates (deactivates other plans)
→ Returns plan with ID
```

**When:** Starting new multi-step work that needs tracking

### 2. Get - Retrieve Specific Plan

```
Check plan status → get by ID

plan({
  action: "get",
  id: "plan_add-search-feature"
})

→ Returns plan with current status, content, timestamp
```

**When:** Checking specific plan details

### 3. List - See All Plans

```
See all plans → list with optional filter

plan({
  action: "list",
  status: "active"  // optional: "active", "archived", "completed"
})

→ Returns plans sorted by timestamp (most recent first)
```

**Common filters:**
- `status: "active"` - Currently working on
- `status: "completed"` - Finished work
- No filter - All plans

### 4. Activate - Switch Active Plan

```
Switch focus → activate different plan

plan({
  action: "activate",
  id: "plan_refactor-database"
})

→ Sets this plan to "active"
→ Archives all other plans
→ Only ONE plan active at a time
```

**When:** Switching between different work streams

### 5. Update - Modify Existing Plan

```
Update progress → modify plan content/status

plan({
  action: "update",
  id: "plan_add-search-feature",
  content: "## Tasks\n- [x] Design API\n- [ ] Implement backend\n- [ ] Add tests",
  status: "active"  // optional
})

→ Atomic update (temp + rename)
→ Timestamp updated automatically
→ Returns updated plan
```

**When:** Progress made, tasks completed, content needs updating

### 6. Complete - Mark Plan Done

```
Work finished → mark complete

plan({
  action: "complete",
  id: "plan_add-search-feature"
})

→ Sets status to "completed"
→ Timestamp updated
→ Plan archived from active work
```

**When:** All tasks done, feature shipped

---

## The Complete Plan Workflow

```
┌─────────────────────────────────────────────┐
│ 1. Save - Create Plan                      │
│                                             │
│ plan({ action: "save", title: "...",       │
│        content: "## Tasks\n- [ ] ..." })   │
│ → Auto-activates (ONE active plan)         │
└────────────┬────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────┐
│ 2. Work & Update Progress                  │
│                                             │
│ → Complete tasks                            │
│ plan({ action: "update", id: "...",        │
│        content: "- [x] Done\n- [ ] Next"}) │
│ → Update as you go                          │
└────────────┬────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────┐
│ 3. Complete When Done                      │
│                                             │
│ plan({ action: "complete", id: "..." })    │
│ → Status: "completed"                       │
│ → Archived from active work                │
└────────────┬────────────────────────────────┘
             │
             ▼
    Plan finished, checkpoint created! ✅
```

---

## Plan Content Format (Markdown)

**Recommended structure:**

```markdown
## Tasks
- [ ] Task 1
- [ ] Task 2
- [x] Task 3 (completed)

## Notes
- Important decision: chose X over Y
- Blocked by: dependency Z

## Progress
Started: 2025-01-10
Updated: 2025-01-11
```

**Key points:**
- Use markdown checkboxes `- [ ]` and `- [x]`
- Organize with headers
- Track blockers and notes
- Update timestamps manually (auto timestamp in metadata)

---

## Workflow Patterns

### Pattern 1: Feature Development

```
Start feature:
  plan({ action: "save", title: "User Profile Page",
         content: "## Tasks\n- [ ] Design\n- [ ] Backend\n- [ ] Frontend\n- [ ] Tests" })

During work:
  plan({ action: "update", id: "plan_user-profile-page",
         content: "## Tasks\n- [x] Design\n- [x] Backend\n- [ ] Frontend\n- [ ] Tests" })

When done:
  plan({ action: "complete", id: "plan_user-profile-page" })
  checkpoint({ description: "Completed user profile page", tags: ["feature"] })
```

### Pattern 2: Multi-Track Work

```
Have 3 efforts in flight:

  plan({ action: "list" })
  → Shows all plans with status

Switch focus:
  plan({ action: "activate", id: "plan_urgent-bugfix" })
  → Urgent work becomes active
  → Other plans archived (still accessible)

Return to original work:
  plan({ action: "activate", id: "plan_refactor-database" })
  → Resume where you left off
```

### Pattern 3: Long-Running Projects

```
Start project:
  plan({ action: "save", title: "Database Migration",
         content: "## Phase 1\n- [ ] Schema design\n- [ ] Write migration\n\n## Phase 2\n- [ ] Test on staging\n- [ ] Deploy to prod" })

Weekly updates:
  plan({ action: "update", ..., content: "..." })
  → Update progress, add notes, track blockers

Monthly:
  plan({ action: "list", status: "active" })
  → See active work, reprioritize
```

---

## Integration with Other Skills

### With Development Memory
```
Plan created → work happens → checkpoint milestones

plan({ action: "save", title: "Refactor Auth" })
→ Work on refactoring...
checkpoint({ description: "Completed phase 1 of auth refactor", tags: ["refactor"] })
→ More work...
plan({ action: "complete", id: "plan_refactor-auth" })
checkpoint({ description: "Auth refactor complete - 60% code reduction", tags: ["refactor", "complete"] })
```

### With Sherpa Workflows
```
[Sherpa: guide check] → get current phase
[Plan: update] → mark task done in plan
[Sherpa: guide done] → progress in workflow
[Plan: update] → update plan content

→ Sherpa tracks workflow phases
→ Plan tracks overall feature tasks
→ Complementary, not competing
```

### With Systematic Development
```
[Phase: Research] → plan({ action: "update", content: "Research done" })
[Phase: Plan] → plan({ action: "update", content: "Approach decided" })
[Phase: Tests] → plan({ action: "update", content: "Tests written" })
[Phase: Implementation] → plan({ action: "update", content: "Feature complete" })
[Phase: Verification] → plan({ action: "complete" })
```

---

## Atomic Updates (Safety)

Plans use **atomic writes** (temp + rename pattern):

```
Update happens:
1. Write to temp file: plan_add-search.json.tmp
2. Rename to real file: plan_add-search.json
3. Atomic operation (no partial writes)

→ Process crash during update? No corruption.
→ File is either old version or new version
→ Never half-written
```

**Why this matters:** Safe concurrent access, no corruption risk

---

## SQL Views (Searchability)

Plans are indexed in SQL views:

```sql
-- View: plan_memories
SELECT id, timestamp, title, status FROM plans;

-- Searchable via fast_search
fast_search({
  query: "database migration",
  search_method: "text",
  file_pattern: ".memories/plans/**/*.json"
})

→ Plans are searchable just like checkpoints
→ Semantic search works on plan content
```

---

## Storage Location

```
.memories/plans/
├── plan_add-search-feature.json
├── plan_refactor-database.json
└── plan_user-profile-page.json
```

**Key points:**
- Stable filenames (slug from title)
- Git-trackable (team can see plans)
- Human-readable (can edit manually if needed)
- Mutable (update in-place)

---

## Plan Lifecycle

```
Created → Active → [Updated many times] → Completed

Saved
  ↓
Active (ONE at a time)
  ↓
[Update, Update, Update...]
  ↓
Completed (archived)
```

**Alternative paths:**
- Saved → Active → Archived (switched away) → Activated again → Completed
- Saved → Active → Archived (abandoned, not completed)

---

## Key Behaviors

### ✅ DO
- Create plan when starting multi-step work
- Update plan as progress is made
- Use ONE active plan at a time (focus!)
- Complete plans when done (closure)
- Use markdown checkboxes for tasks
- Checkpoint major milestones (complement plans)
- List plans to see all work streams

### ❌ DON'T
- Create plans for single-step tasks (use checkpoint when done)
- Forget to update progress (stale plans are useless)
- Have multiple active plans (enforced by system anyway)
- Skip completing plans (mark closure!)
- Use plans as knowledge base (that's checkpoints)
- Manually edit plan files (use update action)

---

## Success Criteria

This skill succeeds when:
- ✅ Plans track all active work
- ✅ Only ONE plan active at a time
- ✅ Plans updated as progress is made
- ✅ Completed plans archived properly
- ✅ Clear visibility into work streams
- ✅ Easy to switch focus between efforts
- ✅ Plans complement checkpoints (not duplicate)

---

## Performance

- **save**: ~50ms (create file + activate)
- **get**: ~5ms (read single file)
- **list**: ~20ms (read all plans + sort)
- **activate**: ~100ms (update multiple files)
- **update**: ~50ms (atomic write)
- **complete**: ~30ms (update status)

---

**Remember:** Plans are working memory for active tasks. Checkpoints are knowledge base for completed work. Use both.

**The Rule:** Multi-step work → plan created. Task done → plan updated. Work finished → plan completed. Always.
