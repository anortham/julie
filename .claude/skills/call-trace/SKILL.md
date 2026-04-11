---
name: call-trace
description: Trace the call path between two functions by following callers/callees. Use when the user asks how function A reaches function B, wants to see connections between symbols, or asks "what calls what."
user-invocable: true
arguments: "<start_function> <end_function>"
allowed-tools: mcp__julie__deep_dive, mcp__julie__fast_refs, mcp__julie__fast_search, mcp__julie__get_context
---

# Call Trace

Trace the call path from a starting function to an ending function.

## Process

### Step 0: Resolve Symbol Names

If the symbol names are approximate or ambiguous, resolve them first:

```
fast_search(query="<name>", search_target="definitions")
```

If the user describes behavior ("how does X reach Y") rather than naming symbols, use a conceptual search to find the entry points:

```
fast_search(query="payment validation", search_target="definitions")
```

Semantic search will find relevant symbols by meaning, not just name.

If `deep_dive` returns the wrong symbol (e.g., a common name like `new` or `process`), use the `context_file` parameter to disambiguate:

```
deep_dive(symbol="process", context_file="handler")
```

### Step 1: Orient (Optional)

If both symbols are in the same domain, get a broad view first:

```
get_context(query="<shared concept or module name>")
```

This may reveal the connection immediately from the pivots and neighbors.

### Step 2: Build Forward Call Graph (Start → End)

Starting from the start function, follow callees:

1. `deep_dive(symbol="<start>", depth="context")` — get callees list
2. For each callee:
   - If callee matches end function → **path found**
   - If callee already visited → skip (cycle)
   - If depth > 5 → stop (too deep)
   - Otherwise, `deep_dive(symbol="<callee>", depth="overview")` and recurse

Track the path as you go: `[start, callee_1, callee_2, ..., end]`

### Step 3: Try Reverse If Forward Fails

If forward search doesn't find a path, try backward from the end function:

1. `fast_refs(symbol="<end>", reference_kind="call")` — get callers
2. For each caller, check if it's reachable from start

### Step 4: Report Results

**Path found:**
```
Call path: start_fn → helper_fn → process_fn → end_fn (3 steps)

Details:
  1. start_fn (src/main.rs:45) calls helper_fn
  2. helper_fn (src/utils.rs:120) calls process_fn
  3. process_fn (src/handler.rs:78) calls end_fn (src/tools.rs:30)
```

**No path found:**
```
No call path found between start_fn and end_fn within 5 steps.

start_fn callees: [a, b, c]
end_fn callers: [x, y, z]

These don't overlap — the functions may not be connected.
```

## Important Notes

- **Max depth: 5** — deeper traces are usually noise
- **Track visited set** — avoid infinite loops from recursive calls
- **Prefer shortest path** — if multiple paths exist, the first found (BFS-like) is usually most direct
- **Generic names** (e.g., `new`, `from`, `into`) should be filtered — they create false connections
- **Cross-workspace**: Call `manage_workspace(operation="open", path="<path>")` first, then pass the returned `workspace_id` to all tool calls
