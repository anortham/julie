---
name: type-flow
description: Trace how types flow through a function — parameters, transformations, and return types. Use when the user asks what types a function accepts or returns, how data transforms through a pipeline, or wants to understand type conversions in a code path.
user-invocable: true
arguments: "<function_name>"
allowed-tools: mcp__julie__deep_dive, mcp__julie__fast_refs, mcp__julie__fast_search, mcp__julie__manage_workspace
---

# Type Flow

Trace how types flow through a function by analyzing type signatures, transformations, and conversions.

## Process

### Step 1: Deep Dive the Function

```
deep_dive(symbol="<function>", depth="full")
```

Extract the function signature, body, and type information.

If `deep_dive` returns the wrong symbol, use `context_file` to disambiguate:

```
deep_dive(symbol="<function>", depth="full", context_file="<partial_file_path>")
```

### Step 2: Map the Type Pipeline

Trace each type from input to output:

1. **Input types**: What are the parameter types?
2. **Transformations**: Where does a type convert to another?
   - Conversion methods (Rust: `.into()`, `From::from()`; JS/TS: type assertions; Python: constructors)
   - Method calls that return different types
   - Destructuring / pattern matching / spread operators
3. **Intermediate types**: What types exist between input and output?
4. **Output type**: What does the function return?

### Step 3: Find Type Definitions

For any non-obvious type, look it up with a quick definition search:

```
fast_search(query="<TypeName>", search_target="definitions")
```

Or for a deeper look at the type's structure:

```
deep_dive(symbol="<TypeName>", depth="overview")
```

To see how a type is used across the codebase:

```
fast_refs(symbol="<TypeName>", reference_kind="type_usage", limit=10)
```

### Step 4: Report

```
Function: <name>

Type Flow:

  Input:
    self: &JulieServerHandler
    params: FastSearchTool { query: String, limit: Option<u32> }

  Pipeline:
    1. FastSearchTool → extract query: String, limit: Option<u32>
    2. String → tantivy::Query (via QueryParser::parse_query)
    3. Query → Vec<(DocAddress, f32)> (via Searcher::search)
    4. DocAddress → TantivyDocument (via Searcher::doc)
    5. TantivyDocument → Symbol (via database lookup)
    6. Vec<Symbol> → String (via format_results)
    7. String → CallToolResult (via CallToolResult::text_content)

  Output:
    Result<CallToolResult> — MCP response wrapper

  Key Type Definitions:
    - FastSearchTool: src/tools/search.rs:15 (tool parameter struct)
    - CallToolResult: rmcp crate (MCP protocol response)
    - Symbol: src/extractors/mod.rs (code symbol representation)

  Complexity: Medium (4 type transformations)
```

## Important Notes

- **Follow error propagation** — Rust's `?`, JS/TS `try/catch`, Python `raise` — these change the effective type in the success path
- **Generic types**: Note type parameters (e.g., `Vec<T>` where T matters)
- **Trait objects**: `dyn Trait` or `impl Trait` — note what concrete types are used
- **Closures**: Types of closure parameters are often inferred, state what they are
- **Cross-workspace**: Call `manage_workspace(operation="open", path="<path>")` first, then pass the returned `workspace_id` to all tool calls
