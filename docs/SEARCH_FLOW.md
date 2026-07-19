# Julie Search Architecture

**Purpose**: Technical reference for Julie's Tantivy-based search engine
**Last Updated**: 2026-07-18
**Status**: Production (Tantivy full-text search + graph centrality + stemming)

---

## Architecture Overview

Julie uses a single-tier search architecture built on Tantivy with a custom
`CodeTokenizer`. SQLite remains the source of truth for symbol metadata and
file content; Tantivy provides full-text search with code-aware tokenization.

```
Source Files
     |
     v
tree-sitter extraction
     |
     +---> shared normalization
     |         |
     |         +-- symbols, identifiers, relationships, types, files
     |         +-- source_regions, structural_facts, complexity_metrics
     |
     +---> SQLite (canonical normalized state)
     |         |
     |         | symbol/file data
     |         v
     +---> Tantivy Index (full-text search)
               |
               +-- SymbolDocument (name, signature, doc_comment, code_body, ...)
               +-- FileDocument   (file_path, content)
```

Both document types share one Tantivy index per workspace, distinguished by a
`doc_type` field (`"symbol"` or `"file"`).

### Shared Extractor Consumer Path

Full indexing, external extraction, and watcher replacement all use
`normalize_extraction_results`. That function classifies literals and test
roles, flattens type arguments, and carries `source_regions`,
`structural_facts`, and `complexity_metrics` into one `NormalizedExtractionData`
value. The canonical SQLite write replaces all domains atomically, so the
database cannot observe a partially updated extractor result. Tantivy
projection follows the committed canonical state.

`fast_search` remains Tantivy-backed for candidate retrieval. When `regions`
is present, only content mode is allowed: Julie parses a comma-separated list
of `comment`, `doc_comment` (or `docstring`), `string_literal`, and `embedded`,
loads matching `source_regions` from the selected workspace database, and
keeps query-matching lines whose 1-based line falls inside an allowed span.
Definition and semantic/hybrid region requests are rejected instead of
silently ignoring the filter.

**Key files:**

| File | Responsibility |
|------|---------------|
| `src/search/index.rs` | `SearchIndex` struct, add/search/commit operations |
| `src/search/tokenizer.rs` | `CodeTokenizer` (CamelCase/snake_case splitting) |
| `src/search/schema.rs` | Tantivy schema definition and `SchemaFields` |
| `src/search/query.rs` | BooleanQuery construction with field boosting |
| `src/search/scoring.rs` | Post-search `important_patterns` boost + centrality boost |
| `src/search/language_config.rs` | Per-language TOML config loader |
| `src/tools/search/text_search.rs` | MCP tool entry point (`text_search_impl`) |

---

## Search Flow

A search goes through these stages:

```
MCP Tool (fast_search)
  |
  v
text_search_impl()                       [src/tools/search/text_search.rs]
  |
  +-- get workspace + search_index
  |
  +-- spawn_blocking (Tantivy uses std::sync::Mutex)
  |     |
  |     +-- unified BM25 sweep across all FTS fields
  |     |     SearchIndex::search_unified()       [src/search/index.rs]
  |     |
  |     +-- SearchIndex methods:                  [src/search/index.rs]
  |           |
  |           +-- tokenize_query()        CodeTokenizer splits query (with stemming)
  |           +-- filter_compound_tokens() remove redundant compounds
  |           +-- build_unified_query()    [src/search/query.rs]
  |           +-- searcher.search()       Tantivy BooleanQuery execution (AND mode)
  |           +-- IF zero results AND multiple terms:
  |           |     retry with OR mode (Occur::Should)  ← OR-fallback
  |           |     set relaxed=true
  |           +-- apply_important_patterns_boost()  [src/search/scoring.rs]
  |
  +-- post-processing:
  |     +-- rerank_unified()              [src/search/scoring.rs]
  |           Eros-recipe field-score boosts (name > signature > doc_comment > body)
  |           centrality boost for well-connected symbols
  |     +-- apply file_pattern glob filter
  |     +-- enrich code_context from SQLite
  |
  v
Results (Vec<SearchHit>, relaxed: bool)   -- each hit carries `kind`
```

### Unified Search

`fast_search` performs a single BM25 sweep across all FTS fields (name,
signature, doc_comment, code_body, file_path, content, relationship_text)
and returns mixed-kind results. Each hit carries a `kind` field so callers
can filter by type if needed.

The single `rerank_unified` pass applies Eros-recipe field-score boosts
(name matches outweigh body matches) and a logarithmic centrality boost for
well-connected symbols. There is no separate routing step.

---

## CodeTokenizer

The `CodeTokenizer` is a custom Tantivy `Tokenizer` that understands code
naming conventions. It runs at both index time and query time, so the same
splitting rules apply to documents and queries.

### Splitting Rules

1. **Delimiters** -- Characters in `(){}[]<>,;"'!@#$%^&*+=|~/\`.-:` split
   tokens (hyphens, dots, and colons are included).

2. **Preserved patterns** -- Multi-character operators like `::`, `->`, `=>`,
   `?.`, `??`, `===` are kept as single tokens instead of being split.

3. **CamelCase splitting** -- `getUserData` emits `[getuserdata, get, user, data]`.
   Handles acronyms: `XMLParser` emits `[xmlparser, xml, parser]`.

4. **snake_case splitting** -- `get_user_data` emits `[get_user_data, get, user, data]`.

5. **Affix stripping** -- Language-specific meaningful affixes (e.g. Rust's
   `is_`, `try_`, `_mut`) produce additional tokens. `is_valid` emits `valid`.

6. **Prefix/suffix stripping** -- Type prefixes like C#'s `I` and suffixes like
   `Service` produce stripped variants. `IUserService` emits `userservice`.

All emitted tokens are lowercased. A `HashSet` prevents duplicate tokens from
overlapping splits.

### Cross-Convention Matching

Because both `getUserData` and `get_user_data` produce the atomic tokens
`[get, user, data]`, searching for either name finds both. The full compound
form is also emitted for exact-match scoring.

### Language Configuration

`CodeTokenizer` can be initialized from `LanguageConfigs`, which collects
preserve_patterns, meaningful_affixes, and strip_prefixes/strip_suffixes from
all language TOML files in `languages/*.toml`. These are embedded in the
binary via `include_str!`.

Example (`languages/rust.toml`):

```toml
[tokenizer]
preserve_patterns = ["::", "->", "=>", "?", "<>", "'", "#[", "#!["]
naming_styles = ["snake_case", "PascalCase", "SCREAMING_SNAKE_CASE"]
meaningful_affixes = ["try_", "into_", "as_", "from_", "to_", "is_", "has_", "_mut", "_ref"]

[variants]
strip_prefixes = []
strip_suffixes = ["_impl", "_trait", "_fn"]

[scoring]
important_patterns = ["pub fn", "pub struct", "pub enum", "pub trait", "impl", "async fn"]
```

---

## Index Schema

Defined in `src/search/schema.rs`. A single Tantivy index holds two document
types, distinguished by the `doc_type` field.

### SymbolDocument Fields

| Field | Tokenizer | Stored | Purpose |
|-------|-----------|--------|---------|
| `doc_type` | STRING (exact) | yes | Always `"symbol"` |
| `id` | STRING (exact) | yes | Unique symbol ID |
| `file_path` | STRING (exact) | yes | Relative file path |
| `language` | STRING (exact) | yes | Language name |
| `name` | code | yes | Symbol name (5x boost) |
| `signature` | code | yes | Full signature (3x boost) |
| `doc_comment` | code | yes | Documentation (2x boost) |
| `code_body` | code | **no** | Function body (1x boost, not stored) |
| `kind` | STRING (exact) | yes | Symbol kind (function, struct, etc.) |
| `start_line` | u64 | yes | Line number |

### FileDocument Fields

| Field | Tokenizer | Stored | Purpose |
|-------|-----------|--------|---------|
| `doc_type` | STRING (exact) | yes | Always `"file"` |
| `file_path` | STRING (exact) | yes | Relative file path |
| `language` | STRING (exact) | yes | Language name |
| `content` | code | **no** | Full file text (not stored) |

Fields using the `code` tokenizer get CamelCase/snake_case splitting. Fields
using `STRING` are matched exactly (no tokenization). The `code_body` and
`content` fields are indexed but not stored to save disk space -- `code_body`
is retrieved from SQLite when needed.

---

## Query Processing

### Tokenization and Compound Filtering

When a query arrives, `SearchIndex` processes it in two steps:

1. **`tokenize_query()`** -- Runs the query through the same `CodeTokenizer`
   used at index time. `"getUserData"` becomes `[getuserdata, get, user, data]`.

2. **`filter_compound_tokens()`** -- Removes snake_case compound tokens whose
   sub-parts are all present in the token list. This prevents AND-per-term
   logic from requiring partial compounds that don't exist in the index.

   **Why this matters:** Consider searching for `"search_term"`. The tokenizer
   produces `[search_term, search, term]`. The indexed symbol
   `search_term_one` was tokenized as `[search_term_one, search, term, one]`
   -- note there is no `search_term` token. If we require ALL query tokens via
   AND, the query would fail because `search_term` is absent from the index.
   By filtering it out (its parts `search` and `term` are both present), we
   get clean AND semantics on just the atomic parts.

### BooleanQuery Construction

**Symbol queries** (`build_symbol_query` in `src/search/query.rs`):

```
Must: doc_type = "symbol"
Must: language = <filter>         (if specified)
Must: kind = <filter>             (if specified)
Must: (for each term)
  Should: name    CONTAINS term   (boost 5.0x)
  Should: signature CONTAINS term (boost 3.0x)
  Should: doc_comment CONTAINS term (boost 2.0x)
  Should: code_body CONTAINS term (boost 1.0x)
```

Each search term must match in at least one field (AND across terms), but
within a single term the field matches are OR'd. This means searching
`"select best candidate"` requires all three tokens to be present, but each
can appear in any field.

**Content queries** (`build_content_query`):

```
Must: doc_type = "file"
Must: language = <filter>         (if specified)
Must: content CONTAINS term       (for each term, AND semantics)
```

---

## Language Config Scoring

After Tantivy returns results, `apply_important_patterns_boost()` applies a
post-search score multiplier based on per-language `important_patterns`.

For each result, if its `signature` field contains any pattern from the
result's language config, the score is multiplied by **1.5x**. Only one boost
is applied per result regardless of how many patterns match. Results are
re-sorted by score after boosting.

This causes public API symbols to rank higher than private implementation
details. For example, in Rust, `pub fn process()` gets a 1.5x boost over
a private `fn helper()`, because `"pub fn"` is in the important_patterns list.

---

## OR-Fallback Search

When a multi-term AND query returns zero results, `search_symbols` and
`search_content` automatically retry with OR semantics (`Occur::Should`).

**Flow:**
1. Build query with `require_all_terms=true` (AND mode)
2. Execute search
3. If results empty AND `terms.len() > 1`:
   - Rebuild query with `require_all_terms=false` (OR mode)
   - BM25 naturally ranks documents matching more terms higher
   - Set `relaxed=true` on the result
4. Tool output prepends: `NOTE: Relaxed search (showing partial matches...)`

This recovers from zero-result queries without degrading precision when AND
succeeds. Single-term queries are never relaxed.

**Key files:** `src/search/index.rs` (fallback logic), `src/search/query.rs`
(`require_all_terms` parameter), `src/tools/search/mod.rs` (relaxed indicator).

---

## Graph Centrality Boost

After Tantivy returns results, `apply_centrality_boost()` re-ranks them using
pre-computed `reference_score` values from the `symbols` table.

**Formula:**
```
boosted_score = score * (1.0 + ln(1 + reference_score) * CENTRALITY_WEIGHT)
```

`CENTRALITY_WEIGHT = 0.3`. Effect at different reference counts:

| reference_score | Boost factor |
|-----------------|-------------|
| 0 (no refs) | 1.0x (no change) |
| 5 | ~1.54x |
| 50 | ~2.18x |
| 100 | ~2.38x |

**Noise filtering:** Ubiquitous trait methods (`to_string`, `clone`, `fmt`,
`eq`, `new`, `default`, `from`, `into`, etc.) are skipped — their high ref
counts reflect language mechanics, not actual importance.
`CENTRALITY_NOISE_NAMES` in `src/search/scoring.rs` defines the blocklist.

**Score computation:** `compute_reference_scores()` runs after indexing
completes. Uses weighted aggregation: `calls=3x`, `implements/imports/extends=2x`,
`uses/references=1x`. Self-references excluded. Stored in `reference_score`
column (migration 009).

**Key files:** `src/search/scoring.rs` (`apply_centrality_boost`,
`CENTRALITY_NOISE_NAMES`), `src/database/relationships.rs`
(`compute_reference_scores`).

---

## English Stemming

The `CodeTokenizer` emits Snowball English stems as additional tokens alongside
exact tokens. This enables morphological matching: "estimation" and "estimator"
both produce stem "estim".

**Rules:**
- Stems are emitted per-segment after all other variants (CamelCase, snake_case, affixes)
- Minimum 4 characters (avoids noise from short identifiers)
- Stems that equal the original token are skipped (deduplication)
- Uses `rust-stemmers` crate with `LazyLock` singleton for zero-cost after first access

**Key file:** `src/search/tokenizer.rs` (stemming logic in `tokenize_code`).

---

## Performance

- **Query latency**: <5ms typical (single-tier, no fallback chain)
- **Search available**: Immediately after indexing (no background build phase)
- **Writer heap**: 256MB (8 threads at 32MB each; see `WRITER_HEAP_SIZE` in `src/search/index.rs`)
- **Blocking context**: Tantivy uses `std::sync::Mutex` for the writer, so
  search operations run inside `tokio::task::spawn_blocking`

### NL Definition Query Latency - In-Process MCP vs Standalone

**In-process MCP path** (normal MCP client path): NL `definitions` queries
(`is_nl_like_query` -> true) take **~100ms** against a 100k-symbol workspace
when the Tantivy index is already on disk and warm. The MCP session serves stdio
directly and can reuse the resident embedding host for embedding-backed work.

**Standalone mode** (`julie-server search ... --standalone`): Latency depends on
whether the Tantivy index is already on disk and whether the OS page cache is
warm:
- **No index on disk**: ~40–75s (full workspace indexing + Tantivy segment build + query).
- **Index on disk, cold OS cache**: ~2s (Tantivy mmaps all segments from disk).
- **Index on disk, warm OS cache**: ~100–200ms (normal).

**Key implementation note — embedding sidecar probe:**  
NL `definitions` queries trigger `maybe_initialize_embeddings_for_nl_definitions` in
`src/tools/search/nl_embeddings.rs`. In the MCP path, embedding-backed work goes
through the resident embedding host when available. In standalone mode without
the fix below it would call `create_embedding_provider()`, which probes and
launches the Python sidecar, costing **8-10 seconds** even when keywords are
sufficient.

The fix (`bootstrap_standalone_handler` in `src/cli_tools/mod.rs`) calls
`handler.mark_standalone_embedding_skipped()` immediately after indexing. This sets
`workspace.embedding_runtime_status` to `Some(...)`, satisfying the guard:
```rust
if workspace.embedding_runtime_status.is_none() { /* probe sidecar */ }
```
so the probe is never entered. Standalone mode degrades to keyword-only search, which
is correct — the sidecar would be torn down immediately after the one-shot query anyway.

**Profiling evidence (2026-05-21, Alamofire ~520 files / 100k symbols, debug build):**

| Path | Latency |
|------|---------|
| In-process MCP, query "function display template" | ~100ms avg |
| `search_symbols` internals (expand + AND search + OR fallback) | ~1ms |
| `expand_query_terms("function display template")` | ~340µs |
| AND pass (Tantivy search, 160 candidate limit) | ~800µs |
| Standalone (pre-fix, warm cache): sidecar probe | ~8.6s of 9s total |
| Standalone (post-fix, warm cache): no sidecar probe | ~130ms |

**Invariant (tested in `src/tests/tools/search/nl_symbol_query_latency_tests.rs`):**  
"NL multi-token symbol-intent queries do not trigger combinatorial expansion."
`expand_query_terms` for a k-word NL query produces O(k) terms bounded by `MAX_ADDED_TERMS`.
The AND/OR fallback adds at most one extra Tantivy search call.

---

## Storage

**In-process MCP path** (shared by sessions under `$JULIE_HOME`):
```
$JULIE_HOME/indexes/{workspace_id}/
  ├── leader.lock              # Per-workspace writer election
  ├── db/
  │   └── symbols.db           # SQLite (symbols, files, relationships, types)
  └── tantivy/
      ├── meta.json            # Tantivy index metadata
      ├── {segment_id}.fast    # Fast fields (stored data)
      ├── {segment_id}.idx     # Inverted index
      ├── {segment_id}.pos     # Positions
      ├── {segment_id}.store   # Doc store
      └── {segment_id}.term    # Term dictionary
```

The registry of all known workspaces and dashboard-visible metrics lives in
`$JULIE_HOME/registry.db`.

**Standalone CLI path** (per-project, no shared registry):
```
<project>/.julie/indexes/{workspace_id}/
  ├── db/
  │   └── symbols.db           # SQLite (symbols, files, relationships, types)
  └── tantivy/
      ├── meta.json            # Tantivy index metadata
      ├── {segment_id}.fast    # Fast fields (stored data)
      ├── {segment_id}.idx     # Inverted index
      ├── {segment_id}.pos     # Positions
      ├── {segment_id}.store   # Doc store
      └── {segment_id}.term    # Term dictionary
```

Each workspace (primary and reference) gets its own Tantivy index directory.
See `docs/OPERATIONS.md` for the `JULIE_HOME` override and migration workflow.

The `SearchIndex` supports `open_or_create` semantics -- if a Tantivy
directory doesn't exist, it creates one. If it exists, it opens it. A v1-to-v2
backfill path (`backfill_tantivy_if_needed`) reads symbols and files from
SQLite when the Tantivy index is empty.

---

## Debugging

### Health Check

Use the `manage_workspace` MCP tool with operation `"health"` and
`detailed: true` to check index status, document counts, and diagnostics.

### Log Location

Logs are per-project, not per-user:

```bash
# Today's log
tail -f .julie/logs/julie.log.$(date +%Y-%m-%d)

# Search-related entries
tail -100 .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -i tantivy

# Recent errors
tail -100 .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -i error
```

### SQLite Verification

```bash
# Check symbol count
sqlite3 .julie/indexes/{workspace_id}/db/symbols.db "SELECT COUNT(*) FROM symbols;"

# Check file count
sqlite3 .julie/indexes/{workspace_id}/db/symbols.db "SELECT COUNT(*) FROM files;"
```

### Tantivy Index Verification

The `SearchIndex::num_docs()` method returns the total document count (both
symbol and file documents). Use the health check tool to see this value.
