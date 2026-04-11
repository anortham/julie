---
name: impact-analysis
description: Analyze what would break if a symbol changes: finds callers, groups by risk level, assesses impact. Use when the user asks about blast radius, who uses a symbol, or is planning a refactor.
user-invocable: true
arguments: "<symbol_name>"
allowed-tools: mcp__julie__fast_refs, mcp__julie__deep_dive, mcp__julie__get_context
---

# Impact Analysis

Analyze the impact of changing a symbol by finding all references and assessing risk. Use this BEFORE modifying widely-used symbols.

## Process

### Step 0: Orient on the Area (Optional)

Get a broad view of the symbol's neighborhood:

```
get_context(query="<symbol_name>")
```

This reveals the symbol's centrality (how well-connected it is) and surrounding context. High-centrality symbols in the pivot list are inherently higher risk — they're well-connected in the reference graph.

### Step 1: Find All References

```
fast_refs(symbol="<symbol>", include_definition=true, limit=100)
```

### Step 2: Deep Dive the Symbol

```
deep_dive(symbol="<symbol>", depth="context")
```

Understand what the symbol does, its signature, and what it depends on.

If `deep_dive` returns the wrong symbol (common names like `new`, `result`, `config`), use `context_file` to disambiguate:

```
deep_dive(symbol="<symbol>", context_file="<partial_file_path>")
```

### Step 3: Categorize References by Risk

Group each reference by the file it appears in, then classify:

**High Risk** — Changes here could cause cascading failures:
- Entry points and core orchestration files (e.g., main entry, request handlers, routers)
- Files with 10+ references to this symbol
- Files that re-export or wrap this symbol

**Medium Risk** — Changes need careful testing:
- Feature implementation files
- Data access / storage modules
- Files with 3-9 references

**Low Risk** — Changes are isolated:
- Test files (paths containing `test`, `tests`, `spec`, `__tests__`, or test annotations)
- Files with 1-2 references

### Step 4: Sample Deep Dives on High-Risk Callers

For each high-risk file, `deep_dive` on the calling function to understand HOW the symbol is used:
- Is it called with specific arguments?
- Does the caller depend on the return type?
- Is it used in error handling paths?

### Step 5: Report

```
Impact Analysis: <symbol_name>
Definition: <file>:<line> (<kind>)
Centrality: <high/medium/low>

Total: <N> references across <M> files

High Risk (<count> files):
  src/handler.rs — 15 refs
    Callers: process_request, handle_error, validate_input
    Usage: Core request pipeline, changes here affect all tool calls

  src/database/queries.rs — 12 refs
    Callers: fetch_symbols, update_index
    Usage: Database layer, type changes would require migration

Medium Risk (<count> files):
  src/tools/search.rs — 5 refs
  src/tools/navigation.rs — 3 refs

Low Risk (<count> files):
  src/tests/search_tests.rs — 8 refs (test code)
  src/tests/handler_tests.rs — 2 refs (test code)

Recommendation:
  <1-2 sentences on how to approach this change safely>
```

## Important Notes

- **Always check test coverage** — high-risk changes with no test references are especially dangerous
- **Type changes cascade** — if the symbol is a type/struct, any field change affects all users
- **Interface/trait changes are widest** — changing an interface method, trait, or abstract class affects all implementors
- **Cross-workspace**: Call `manage_workspace(operation="open", path="<path>")` first, then pass the returned `workspace_id` to all tool calls
