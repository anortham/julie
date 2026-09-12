---
name: impact-analysis
description: "Analyze what would break if a symbol changes: finds callers, groups by risk level, assesses impact. Use when the user asks about blast radius, who uses a symbol, or is planning a refactor."
user-invocable: true
arguments: "<symbol, file path, or change target>"
allowed-tools: mcp__julie__fast_search, mcp__julie__fast_refs, mcp__julie__deep_dive, mcp__julie__get_context, mcp__julie__call_path, mcp__julie__blast_radius, mcp__julie__manage_workspace
---

# Impact Analysis

Analyze the impact of changing a symbol by finding all references and assessing risk. Use this BEFORE modifying widely-used symbols.

## Process

### Step 1: Resolve the change target

If the user gives you a symbol name, resolve the definition first so you know which file or symbol they mean:

```
fast_search(query="<symbol_name>", workspace="<workspace_id>")
deep_dive(symbol="<symbol_name>", context_file="<partial_file_path>", depth="overview", workspace="<workspace_id>")
```

Use `context_file` when the name is ambiguous. If the target is described conceptually rather than named exactly, try `fast_search(query="<concept>", backend="semantic", workspace="<workspace_id>")` or `backend="hybrid"` to find candidate symbols. Semantic/hybrid `fast_search` results are symbol-only; use explicit lexical for file paths and pure lexical comparison. Default `fast_search` may show labeled semantic fallback candidates only after an identifier-like unscoped lexical zero-hit. `blast_radius(symbol_ids=[...], workspace="<workspace_id>")` is the tightest seed mode, but only use it when another Julie result already gave you concrete symbol IDs. If all you have is a definition file, use `file_paths=[...]`.

### Step 2: One-shot impact via blast_radius

```
blast_radius(file_paths=["<definition_file>"], max_depth=2, include_tests=true, workspace="<workspace_id>")
```

`blast_radius` is the primary entry point for impact analysis. One call returns ranked impacted symbols with why-reasons and likely tests. It walks the reference graph deterministically, so you don't have to chain `get_context → fast_refs → deep_dive` to build the same picture.

You can seed it three ways:
- `file_paths=["src/foo.rs"]` — default when you know the changed file but not a symbol ID
- `symbol_ids=["<id>"]` — tighter impact when another Julie result already gave you concrete symbol IDs

If the impact list is large, the output ends with `Output truncated at <N> results; narrow the query or pass a smaller limit.` See Step 4.

### Step 3: Drill down into high-risk callers

For any impacted symbol you need to understand in depth (unfamiliar caller, ambiguous usage, high centrality), use the targeted tools:

```
fast_refs(symbol="<caller>", include_definition=true, limit=100, workspace="<workspace_id>")
deep_dive(symbol="<caller>", depth="context", workspace="<workspace_id>")
```

If `deep_dive` returns the wrong symbol (common names like `new`, `result`, `config`), use `context_file` to disambiguate:

```
deep_dive(symbol="<caller>", context_file="<partial_file_path>", workspace="<workspace_id>")
```

### Step 4: Narrow long impact lists

If Step 2 ended with the truncation line, rerun `blast_radius` with a larger `limit`, a smaller `max_depth`, or a tighter seed (`symbol_ids` instead of `file_paths`).

### Step 5: Sample Deep Dives on High-Risk Callers

For each high-risk file surfaced by `blast_radius`, `deep_dive` on the calling function to understand HOW the symbol is used:
- Is it called with specific arguments?
- Does the caller depend on the return type?
- Is it used in error handling paths?

If you need the shortest route from one surfaced caller into a downstream sink or shared dependency, use `call_path` after `blast_radius`:

```
call_path(from="<impacted_symbol>", to="<downstream_symbol>", workspace="<workspace_id>")
```

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

### Step 6: Report

```
Impact Analysis: <change_target>
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
- **Cross-workspace**: Call `manage_workspace(operation="open", path="<path>")` first, then pass the returned ID as `workspace` to scoped calls; use `workspace_id` only for management operations that require it.
