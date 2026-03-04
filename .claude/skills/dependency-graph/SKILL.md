---
name: dependency-graph
description: Show module dependencies by analyzing imports, exports, and cross-references between files. Use when the user asks what a file imports, what depends on a module, how modules are coupled, or wants to understand the dependency structure of any part of the codebase.
user-invocable: true
arguments: "<module_path>"
allowed-tools: mcp__julie__get_symbols, mcp__julie__fast_refs, mcp__julie__fast_search, mcp__julie__get_context
---

# Dependency Graph

Analyze module dependencies by examining what a file imports and what depends on it.

## Process

### Step 1: Get Broad Context (Optional)

For a quick overview of what's connected to this module:

```
get_context(query="<module name or concept>")
```

The file map and neighbors in the response often reveal the dependency structure immediately.

### Step 2: Get Module Symbols

```
get_symbols(file_path="<module_path>", mode="structure", max_depth=1)
```

List all symbols (functions, structs, traits, imports) in the module.

### Step 3: Identify Imports (What This Module Depends On)

From the symbol list, extract all import/use statements. Group by source:
- **Internal project**: Imports from other modules within the same project
- **External libraries**: Third-party dependencies (crates, npm packages, pip packages, etc.)
- **Standard library**: Language-provided standard library imports

### Step 4: Identify Exports (What Depends on This Module)

For each **public** symbol in the module, check who uses it:

```
fast_refs(symbol="<public_symbol>", include_definition=false, limit=20)
```

Group results by file to see which modules depend on this one.

### Step 5: Report

```
Module: <file_path>

Imports (depends on):
  Internal:
    - database/SymbolDatabase (queries, storage)
    - search/SearchIndex (full-text search)
    - workspace/JulieWorkspace (workspace context)
  External:
    - tantivy (full-text search engine)
    - serde (serialization)
  Stdlib:
    - HashMap, Path

Exports (depended on by):
  FastSearchTool → used by:
    - handler.rs (tool registration + routing)
    - tests/tools/search.rs (test suite)
  SearchResult → used by:
    - tools/deep_dive/mod.rs (result formatting)

Internal Only (not exported):
  - build_query() — private helper
  - format_results() — private helper

Summary:
  Imports: 3 internal, 2 external, 2 stdlib
  Exports: 2 public symbols used by 3 files
  Coupling: Medium (core handler depends on this)
```

## Important Notes

- **Focus on public API** — private symbols don't affect other modules
- **Count references** to gauge coupling — a module with 50+ external references is tightly coupled
- **Watch for circular dependencies** — if A imports B and B imports A, flag it
- **Reference workspaces**: Pass `workspace: "<workspace_id>"` to all tool calls when analyzing a non-primary workspace
