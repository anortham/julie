---
name: explore-area
description: Orient on a new codebase area using get_context for token-budgeted exploration. Use when the user asks "what does this module do", "explain this part", or wants to understand an unfamiliar area before making changes.
user-invocable: true
arguments: "<query or concept>"
allowed-tools: mcp__julie__get_context, mcp__julie__deep_dive, mcp__julie__get_symbols, mcp__julie__spillover_get
---

# Explore Area

Orient on an unfamiliar area of the codebase using Julie's `get_context` tool. This is the recommended starting point before any modification task — understand the area first, then dive in.

## Process

### Step 1: Get Token-Budgeted Context

```
get_context(query="<area, concept, or module name>")
```

Conceptual queries work well here, not just symbol names. For example, `get_context(query="error handling and retries")` will find relevant symbols by meaning via semantic search.

This returns:
- **Pivots**: The most relevant symbols with full code bodies
- **Neighbors**: Connected symbols with signatures only
- **File map**: Which files contain which symbols

The output is automatically token-budgeted — few results get deep context, many results get broad overview.

### Step 1a: Bias with task inputs (optional)

`get_context` accepts task-shaped inputs that bias pivot selection and neighbor ranking toward what the user is actually working on. Mix and match as relevant:

- **`edited_files=[...]`** — boost pivots and neighbors in those files. Use when the user has changes in progress and wants context around them.
- **`entry_symbols=[...]`** — explicit symbol entry points the user cares about. Pins those symbols as pivots even if search scoring wouldn't have surfaced them.
- **`stack_trace="..."`** — paste a stack trace; `file:line` references inside get parsed and bias pivot selection toward the frames involved.
- **`failing_test="<test file or test symbol>"`** — boost production symbols linked to that test via the test-linkage graph. Great for "why is this test failing?" orientation.
- **`max_hops=2`** — allow bounded second-hop expansion when first-hop is thin (fewer than 4 code neighbors). Default is 1. Use for sparsely-connected areas where the immediate neighborhood isn't enough.
- **`prefer_tests=true`** — let test-linked symbols compete for neighbor slots alongside production callers. Default is `false` (tests stay out of the neighbor budget). Flip on when the user is writing or debugging tests.

Example — task-shaped usage combining several inputs:

```
get_context(query="error handling in request path",
            edited_files=["src/handler.rs"],
            failing_test="test_request_rejects_invalid_auth",
            max_hops=2)
```

### Step 2: Identify Key Symbols

From the pivots, identify:
- **Entry points**: Public functions/methods that external code calls (high centrality score)
- **Core types**: Structs, enums, traits that define the area's data model
- **Utilities**: Private helpers that support the entry points

Centrality scores in the output indicate how well-connected each symbol is — high-centrality symbols are the important ones.

### Step 3: Drill Into Key Symbols (Optional)

For pivots that need more context (e.g., you need to see callers/callees):

```
deep_dive(symbol="<key_symbol>", depth="overview")
```

Use `context_file` if the symbol name is ambiguous:

```
deep_dive(symbol="<symbol>", depth="overview", context_file="<partial_path>")
```

### Step 4: Check File Structure (Optional)

For files with many symbols that weren't fully covered by `get_context`:

```
get_symbols(file_path="<file>", mode="structure", max_depth=1)
```

This shows the full outline without reading the file contents.

### Step 5: Report

```
Area: <concept/module>

Key Entry Points:
  - function_a (src/module.rs:45) — centrality: high
    Purpose: <what it does>
  - function_b (src/module.rs:120) — centrality: medium
    Purpose: <what it does>

Core Types:
  - StructA (src/types.rs:10) — <what it represents>
  - TraitB (src/traits.rs:5) — <what it defines>

File Map:
  src/module.rs — main implementation (function_a, function_b, helpers)
  src/types.rs — data types (StructA, EnumC)
  src/traits.rs — trait definitions (TraitB)

Dependency Flow:
  External callers → function_a → helper_1 → database queries
                   → function_b → TraitB implementations

Suggested Starting Point:
  <which file/function to read first for this task>
```

## Tips

- **Use `format: "readable"` for human-friendly output** with section separators and richer formatting:
  ```
  get_context(query="...", format="readable")
  ```
  The default format is compact (optimized for AI agent consumption).
- **Use `max_tokens` to control budget** if the default is too much or too little:
  ```
  get_context(query="...", max_tokens=1000)
  ```
- **Filter by language or file pattern** to narrow scope:
  ```
  get_context(query="...", language="rust", file_pattern="src/tools/**")
  ```
- **Cross-workspace**: Call `manage_workspace(operation="open", path="<path>")` first, then pass the returned `workspace_id` to all tool calls

## Paging long neighbor lists

For broad queries, `get_context` may return `spillover_handle=gc_xxx` in its response when the neighbor list didn't fit in the first page. Fetch the rest without re-running the query:

```
spillover_get(spillover_handle="gc_xxx")
```

Keep paging until the handle stops appearing. The same mechanism backs `blast_radius` spillovers (`br_xxx`).
