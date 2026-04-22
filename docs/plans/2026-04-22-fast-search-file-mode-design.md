# Fast Search File Mode Design

## Problem

Julie has two `fast_search` targets today:

- `definitions`, which finds symbols
- `content`, which finds file content and then verifies lines

That leaves a hole in the middle. Agents are using `fast_search` to hunt for files and paths, but the tool has no path-native mode. The mined live seed report in `artifacts/search-matrix/seeds-2026-04-22-live.json` shows path-shaped queries such as:

- `2026-04-21-search-quality-recovery-plan`
- `search-matrix-cases`
- `workspace_pool.get_or_init`
- `read_to_string(paths.daemon_state())`

Those queries are a bad fit for `content` and a worse fit for `definitions`. The result is familiar and dumb:

- content search returns `line_match_miss` on path-ish queries
- definitions search returns imports, namespaces, or symbol noise
- agents burn tokens on content snippets when they wanted a file list

Julie already indexes `file_path`, but only as an exact `STRING` field in `src/search/schema.rs`. Current query builders in `src/search/query.rs` do not search that field, and `file_pattern` in `fast_search` is only a scope filter, not a search target.

We need a first-class file search mode so agents can ask for files without pretending that file lookup is content search.

## Goal

Add a deterministic file search target to `fast_search` that:

1. finds indexed files by basename, relative path, path fragment, or glob-like query
2. returns token-lean file results, not symbol or content-shaped noise
3. keeps the existing `fast_search` API shape where that shape still makes sense
4. uses the Tantivy index as the primary retrieval engine
5. preserves Julie's relative Unix-style path contract

## Non-goals

- Do not add typo-fuzzy edit-distance search in v1.
- Do not make file search semantic or embedding-backed.
- Do not search unindexed or excluded files.
- Do not replace `get_symbols(file_path=...)` as the exact-file browse path.
- Do not auto-reroute content or definition searches into file mode in v1.

## Options Considered

### Option A: separate `find_files` tool

This keeps semantics clean, but it creates a second search entry point for something that users and agents already treat as search.

### Option B: add a new `fast_search` target

This keeps the mental model intact. One tool, multiple search targets, each with output shaped to the target.

### Option C: overload `content` with more path heuristics

This is a bad idea. It keeps the wrong abstraction and turns content search into a kitchen sink.

## Recommendation

Pick **Option B**.

Add `search_target="files"` as the canonical new target. Accept `search_target="paths"` as an alias at the API edge, then normalize it to `files` for execution, telemetry, and reporting.

`files` is the better canonical term because the result set is files. `paths` is still useful sugar, but it is the worse primary name because Julie already has call paths, workspace paths, and `file_pattern` in the same orbit.

## Design

### 1. Parse and validate `search_target`

Today `FastSearchTool.search_target` is a raw `String`, and the shared execution path in `src/tools/search/execution.rs` treats anything other than `"content"` as definitions search.

That is a trap. A typo such as `"defintions"` silently routes to the wrong engine.

Add a parsed target layer:

- `content`
- `definitions`
- `files`
- alias: `paths` -> `files`

Unknown targets should return a validation error instead of silently falling through.

This parser should live near `FastSearchTool` so tool validation, telemetry, and execution all agree on the canonical target name.

Use an enum, not another string special case:

- `SearchTarget::Content`
- `SearchTarget::Definitions`
- `SearchTarget::Files`

### 2. Reuse the existing parameter surface

File mode should keep the current `fast_search` shape:

- `query`
- `workspace`
- `language`
- `file_pattern`
- `exclude_tests`
- `limit`
- `return_format`

Parameter semantics for file mode:

- `query`
  File expression to search for. Supports basename (`line_mode.rs`), relative path (`src/tools/search/line_mode.rs`), path fragment (`tools/search/line_mode`), and glob-like queries (`**/Program.cs`, `src/**/*.rs`).
- `file_pattern`
  Hard scope filter. It intersects with `query`. It does not become a second query language.
- `language`
  Normal language filter on matching files.
- `exclude_tests`
  `None` resolves to `false` in file mode. Hiding test files during a file hunt is hostile.
- `limit`
  Keep current `fast_search` limit behavior in v1. Callers that want a wider disambiguation set can ask for a higher limit.
- `return_format`
  Supported in both `locations` and `full`.
- `context_lines`
  Invalid in file mode. Return a clear error instead of a silent no-op.

### 3. Keep the backend Tantivy-led

File search should be Tantivy-led, not SQLite-led.

Why:

- partial basename and path-fragment matching belongs in an inverted index
- ranking belongs in an inverted index
- path queries need the same workspace and language filtering model as the rest of `fast_search`

SQLite remains useful once the caller already knows the path and wants exact-file follow-up through tools such as `get_symbols(file_path=...)`.

### 4. Extend the file document schema

Current file docs in `src/search/index.rs` store:

- `file_path`
- `language`
- `content`

Add two file-search fields for file documents:

- `basename`
  exact basename, stored as `STRING | STORED`
- `path_text`
  tokenized path text, stored with the existing `code` tokenizer

Keep the existing `file_path` field:

- it remains the exact full relative path field
- delete-by-path keeps using it
- exact full-path matches should still query it

Why this split:

- `file_path` gives exact full-path lookup
- `basename` gives high-confidence exact filename hits
- `path_text` handles fragments such as `tools/search/line_mode`, `workspace_pool`, `Program.cs`, and `json_pointer`

#### Coverage rule

File mode should cover all indexed file rows, not only rows whose `content` is present.

Today `src/search/projection.rs` backfills file docs from `get_all_file_contents_with_language()`, which filters to `content IS NOT NULL`. That is fine for content search. It is the wrong contract for file search.

Fix the projection contract so file docs are emitted for every indexed file row:

- `file_path`, `basename`, and `path_text` are always populated
- `content` is populated when present and empty otherwise

This avoids a split-brain feature where a file exists in Julie's database but cannot be found by `search_target="files"`.

Schema change implications:

- `src/search/schema.rs` must add the new fields
- `SchemaFields` must expose them
- `src/search/index.rs` must populate them in `add_file_content`
- `src/search/projection.rs` and `src/database/files.rs` must stop treating file docs as content-only backfill input
- `SEARCH_COMPAT_MARKER_VERSION` in `src/search/index.rs` must bump because the schema changed
- no SQLite schema migration is required for v1 because the `files` table already stores `path` and `language`

### 5. Add a dedicated file query builder and search path

Add a new index method:

- `SearchIndex::search_files(query_str, filter, limit)`

This should search only file documents (`doc_type = "file"`).

#### Query handling

File mode should support four common shapes without inventing a tiny second search engine:

1. **Exact relative path**
   Example: `src/tools/search/line_mode.rs`
   Highest-confidence match. Query `file_path` directly.

2. **Exact basename**
   Example: `line_mode.rs`
   Query `basename` directly with a high boost.

3. **Path fragment or basename stem**
   Example: `tools/search/line_mode`, `search-matrix-cases`, `workspace_pool`
   Query `path_text` with tokenized terms.

4. **Glob-like query**
   Example: `**/Program.cs`, `src/**/*.rs`
   Use literal path tokens to retrieve candidates from `path_text` and `basename`, then post-filter the candidate set with the existing glob matcher from `src/tools/search/query.rs`.

This keeps v1 deterministic:

- exact string fields for exact path hits
- tokenized field for fragments
- existing glob matcher for glob semantics

No edit-distance fuzzy matching, no semantic expansion, no embedding fallback.

Do not route file mode through the current content verification path in `src/tools/search/text_search.rs`. That code verifies line matches against file content and synthesizes content hits as `Symbol` results. File mode needs its own retrieval and conversion lane.

#### Candidate fetch and filtering

File mode should over-fetch before post-filters so `file_pattern`, `exclude_tests`, or glob post-filtering does not starve the result set at small limits.

The current content pipeline already paid tuition on starvation. File mode should not repeat the joke.

#### Ranking

Use deterministic ranking tiers in this order:

1. exact relative path match
2. exact basename match
3. exact basename stem or suffix-path match
4. tokenized path-segment match
5. broader substring or glob-filtered candidate

Tie-breaks:

1. source-like before test, docs, and fixture paths using the language-agnostic helpers in `src/search/scoring.rs`
2. shorter relative path
3. lexicographic path

Do not reuse `src/utils/path_relevance.rs` for this. It bakes in `src` and `lib` substring rules that fight Julie's language-agnostic contract.

### 6. Add file-shaped execution and hit backing

Current search execution is shaped around:

- definitions -> `Symbol`
- content -> `LineMatch`

File mode needs its own backing type instead of smuggling file hits through symbol or line types.

Add:

- a file hit/backing type in `src/tools/search/trace.rs`
- `SearchExecutionKind::Files`

For telemetry and reporting, file hits should serialize as:

- `name`: basename
- `file`: relative Unix-style path
- `line`: `None`
- `kind`: `"file"`
- `language`: file language
- `score`: deterministic ranking score

Optional file-only annotation data such as `match_kind` can stay on the file backing type and be consumed by the formatter without bloating top-hit telemetry.

### 7. Format results for agents, not for dashboards

File mode output should be token-lean and parse-free.

#### `return_format="locations"`

Header plus one relative path per line:

```text
2 files for "line_mode.rs":
  src/tools/search/line_mode.rs
  src/tests/tools/search/line_mode.rs
```

No line numbers, no symbol kinds, no code snippets.

#### `return_format="full"`

Still keep it lean. One line per result, with terse annotations only when they help disambiguate:

```text
2 files for "line_mode.rs":
  src/tools/search/line_mode.rs (rust, exact basename)
  src/tests/tools/search/line_mode.rs (rust, exact basename, test)
```

Formatting rules:

- always relative Unix-style paths
- dedupe by file path
- no code context
- no absolute paths
- no platform-native separators

This means file mode can stay cheap even when `return_format` defaults to `full`.

### 8. Zero-hit behavior and hints

File mode does not need a fancy hint ladder in v1.

When file search returns nothing:

- say no indexed files matched the query
- mention that results are limited to indexed files
- mention excluded areas that often surprise users, such as `target/`, `node_modules/`, `.julie/`, and lockfiles
- when the query shape looks more like a symbol than a file lookup, suggest `search_target="definitions"`
- when the query shape looks conceptual, suggest `get_context` or `search_target="content"`

Trace behavior for v1:

- reuse `ZeroHitReason::TantivyNoCandidates` when no file candidates exist
- reuse `ZeroHitReason::FilePatternFiltered` when `file_pattern` removes all candidates after retrieval
- keep `hint_kind` unset unless a future path-specific hint is added

### 9. Telemetry and matrix impact

`src/handler/search_telemetry.rs` should normalize `paths` -> `files` before writing metadata so we do not split one feature into two buckets.

Telemetry intent classification should add a distinct file intent bucket such as `file_lookup`.

The search-matrix harness should gain:

- starter cases for `search_target="files"`
- file-shaped cases drawn from the mined live queries above
- cross-language cases for basename, relative path, glob, and test-file disambiguation

This keeps the new target under the same regression discipline as content and definitions.

### 10. Compatibility with Julie's path contract

The existing contract in `docs/RELATIVE_PATHS_CONTRACT.md` stays intact:

- store relative paths
- use Unix-style `/` separators
- keep identity on the stored path string

File mode must not normalize away case-distinct paths during dedupe.

## Implementation Impact Map

Core code likely changes in:

- `src/tools/search/mod.rs`
- `src/tools/search/execution.rs`
- `src/tools/search/formatting.rs`
- `src/tools/search/query.rs`
- `src/tools/search/text_search.rs`
- `src/tools/search/trace.rs`
- `src/search/schema.rs`
- `src/search/index.rs`
- `src/search/projection.rs`
- `src/database/files.rs`
- `src/handler/search_telemetry.rs`
- `xtask/src/search_matrix.rs`

Primary test areas:

- `src/tests/tools/search/lean_format_tests.rs`
- `src/tests/tools/text_search_tantivy.rs`
- `src/tests/tools/search/tantivy_index_tests.rs`
- `src/tests/integration/projection_repair.rs`
- `src/tests/core/handler_telemetry.rs`
- `xtask/tests/search_matrix_contract_tests.rs`

## Acceptance Criteria

- `fast_search` accepts `search_target="files"` and `search_target="paths"` aliasing to the same execution path.
- Unknown `search_target` values return a validation error instead of silently falling through.
- File search finds indexed files by basename, relative path, fragment, and glob-like query.
- `file_pattern`, `language`, and `exclude_tests` work in file mode with intersection semantics.
- `context_lines` is rejected for file mode.
- File results are token-lean and return relative Unix-style paths only.
- Exact path and exact basename matches outrank fragment matches.
- File mode covers indexed file rows even when `content` is absent.
- Schema/index compatibility is handled through an explicit compat-marker bump.
- Telemetry records the canonical `files` target and a distinct file intent bucket.
- Search-matrix coverage includes at least one file case in smoke and multiple file cases in breadth.

## Open Questions

These are implementation-shape questions, not product-shape blockers:

1. Should `path_text` use the existing `code` tokenizer or a dedicated path tokenizer?

Recommendation: start with `code`. It already understands punctuation-heavy code tokens and keeps v1 smaller. Add a dedicated tokenizer only if ranking data proves the split is needed.

2. Should file mode expose `match_kind` in telemetry top hits?

Recommendation: no for v1. Keep `match_kind` formatter-local unless the matrix or dashboard proves it is worth the extra surface area.

3. Should content zero-hit hints later suggest `search_target="files"` for path-like queries?

Recommendation: yes, but not in this design. Land the file mode first, then add promotion logic with fresh telemetry.
