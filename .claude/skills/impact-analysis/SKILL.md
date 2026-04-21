---
name: impact-analysis
description: Analyze what would break if a symbol changes: finds callers, groups by risk level, assesses impact. Use when the user asks about blast radius, who uses a symbol, or is planning a refactor.
user-invocable: true
arguments: "<symbol_name>"
allowed-tools: mcp__julie__fast_refs, mcp__julie__deep_dive, mcp__julie__get_context, mcp__julie__blast_radius, mcp__julie__spillover_get
---

# Impact Analysis

Analyze the impact of changing a symbol by finding all references and assessing risk. Use this BEFORE modifying widely-used symbols.

## Process

### Step 1: One-shot impact via blast_radius

```
blast_radius(symbol_ids=["<id>"], max_depth=2, include_tests=true)
```

`blast_radius` is the primary entry point for impact analysis. One call returns ranked impacted symbols with why-reasons, likely tests, and (for revision-range seeds) deleted files. It walks the reference graph deterministically, so you don't have to chain `get_context → fast_refs → deep_dive` to build the same picture.

You can seed it three ways:
- `symbol_ids=["<id>"]` — impact of changing one or more specific symbols
- `file_paths=["src/foo.rs"]` — impact of editing whole files
- `from_revision="<ref>"`, `to_revision="<ref>"` — impact of a commit range

If the impact list is large, the first page includes `spillover_handle=br_xxx`. Hold onto it for Step 3.

### Step 2: Drill down into high-risk callers

For any impacted symbol you need to understand in depth (unfamiliar caller, ambiguous usage, high centrality), use the targeted tools:

```
fast_refs(symbol="<caller>", include_definition=true, limit=100)
deep_dive(symbol="<caller>", depth="context")
```

If `deep_dive` returns the wrong symbol (common names like `new`, `result`, `config`), use `context_file` to disambiguate:

```
deep_dive(symbol="<caller>", context_file="<partial_file_path>")
```

### Step 3: Page long impact lists

If Step 1 returned `spillover_handle=br_xxx`, fetch the rest without rerunning the walk:

```
spillover_get(spillover_handle="br_xxx")
```

Keep paging until the handle stops appearing.

### Reviewing what changed since a revision

For "what's the blast radius of everything that shipped since the last deploy?" use revision-range seeds:

```
blast_radius(from_revision="<base>", to_revision="<head>", include_tests=true)
```

This walks from every symbol touched in that range. Deleted files get reported in a separate section of the output (they have no symbols to walk from, so they need explicit callout). Useful for pre-deploy reviews and post-incident diffs.

### Step 4: Sample Deep Dives on High-Risk Callers

For each high-risk file surfaced by `blast_radius`, `deep_dive` on the calling function to understand HOW the symbol is used:
- Is it called with specific arguments?
- Does the caller depend on the return type?
- Is it used in error handling paths?

### Categorizing Callers by Risk

`blast_radius` gives you ranked impact with why-reasons, but you still want to sort callers into tiers for the final report:

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

Likely Tests:
  src/tests/handler_tests.rs::test_process_request
  src/tests/integration/pipeline.rs::test_end_to_end

Recommendation:
  <1-2 sentences on how to approach this change safely>
```

## Important Notes

- **Always check test coverage** — high-risk changes with no test references are especially dangerous. `blast_radius` surfaces likely tests via the `include_tests=true` flag, so use it.
- **Type changes cascade** — if the symbol is a type/struct, any field change affects all users
- **Interface/trait changes are widest** — changing an interface method, trait, or abstract class affects all implementors
- **Cross-workspace**: Call `manage_workspace(operation="open", path="<path>")` first, then pass the returned `workspace_id` to all tool calls
