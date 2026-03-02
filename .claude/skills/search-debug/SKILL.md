---
name: search-debug
description: Diagnose why a search returns unexpected results — analyze scoring factors, tokenization, and index health for dogfooding Julie's search quality
user-invocable: true
arguments: "<search_query> [expected_result]"
allowed-tools: mcp__julie__fast_search, mcp__julie__deep_dive, mcp__julie__get_context, mcp__julie__manage_workspace
---

# Search Debug

Diagnose why a Julie search returns unexpected results. Use this when a search misses an expected symbol, ranks it too low, or returns irrelevant results. Essential for dogfooding search quality.

## Process

### Step 1: Reproduce the Search

Run the exact search the user reported:

```
fast_search(query="<original_query>", search_target="definitions", limit=20)
```

Note what comes back — the top results, their scores, and what's missing.

Also try with `search_target="content"` to see if the symbol appears in line-mode search:

```
fast_search(query="<original_query>", search_target="content", limit=10)
```

### Step 2: Verify the Expected Symbol Exists

Search for the expected symbol directly:

```
fast_search(query="<expected_symbol_name>", search_target="definitions")
```

If it doesn't appear at all, the symbol may not be indexed.

### Step 3: Check Index Health

```
manage_workspace(operation="health", detailed=true)
```

Look for:
- Is the workspace indexed?
- Are there indexing errors?
- Is the file containing the expected symbol in the indexed file list?

### Step 4: Deep Dive the Expected Symbol

```
deep_dive(symbol="<expected_symbol>", depth="context")
```

Check:
- **Centrality (ref_score)**: Low-centrality symbols get less boost in search results. A ref_score of 0 means nothing references this symbol — it won't benefit from centrality boosting.
- **Symbol kind**: Is it a function, struct, variable? Variables and constants may score differently.
- **File path**: Is it in a test/docs/fixture path? These get a mild penalty from `NL_PATH_PENALTY` (0.95x).
- **Visibility**: Public symbols with important patterns (e.g., `pub fn`) get a 1.5x boost from `IMPORTANT_PATTERN_BOOST`.

### Step 5: Analyze Scoring Factors

Consider why the expected result might score lower than competitors:

**Tokenization issues:**
- Does the query tokenize to match the symbol? Julie's `CodeTokenizer` splits CamelCase and snake_case — `getUserData` becomes `[get, user, data]`, `get_user_data` also becomes `[get, user, data]`.
- Are there prefix/suffix stripping rules in the language config that might affect matching?
- Does the query contain words that stem differently? (English stemming: "running" → "run")

**Ranking issues:**
- Is a different symbol with higher centrality stealing the top spot? High-centrality symbols get a logarithmic boost.
- Is the expected symbol in a test file? Test-path symbols get a mild penalty for natural-language queries.
- Does a competitor symbol have an `IMPORTANT_PATTERN_BOOST` (public + significant kind) that the expected symbol lacks?

**Index issues:**
- Was the file recently added? It may need re-indexing: `manage_workspace(operation="refresh")`
- Is the symbol in a language Julie doesn't extract well?

### Step 6: Report

```
Search Debug: "<query>"
Expected: <symbol_name> (<file>:<line>)

Current Results (top 5):
  1. <symbol> (<file>) — why it scores high
  2. <symbol> (<file>) — why it scores high
  ...

Expected Symbol Analysis:
  Centrality: ref_score <N> (<high/medium/low>)
  Kind: <function/struct/etc>
  Visibility: <pub/private>
  File path: <path> (production/test/docs)

Diagnosis:
  <Root cause — one of: tokenization mismatch, centrality gap, path penalty,
   not indexed, wrong symbol kind, or genuinely correct ranking>

Scoring Factors at Play:
  - Centrality boost: <applies/doesn't apply> (ref_score: <N>)
  - Important pattern boost: <1.5x/none>
  - NL path prior: <boost/penalty/neutral>
  - Tokenization: query "<query>" → tokens [<tokens>], symbol "<name>" → tokens [<tokens>]

Recommendation:
  <What to fix — scoring bug, tokenizer config, or "ranking is correct">
```

## Important Notes

- **Not every "missing" result is a bug** — if the query is ambiguous, the correct result may legitimately rank lower than a more central symbol with the same name
- **Check tokenization first** — most search misses are tokenization issues (the query doesn't split/stem to match the symbol name)
- **Centrality is intentional** — well-connected symbols ranking higher is a feature, not a bug. Only flag it if the centrality score seems wrong
- **Reference workspaces**: Pass `workspace: "<workspace_id>"` to debug searches in a non-primary workspace
