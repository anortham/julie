---
name: dependency-graph
description: Show module dependencies by analyzing imports, exports, and cross-references between files
user-invocable: true
arguments: "<module_path>"
allowed-tools: mcp__julie__get_symbols, mcp__julie__fast_refs
---

# Dependency Graph

Analyze module dependencies by examining what a file imports and what depends on it. This replaces the removed `fast_explore(mode="dependencies")` tool.

## Process

### Step 1: Get Module Symbols

```
get_symbols(file_path="<module_path>", mode="structure", max_depth=1)
```

List all symbols (functions, structs, traits, imports) in the module.

### Step 2: Identify Imports (What This Module Depends On)

From the symbol list, extract all `use` / `import` statements. Group by source:
- **Internal crate**: `use crate::database::...`
- **External crate**: `use tantivy::...`, `use serde::...`
- **Standard library**: `use std::...`

### Step 3: Identify Exports (What Depends on This Module)

For each **public** symbol in the module, check who uses it:

```
fast_refs(symbol="<public_symbol>", include_definition=false, limit=20)
```

Group results by file to see which modules depend on this one.

### Step 4: Report

```
Module: <file_path>

Imports (depends on):
  Internal:
    - crate::database::SymbolDatabase (queries, storage)
    - crate::search::SearchIndex (Tantivy search)
    - crate::workspace::JulieWorkspace (workspace context)
  External:
    - tantivy (full-text search engine)
    - serde (serialization)
    - anyhow (error handling)
  Stdlib:
    - std::collections::HashMap
    - std::path::Path

Exports (depended on by):
  FastSearchTool → used by:
    - src/handler.rs (tool registration + routing)
    - src/tests/tools/search.rs (test suite)
  SearchResult → used by:
    - src/tools/deep_dive/mod.rs (result formatting)

Internal Only (not exported):
  - build_query() — private helper
  - format_results() — private helper

Summary:
  Imports: 3 internal, 3 external, 2 stdlib
  Exports: 2 public symbols used by 3 files
  Coupling: Medium (core handler depends on this)
```

## Important Notes

- **Focus on public API** — private symbols don't affect other modules
- **Count references** to gauge coupling — a module with 50+ external references is tightly coupled
- **Watch for circular dependencies** — if A imports B and B imports A, flag it
