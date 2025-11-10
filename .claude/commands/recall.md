---
allowed-tools: mcp__julie__recall, mcp__julie__fast_search
argument-hint: [time|topic] [--type type] [--since time]
description: Retrieve development memories
---

Retrieve development memories based on the provided query.

Parse $ARGUMENTS to determine query mode:

**Time-based** (e.g., "30m", "1hr", "3d"):
- Parse time expression (m/min=minutes, h/hr=hours, d/day=days)
- Convert to ISO 8601 datetime with UTC timezone (must end with 'Z')
  Example: "2025-11-10T02:10:08Z" (always append Z for UTC)
- Use mcp__julie__recall with since parameter

**Topic-based** (e.g., "db path bug", "auth implementation"):
- Use mcp__julie__fast_search with:
  - query=$ARGUMENTS
  - search_method="hybrid" (combines FTS5 + semantic)
  - search_target="content"
  - file_pattern=".memories/**/*.json"
  - limit=20

**Filtered** (e.g., "--type decision", "--tags bug,auth"):
- Parse flags: --type, --tags, --since
- Use mcp__julie__recall with appropriate filters
- Can combine with topic search

**No arguments**:
- Use mcp__julie__recall with limit=10 (last 10 memories)

Present results formatted with:
- Type icon (✓ checkpoint, 🎯 decision, 💡 learning, 👁️ observation)
- Description
- Relative time and git branch
- Tags
- Keep output scannable (newest first)
