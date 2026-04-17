---
name: architecture
description: Generate an architecture overview — key entry points, module map, dependency flow, and suggested reading order. Use when the user is new to a codebase, asks "how does this work?", wants an architecture overview, or needs onboarding documentation.
user-invocable: true
disable-model-invocation: true
allowed-tools: mcp__julie__deep_dive, mcp__julie__get_context, mcp__julie__get_symbols, mcp__julie__fast_search, mcp__julie__manage_workspace
---

# Architecture Overview

Generate a structural overview of the codebase or a specific area. Designed for onboarding, documentation, or understanding unfamiliar code.

## Arguments

`$ARGUMENTS` is the area to analyze. Can be a concept ("authentication"), a path ("src/tools/"), or empty for the whole codebase.

## Query Pattern

### Step 1: Oriented Discovery

```
get_context(query="$ARGUMENTS", format="readable")
```

This returns pivots (key symbols with code), neighbors (connected symbols), and a file map — all token-budgeted. The pivots are the starting point for understanding the area.

### Step 2: Find Key Entry Points

```
fast_search(query="$ARGUMENTS", search_target="definitions", limit=20)
```

Search results are ranked by centrality (reference count), so the top results are the most connected symbols -- the architectural backbone. Focus on public functions/methods.

### Step 3: Understand Entry Point Structure

For the top 5 highest-centrality symbols:

```
get_symbols(file_path="<file>", mode="structure", max_depth=1)
```

This shows the full outline of each key file without reading all the code.

### Step 4: Trace Key Connections

For the top 3 entry points:

```
deep_dive(symbol="<name>", depth="overview")
```

This reveals callers (who depends on this?) and callees (what does it use?) — the dependency flow.

## Report Format

```markdown
# Architecture Overview
**Area:** [area or "Full codebase"] | **Date:** [today]

## Overview
[2-3 paragraph description of what this area/codebase does, inferred from:
- Symbol names and kinds (what concepts exist?)
- File paths (how is it organized?)
- Doc comments on pivots (what do the key functions say they do?)
- Centrality patterns (what's important?)]

## Key Entry Points
[Top 10-15 highest-centrality public symbols, formatted as:]

| Symbol | Location | Centrality | Role |
|--------|----------|-----------|------|
| name | file:line | score | One-line description of what it does |

## Module Map
[Group files by directory/responsibility. For each group:]

### [Directory/Module Name]
**Purpose:** [What this module is responsible for]
**Key files:**
- `file.rs` — [what it contains, key exports]
- `file2.rs` — [what it contains]

## Dependency Flow
[How the key modules connect to each other]
[Use the caller/callee data from deep_dive to show:]

External input -> [Entry Point A] -> [Module B] -> [Database Layer]
               -> [Entry Point C] -> [Module D] -> [External API]

[Describe the main data flow paths in 3-5 sentences]

## Suggested Reading Order
[For someone new to this code, recommend which files to read first and why:]

1. **Start here:** `file.rs` — [why: it's the main entry point / defines the core types]
2. **Then read:** `file2.rs` — [why: it implements the key logic called by #1]
3. **Then read:** `file3.rs` — [why: it handles the output / storage / API layer]
4. **Reference as needed:** `file4.rs` — [why: utility code, read when you encounter calls to it]
```

## Guidelines

- Focus on STRUCTURE, not implementation details — this is a map, not a tutorial
- Use the centrality scores to identify what matters — high centrality = architecturally important
- Group files by what they DO, not by what they ARE (group by "authentication" not by "structs vs functions")
- The suggested reading order should form a narrative: start with the big picture, then drill into specifics
- If the codebase is small (<20 files), show everything. If large, focus on the highest-centrality modules
- **Cross-workspace**: Call `manage_workspace(operation="open", path="<path>")` first, then pass the returned `workspace_id` to all tool calls
