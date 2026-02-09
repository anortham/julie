---
name: impact-analysis
description: Analyze what would break if a symbol is changed — finds all callers, groups by risk level, and assesses change impact
user-invocable: true
arguments: "<symbol_name>"
allowed-tools: mcp__julie__fast_refs, mcp__julie__deep_dive
---

# Impact Analysis

Analyze the impact of changing a symbol by finding all references and assessing risk. Use this BEFORE modifying widely-used symbols.

## Process

### Step 1: Find All References

```
fast_refs(symbol="<symbol>", include_definition=true, limit=100)
```

### Step 2: Deep Dive the Symbol

```
deep_dive(symbol="<symbol>", depth="context")
```

Understand what the symbol does, its signature, and what it depends on.

### Step 3: Categorize References by Risk

Group each reference by the file it appears in, then classify:

**High Risk** — Changes here could cause cascading failures:
- Core files (main.rs, handler.rs, mod.rs)
- Files with 10+ references to this symbol
- Files that re-export or wrap this symbol

**Medium Risk** — Changes need careful testing:
- Tool implementation files
- Database/search modules
- Files with 3-9 references

**Low Risk** — Changes are isolated:
- Test files (any file in tests/ or with `#[test]`)
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
- **Trait changes are widest** — changing a trait method affects all implementors
