---
name: standup
description: Use when user asks for a standup, daily summary, weekly report, or "what did I work on" — aggregates development memories across all Julie-registered projects
user-invocable: true
arguments: "[time_range]"
allowed-tools: mcp__julie__recall
---

# Standup Summary Generator

Generate narrative standup summaries by aggregating development memories across all registered Julie projects.

## Arguments

- **No argument**: Yesterday at 00:00 through now
- **`3d`**, **`7d`**: Last N days through now
- **`2026-01-20`**: Since specific date through now

## Process

### Step 1: Calculate Time Range

Parse the argument into an ISO 8601 `since` date:

| Input | Since Value |
|-------|-------------|
| (none) | Yesterday at 00:00 local time |
| `3d` | 3 days ago at 00:00 |
| `7d` | 7 days ago at 00:00 |
| `2026-01-20` | That date at 00:00 |

Format as `YYYY-MM-DDTHH:MM:SS` for the `since` parameter.

### Step 2: Recall Global Memories

Call the recall tool with global scope:

```json
{
  "scope": "global",
  "since": "<calculated_date>",
  "limit": 50
}
```

This scans all registered Julie projects and returns memories grouped by project with `## ProjectName` headers.

### Step 3: Synthesize Narrative

From the recalled memories, write a **2-4 paragraph standup summary** in past tense:

1. **Lead with the most significant work** — what would matter most in a standup meeting
2. **Group related work** — don't just list memories, connect them into coherent themes
3. **Use past tense** — "Fixed", "Implemented", "Designed", "Decided"
4. **Reference project names** when there are multiple projects
5. **Keep it concise** — a standup is 2-3 minutes, not a novel

## Output Format

Write the summary as natural prose. Do NOT output raw memory data — synthesize it.

Good example:
> Yesterday and today I focused on two main areas. In **Julie**, I implemented the cross-project memory system — a user-level project registry that auto-registers workspaces, plus a `scope: "global"` parameter on recall that aggregates memories across projects. This unlocks the standup feature. In **webapp**, I fixed the token refresh race condition that was causing intermittent 401 errors.

Bad example (don't do this):
> - checkpoint_aaa1: Implemented JWT auth
> - checkpoint_bbb1: Fixed bug
> - decision_ccc1: Chose PostgreSQL

## Edge Cases

- **No registered projects**: Say "No Julie projects are registered yet. Projects auto-register when you open them with Julie."
- **No memories in range**: Say "No development activity found for this time period. Try a wider range like `7d`."
- **Only one project**: Skip project grouping, just describe the work
- **Very many memories (30+)**: Focus on the most significant themes, don't try to mention everything
