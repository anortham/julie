---
name: type-flow
description: Trace how types flow through a function — parameters, transformations, and return types
user-invocable: true
arguments: "<function_name>"
allowed-tools: mcp__julie__deep_dive, mcp__julie__fast_refs
---

# Type Flow

Trace how types flow through a function by analyzing type signatures, transformations, and conversions. This replaces the removed `fast_explore(mode="types")` tool.

## Process

### Step 1: Deep Dive the Function

```
deep_dive(symbol="<function>", depth="full")
```

Extract the function signature, body, and type information.

### Step 2: Map the Type Pipeline

Trace each type from input to output:

1. **Input types**: What are the parameter types?
2. **Transformations**: Where does a type convert to another?
   - `.into()`, `From::from()`, `as` casts
   - Method calls that return different types
   - Destructuring / pattern matching
3. **Intermediate types**: What types exist between input and output?
4. **Output type**: What does the function return?

### Step 3: Find Type Definitions

For any non-obvious type, look it up:

```
deep_dive(symbol="<TypeName>", depth="overview")
```

Or for external references:

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

- **Follow the `?` operator** — it unwraps Result/Option, changing the effective type
- **Generic types**: Note type parameters (e.g., `Vec<T>` where T matters)
- **Trait objects**: `dyn Trait` or `impl Trait` — note what concrete types are used
- **Closures**: Types of closure parameters are often inferred, state what they are
