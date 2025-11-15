---
allowed-tools: mcp__julie__recall, mcp__julie__fast_search
argument-hint: [time|topic] [--type type] [--since time]
description: Retrieve development memories
---

IMMEDIATELY retrieve development memories based on the provided query. DO NOT wait or ask for confirmation.

Determine query mode from $ARGUMENTS and execute the appropriate tool NOW:

**Time-based query** (e.g., "30m", "1hr", "3d"):
⚠️ CRITICAL: Get actual current time from system BEFORE calculations!

1. Run `date '+%Y-%m-%dT%H:%M:%S'` to get current LOCAL datetime
   Example output: "2025-11-14T20:53:43"
2. Parse the time expression (m/min=minutes, h/hr=hours, d/day=days)
3. Add 10-minute margin for reliability (e.g., "10m" → look back 20 minutes)
   - Accounts for: time calculation edge cases, user imprecision
4. Calculate the "since" datetime by subtracting (duration + margin) from current time
   - Use LOCAL time format (NO 'Z' suffix) - tool converts to UTC automatically
   - Format: "YYYY-MM-DDTHH:MM:SS" (example: "2025-11-14T20:33:43")
5. IMMEDIATELY call mcp__julie__recall with the since parameter

💡 TIP: For very recent memories (< 30 minutes), just use limit instead:
   "/recall" (no args) → last 10 memories

**Topic-based query** (e.g., "db path bug", "auth implementation"):
1. IMMEDIATELY call mcp__julie__fast_search with:
   - query=$ARGUMENTS
   - search_method="hybrid"
   - search_target="content"
   - file_pattern=".memories/**/*.json"
   - limit=20

**Filtered query** (e.g., "--type decision", "--since 2d"):
1. Parse the flags (--type, --since, --tags)
2. IMMEDIATELY call mcp__julie__recall with the appropriate filters
3. Can combine with fast_search for topic + filter combinations

**No arguments provided**:
1. IMMEDIATELY call mcp__julie__recall with limit=10 to get the last 10 memories

After retrieving results, present them formatted with:
- Type icon (✓ checkpoint, 🎯 decision, 💡 learning, 👁️ observation)
- Description
- Relative time and git branch
- Tags (if present)
- Keep output scannable (newest first)
