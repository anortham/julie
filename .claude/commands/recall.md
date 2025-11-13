---
allowed-tools: mcp__julie__recall, mcp__julie__fast_search
argument-hint: [time|topic] [--type type] [--since time]
description: Retrieve development memories
---

IMMEDIATELY retrieve development memories based on the provided query. DO NOT wait or ask for confirmation.

Determine query mode from $ARGUMENTS and execute the appropriate tool NOW:

**Time-based query** (e.g., "30m", "1hr", "3d"):
1. Parse the time expression (m/min=minutes, h/hr=hours, d/day=days)
2. Calculate the "since" datetime in ISO 8601 format with UTC timezone (MUST end with 'Z')
   Example: "2025-11-10T02:10:08Z"
3. IMMEDIATELY call mcp__julie__recall with the since parameter

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
