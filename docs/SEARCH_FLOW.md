# Julie Search Architecture

**Purpose**: Technical reference for Julie's Tantivy-based search engine
**Last Updated**: 2026-02-06
**Status**: Production (Tantivy full-text search)

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
     +---> SQLite (symbols, identifiers, relationships, types, files)
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

**Key files:**

| File | Responsibility |
|------|---------------|
| `src/search/index.rs` | `SearchIndex` struct, add/search/commit operations |
| `src/search/tokenizer.rs` | `CodeTokenizer` (CamelCase/snake_case splitting) |
| `src/search/schema.rs` | Tantivy schema definition and `SchemaFields` |
| `src/search/query.rs` | BooleanQuery construction with field boosting |
| `src/search/scoring.rs` | Post-search `important_patterns` boost |
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
  |     +-- route on search_target:
  |     |     "definitions" --> search_symbols()
  |     |     "content"     --> search_content()
  |     |
  |     +-- SearchIndex methods:          [src/search/index.rs]
  |           |
  |           +-- tokenize_query()        CodeTokenizer splits query
  |           +-- filter_compound_tokens() remove redundant compounds
  |           +-- build_symbol_query()    [src/search/query.rs]
  |           |   or build_content_query()
  |           +-- searcher.search()       Tantivy BooleanQuery execution
  |           +-- apply_important_patterns_boost()  [src/search/scoring.rs]
  |
  +-- post-processing:
        "definitions" --> enrich code_context from SQLite
        "content"     --> post-verify candidates against SQLite file content
  |
  +-- apply file_pattern glob filter
  |
  v
Results (Vec<Symbol>)
```

### Two Search Targets

**"definitions"** -- searches symbol documents (functions, classes, structs).
Tantivy returns ranked matches. Each result is enriched with `code_context`
from SQLite (Tantivy indexes `code_body` for search but does not store it).

**"content"** -- searches file content documents (grep-like). Tantivy acts as a
candidate retrieval stage (fetches 5x the limit). Each candidate is
post-verified against actual file content from SQLite to eliminate false
positives from `CodeTokenizer` over-splitting. For example, `"Blake3 hash"`
tokenizes to `["blake", "3", "hash"]`, which could match files containing
unrelated "3" and "hash" -- post-verification catches this.

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
all 31 language TOML files in `languages/*.toml`. These are embedded in the
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

## Performance

- **Query latency**: <5ms typical (single-tier, no fallback chain)
- **Search available**: Immediately after indexing (no background build phase)
- **Writer heap**: 50MB (configurable via `WRITER_HEAP_SIZE`)
- **Blocking context**: Tantivy uses `std::sync::Mutex` for the writer, so
  search operations run inside `tokio::task::spawn_blocking`

---

## Storage

```
.julie/indexes/{workspace_id}/
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
Reference workspaces are stored at `.julie/indexes/{ref_workspace_id}/tantivy/`.

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
