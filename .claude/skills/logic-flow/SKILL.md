---
name: logic-flow
description: Explain the logic flow of a function step-by-step by analyzing its implementation and call graph
user-invocable: true
arguments: "<function_name>"
allowed-tools: mcp__julie__deep_dive
---

# Logic Flow

Explain the logic flow of a function by examining its implementation, control flow, and callees. This replaces the removed `fast_explore(mode="logic")` tool.

## Process

### Step 1: Deep Dive with Full Depth

```
deep_dive(symbol="<function>", depth="full")
```

This returns the function's code body, callers, callees, and type information.

### Step 2: Analyze the Code

Read through the function body and identify:

1. **Entry**: Function signature, parameters, what they represent
2. **Guard clauses**: Early returns, validation, error checks at the top
3. **Setup**: Variable initialization, resource acquisition
4. **Core logic**: The main processing — what does this function actually DO?
5. **Control flow**: Branches (if/else, match), loops, error handling (? operator, try/catch)
6. **Callees**: What other functions are called and why
7. **Return**: What value is produced and what it means

### Step 3: Write Step-by-Step Explanation

```
Function: <name> (<file>:<line>)
Purpose: <1-sentence summary of what this function does>

Parameters:
  - param1: Type — what it represents
  - param2: Type — what it represents

Logic Flow:
  1. Validates input: checks X and Y, returns early with error if invalid
  2. Acquires database lock via get_connection()
  3. Fetches matching symbols from database (calls fetch_symbols)
  4. If query is provided:
     a. Builds Tantivy search query
     b. Searches index for matches (calls search_index)
     c. Ranks results by BM25 score
  5. Otherwise: returns all symbols sorted by name
  6. Formats results into CallToolResult and returns

Key Callees:
  - fetch_symbols(): Retrieves symbols from SQLite by workspace ID
  - search_index(): Runs Tantivy full-text search with code tokenizer

Error Handling:
  - Returns Err if workspace not initialized
  - Logs warning and continues if individual file parsing fails

Returns: Result<CallToolResult> — MCP response with formatted search results
```

## Important Notes

- **Don't just repeat the code** — explain the WHY, not just the WHAT
- **Focus on the interesting parts** — skip boilerplate, highlight decisions
- **For large functions (>100 lines)**: Summarize sections rather than explaining every line
- **For trait methods**: Note which implementations exist and which is being analyzed
