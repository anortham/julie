---
name: checkpoint
description: Save developer context to Julie memory — checkpoint at meaningful milestones, not after every action
allowed-tools: mcp__julie__checkpoint
---

# Checkpoint — Save Developer Memory

## When to Checkpoint

Checkpoint at **meaningful milestones** — think git commits, not keystrokes.

- **Completed a deliverable** — feature slice, bug fix, refactor step
- **Made a key decision** — architecture, tradeoffs, approach choices that future sessions must follow
- **Before context compaction** — preserve active state (PreCompact hook handles this automatically)
- **Found something non-obvious** — blockers, root causes, discoveries worth preserving
- **User shared requirements/constraints** — preserve what future work must honor

## When NOT to Checkpoint

- After every small edit or file change
- After routine test runs that pass
- Multiple times for the same piece of work
- Rapid-fire — if you checkpointed in the last few minutes, you probably don't need another
- With nearly identical descriptions to a recent checkpoint (4 checkpoints in 2 minutes about "embeddings fix" is not helpful)

One checkpoint per completed milestone. Not one per action.

## How to Write Good Descriptions

Your description becomes the **markdown body** of a `.md` file. Format it with structure — headers, bullet points, bold, code spans.

### The WHAT/WHY/HOW/IMPACT Formula

```
mcp__julie__checkpoint(
  description: """
## Implemented weighted RRF scoring for hybrid search

**Problem:** BM25 and embedding results had equal weight, causing keyword-perfect
matches to rank below semantically-similar-but-wrong results.

**Solution:** Added configurable weights to `rrf_merge()` — BM25 gets 0.7, embeddings
get 0.3 by default. Weights are per-workspace configurable via `search_config.toml`.

**Key change:** `src/search/merge.rs` — `rrf_merge()` now accepts `&RrfWeights` param
instead of using hardcoded k=60.
""",
  type: "checkpoint",
  tags: ["search", "rrf", "scoring", "hybrid-search"],
  symbols: ["rrf_merge", "RrfWeights", "SearchConfig"],
  impact: "Search relevance improved — keyword matches now consistently rank top-3",
  next: "Wire weights into manage_workspace tool so users can tune per-project"
)
```

### Good vs Bad

**Good** — structured, searchable, captures meaning:

```
mcp__julie__checkpoint(
  description: """
## Fixed phantom duplicate results in multi-workspace search

**Root cause:** `dedup_results()` compared by `(file_path, line)` but reference
workspaces store paths relative to their own root, not the primary workspace root.
Two different symbols at `src/lib.rs:42` in different workspaces looked identical.

**Fix:** Changed dedup key to `(workspace_id, file_path, line)` in `search/dedup.rs`.
Added regression test `test_cross_workspace_dedup_no_false_positives`.
""",
  type: "incident",
  tags: ["search", "dedup", "multi-workspace", "bug-fix"],
  symbols: ["dedup_results"],
  context: "Found during dogfooding — searched for 'TokenStream' and got 3 identical-looking results",
  evidence: ["search/dedup.rs:dedup key was (file_path, line)", "test_cross_workspace_dedup_no_false_positives"]
)
```

**Bad** — vague, unsearchable, no future value:

```
mcp__julie__checkpoint(
  description: "Fixed a bug in search dedup",
  tags: ["bug"]
)
```

**Bad** — restates the obvious without adding meaning:

```
mcp__julie__checkpoint(
  description: "Updated dedup_results function in search/dedup.rs to use workspace_id",
  tags: ["update"]
)
```

## Structured Fields

Use `type` to classify your checkpoint for better searchability:

- `type: "decision"` — include `decision` + `alternatives`
- `type: "incident"` — include `context` + `evidence`
- `type: "learning"` — include `impact`
- `type: "checkpoint"` — general milestone (default)

All types benefit from `symbols`, `next`, and `impact`.

### Decision Example

```
mcp__julie__checkpoint(
  description: """
## Chose Tantivy over FTS5 for full-text search

Tantivy provides code-aware tokenization (CamelCase/snake_case splitting),
BM25 scoring, and boolean query support out of the box. FTS5 required custom
tokenizers via C extensions and couldn't do per-field boosting.
""",
  type: "decision",
  decision: "Use Tantivy as the full-text search engine, replacing SQLite FTS5",
  alternatives: ["Keep FTS5 with custom tokenizer", "Use MeiliSearch as sidecar", "Use PostgreSQL with pg_trgm"],
  tags: ["search", "tantivy", "architecture"],
  impact: "Eliminates C FFI dependency, enables code-aware tokenization natively in Rust"
)
```

### Learning Example

```
mcp__julie__checkpoint(
  description: """
## BooleanQuery `Should` clauses are ignored when `Must` clauses exist

Tantivy's BooleanQuery treats `Should` as optional when any `Must` clause is present.
To get OR behavior alongside required clauses, wrap the OR terms in a nested
BooleanQuery with all-Should clauses, then add that nested query as a single Must.
""",
  type: "learning",
  tags: ["tantivy", "boolean-query", "gotcha"],
  symbols: ["BooleanQuery"],
  impact: "Affects all compound queries — must use nested structure for OR-within-AND"
)
```

## Tags — Think About Future Search

Tags power search recall. Julie uses **BM25 full-text search via Tantivy**, which means tags are tokenized and matched with relevance scoring — exact and stemmed matches both work well.

**Write tags for discoverability:**

| Good tags | Why |
|-----------|-----|
| `["search", "tantivy", "scoring", "bm25"]` | Specific, varied, multiple entry points |
| `["workspace", "isolation", "routing", "multi-project"]` | Covers the concept from different angles |
| `["tree-sitter", "rust-extractor", "symbol-extraction"]` | Technical + domain terms |

| Bad tags | Why |
|----------|-----|
| `["fix"]` | Too generic — matches everything, distinguishes nothing |
| `["code", "update"]` | Meaningless in a code project |
| `["search", "search-fix", "search-update", "search-change"]` | Redundant — BM25 will match "search" regardless of suffix |

**Tip:** Include the **domain** (search, workspace, extractor), the **technology** (tantivy, tree-sitter, sqlite), and the **action** (scoring, routing, tokenization). Three dimensions make recall reliable.

## What Gets Captured Automatically

You don't need to include these — Julie captures them:
- Git branch, commit, changed files
- Timestamp (UTC)

Focus your description on the **MEANING**, not the mechanics.
