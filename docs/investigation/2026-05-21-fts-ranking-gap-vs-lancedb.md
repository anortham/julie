# Julie FTS Ranking Gap vs Eros `lancedb-fts` — Investigation

**Date:** 2026-05-21
**Trigger:** In the multi-language bakeoff at
`~/source/eros/docs/eval/multi-lang-bakeoff-20260521.md`, Eros's `lancedb-fts`
candidate (no embeddings, pure lexical) scored **374/406 top1 / 0.945 MRR** vs Julie's
**267/406 top1 / 0.670 MRR** on the same fresh, untainted corpus — same 406 queries,
same 18 repos.

This is **not** a vector-vs-lexical story. Both systems are lexical. The point of this
doc is to figure out what's structurally different so Julie's ranking can catch up.

## TL;DR

1. **LanceDB FTS *is* Tantivy.** Specifically, LanceDB-Rust ships an embedded Tantivy
   FTS index. So the engine is the same as Julie's. The gap is not in the index
   technology.
2. **Julie fragments search by `target`.** `definitions` / `files` / `content` are
   three independent code paths, each querying a disjoint set of schema fields.
   Eros's `lancedb-fts` searches **all 7 indexed fields at once** for every query.
3. **Julie's content search is pure BM25 with no name/path/title signal.** When a
   query like `test requested redirect` is routed to `content`, Julie's ranker has no
   way to use the fact that a *symbol named* `testRequestedRedirect` exists in the
   right file. Eros's ranker uses that title signal heavily.
4. **The bakeoff harness picks the target per category.** That mapping
   (`exact symbol lookup → definitions`, `documentation phrase → content`,
   `test intent → files`) was reasonable but it shows up as ranking misses when the
   query's intent and the target's field set don't line up.
5. **On the most common loss pattern, Julie has the right document in its results
   — it just ranks it outside top 5.** This is a ranker problem, not a recall problem.

## Both index symbols *and* files — the difference is shape, not scope

A natural question on seeing this gap is "are we indexing too much, or the wrong
things?" Answer: both systems index the same raw material (tree-sitter symbols +
file content). The divergence is how it's organized into rows and queried.

**Julie / Tantivy — two distinct doc types:**
- `symbol` docs (one per tree-sitter symbol): name, signature, doc_comment,
  code_body, owner_names_text, kind, file_path, language
- `file` docs (one per source file): file_path, basename, path_text, content
  (full file text), language

Disjoint field sets. A symbol doc has no `content`. A file doc has no `name` /
`signature`.

**Eros / lancedb-fts — single unified `search_doc` schema with a `kind`
discriminator column** (`~/source/eros/python/eros/retrieval/search_docs.py`):
- One row per file (kind=`"file"`) AND one row per symbol
  (kind=`"function"` / `"class"` / etc.); both row variants live in the same
  table.
- Every row carries the *same* 7 FTS fields regardless of kind: `name_text`,
  `path_text`, `signature_text`, `doc_text`, `relationship_text`,
  `body_excerpt` (`body[:2000]`, truncated), `pretokenized_code`.
- `pretokenized_code` is built at index time by splitting `body` on
  non-alphanumeric characters and rejoining with spaces — this is how Eros
  captures CamelCase / snake_case splits without needing a custom matching
  tokenizer. The FTS tokenizer just sees pre-split tokens.
- File-vs-symbol differs only by which fields are populated and the `kind`
  value; never by which fields *exist*.

### The structural fork

- **Julie chose schema fragmentation.** Two row types with disjoint fields,
  surfaced to callers as `target=definitions|files|content`. Cleaner per-type
  schema, but it forces the caller to pick the right slice and locks each
  query into a subset of fields.
- **Eros chose schema unification.** One row type with a `kind` column, all
  fields queryable at once. Slightly more "wasted" storage (symbol rows still
  carry path/body excerpts), but the ranker sees every signal for every
  candidate in a single BM25 sweep.

### Where the over-engineering actually lives

The over-engineering — if there is any — is not in *what* gets indexed. Both
sides extract symbols, both sides keep file content. It's in **how it's split
into rows and queried**:

1. **Two doc types where one would do.** The symbol/file split forces every
   ranking decision through a target dispatch the caller has to guess.
2. **Tokenizer doing too much at query time.** Julie does CamelCase +
   snake_case + stemming + affix variants at *both* index and query time.
   Eros does CamelCase splitting at index time into a side field
   (`pretokenized_code`) and uses a `simple` tokenizer with no stemming for
   matching. Index-time-only splitting is doing better empirically.
3. **RRF × 200 rescale.** This constant exists in
   `src/tools/search/text_search.rs:507-515` to fuse three target-fragmented
   searches that should have been one. It's a symptom, not a bug.

### The cure isn't "index less"

- Collapse `symbol` and `file` into one row schema with a `kind` column.
- Drop stemming and CamelCase emission from the matching tokenizer; keep
  CamelCase splitting as an index-time-only side field, the way Eros does.
- Query all fields in one BM25 sweep; let the *ranker*, not the *target*,
  pick the winner.
- Add `title==query` / `basename==query` / `kind=="file" && stem==query`
  boosts to the reranker (item #2 in Recommended Fixes below).

This is the Eros recipe applied back to Julie. Tree-sitter symbol extraction
stays. Body indexing stays. The wins come from collapsing the doc-type split
and simplifying the matching tokenizer.

## Architecture: what's actually different

### Schema and fields

**Julie's Tantivy schema** (`src/search/schema.rs:53-98`):

| Field | Tokenizer | Indexed for which target |
| --- | --- | --- |
| `file_path` | `STRING` (raw) | files |
| `basename` | `STRING` (raw) | files |
| `path_text` | `code` | files |
| `name` | `code` | definitions |
| `signature` | `code` | definitions |
| `doc_comment` | `code` | definitions |
| `code_body` | `code` | definitions |
| `owner_names_text` | `code` | definitions |
| `annotations_exact` | `STRING` | definitions (filter) |
| `annotations_text` | `code` | definitions |
| `content` | `code` | **content** |

**Eros's `lancedb-fts` schema**
(`~/source/eros/python/eros/retrieval/lancedb_index.py:28-55`):

| Field | Tokenizer | Indexed for which target |
| --- | --- | --- |
| `name_text` | `simple`, lowercase, ascii fold | **all FTS queries** |
| `path_text` | `simple` | **all FTS queries** |
| `signature_text` | `simple` | **all FTS queries** |
| `doc_text` | `simple` | **all FTS queries** |
| `relationship_text` | `simple` | **all FTS queries** |
| `body_excerpt` | `simple` | **all FTS queries** |
| `pretokenized_code` | `simple` | **all FTS queries** |

Eros's FTS config (line 48):

```python
CODE_FTS_INDEX_OPTIONS = {
    "base_tokenizer": "simple",
    "lower_case": True,
    "stem": False,            # ← NO stemming
    "remove_stop_words": False,
    "ascii_folding": True,
    "max_token_length": 80,
}
```

Julie's `code` tokenizer (`src/search/tokenizer.rs:186-300+`) does *more*: it splits
CamelCase, splits snake_case, emits affix-stripped variants, prefix/suffix-stripped
variants, **and runs an English stemmer** on every token ≥ 4 chars. You might expect
that to help Julie. Empirically it's not pulling its weight, and there's a hypothesis
below for why.

### The target dispatch is the architectural fault line

Julie's `fast_search` requires you to pick a `search_target`:

- `definitions` → `text_search::text_search_impl` → queries `name`, `signature`,
  `doc_comment`, `code_body`, `owner_names_text` only
  (`src/search/index.rs:1403-1518` — `build_annotation_symbol_query`)
- `files` → `SearchIndex::search_files` → queries `file_path`, `basename`,
  `path_text` only (`src/search/index.rs:792-883`)
- `content` → `SearchIndex::search_content` → queries the `content` field only
  (`src/search/index.rs:713-790`)

Eros's `lancedb-fts` just calls `table.search(query, query_type="fts")` which fans
out across all 7 indexed fields, then re-ranks with field-weighted scoring (title /
path / basename exact-match boosts in
`~/source/eros/python/eros/retrieval/lancedb_index.py:449-494`).

**This is the biggest structural gap.** Julie chose to surface the target as a
first-class API parameter; Eros doesn't make the caller make that choice.

### The eval target mapping

The bakeoff harness uses this mapping
(`~/source/eros/python/eros/eval/compare.py:1467-1472`):

```python
def _julie_search_target(category):
    if category in {"exact symbol lookup", "symbol intent lookup"}:
        return "definitions"
    if category in {"file/path search", "likely test lookup", "test intent lookup"}:
        return "files"
    return "content"   # documentation phrase lookup
```

This mapping is reasonable on its face but it bakes in a brittle assumption: that
the corpus category is a reliable signal of *which Julie field set will contain the
answer*. The bakeoff data says it isn't.

## Concrete examples — Julie has the right answer but ranks it wrong

These are queries where `lancedb-fts` got top1 / top5 and Julie missed (rank > 5 or
nothing returned). All from the same May-21 bakeoff artifact at
`/Users/murphy/.eros-eval/eval/bakeoff/20260521T202136Z-956e68b80b0c.json`.

### Pattern A — Right answer, wrong rank (most common)

**`exact symbol lookup`** — repo `Alamofire`, query `displayTemplate`,
expected `docs/.../js/jazzy.search.js` or `docs/js/jazzy.search.js`:

- Julie (`definitions` target): the snippet shown is
  `docs/docsets/Alamofire.docset/Contents/Resources/Documents/js/jazzy.search.js:10
   (function...)` — i.e., the right file *is* in Julie's output, but it's not at
  rank ≤ 5. Latency: 319 ms.
- `lancedb-fts`: same file at rank 1, title `displayTemplate`.

Why this happens: Alamofire has 4–5 nearly identical copies of jazzy.search.js in
different docset paths. Both engines find all the copies. Eros's ranker uses
`title == query` and `kind=file && basename match` boosts (the
`_PARTIAL_DEFINITION_KIND_BOOST = 40` and the file-stem boost), so the copy whose
*symbol title* exactly equals `displayTemplate` wins. Julie's
`build_annotation_symbol_query` boosts the `name` field but doesn't have a strong
short-circuit for "exact title equality" — multiple `displayTemplate` symbols across
the copies have similar scores and one of the docset copies wins on BM25 length
normalization.

### Pattern B — Wrong target field set

**`test intent lookup`** — repo `express`, query `test requested redirect`,
expected `test/res.location.js`:

- Julie (`files` target → searches `file_path / basename / path_text` only):
  returns `test/res.redirect.js` first. The basename `res.redirect.js` happens to
  share a token (`redirect`) with the query, so file search ranks it above
  `res.location.js` — which is the right file but whose basename has nothing to do
  with the query.
- `lancedb-fts`: returns `test/res.location.js` at rank 1, title
  `testRequestedRedirect`. The body of `res.location.js` contains the Mocha test
  declaration `it('test requested redirect', ...)`, so the body+title hit wins.

Julie's `files` target *can't see* the test name `testRequestedRedirect` because
file search doesn't query name/signature/body/content fields.

### Pattern C — Documentation-phrase forced into content-only

**`documentation phrase lookup`** — repo `browser39`, query
`browser39 tools drop in web search`, expected `examples/browser39_tools.rs`:

- Julie (`content` target → only the `content` field): returns
  `examples/browser39_tools.py` (the Python sibling). Same file body content, but
  the wrong language variant.
- `lancedb-fts`: returns `examples/browser39_tools.rs` rank 1, title `std::fs`.
  Wins because Eros's pretokenized_code field includes path components — and the
  path `examples/browser39_tools.rs` shares more tokens with the query than the
  Python one's body does.

Julie's content search has no path-text signal, so it can't distinguish the .rs from
the .py variant when their content is similar.

### Pattern D — Symbol-intent in `definitions` target with cold-start cost

**`symbol intent lookup`** — same Alamofire jazzy examples as Pattern A but with
NL queries like `function display template`:

- Julie (`definitions`): the right file is in the results, but latency is **9.5
  seconds**. Two issues compounded: (a) query-term expansion in
  `expand_query_terms` may be producing a combinatorial explosion of alias/normalized
  candidates, and (b) the cold-start of `julie-server --standalone` per query
  amortizes badly. Even if the rank were right, this is unusable.
- `lancedb-fts`: 1 ms p50.

The latency outlier suggests this category is hitting a slow path that doesn't show
up on simpler symbol queries.

## What Eros's ranker does that Julie's doesn't

`~/source/eros/python/eros/retrieval/lancedb_index.py:449-494` defines:

```
_exact_field_score(row, normalized_query):
    if title == normalized_query: +100, +kind_boost (30-35 for funcs/classes)
    if kind == "file" and query in {basename, stem}: +120
    elif query in {path, basename, stem}: +80

_field_score(row, terms):
    for each term:
      title == term: +100
      term in title: +50 (+40 if single-term + definition kind)
      term in path: +25
      term == basename: +40
      kind == "file" and term == stem: +30
```

This is applied as a Python re-rank *after* the FTS retrieval. The FTS just gets
candidates. The re-ranker decides who wins.

Julie has reranking in `src/search/reranker.rs` and `src/search/scoring.rs`, but the
key gap is: those rerankers only run on the *definitions* path. `search_content` and
`search_files` return BM25-ordered results with no post-rank step except a
file-search-rank heuristic for the `files` target
(`src/search/index.rs:1619 file_search_rank`).

## Why Julie's tokenizer doesn't save it

Julie's `code` tokenizer is more aggressive (CamelCase + snake_case + stemming +
affix variants). You'd expect this to *help* recall for NL queries. It probably
does help recall. But two things bite:

1. **It also produces noisy index terms.** Stemming `redirect` → `redirect` is fine,
   but stemming `redirected`, `redirection`, `redirects` to the same root and
   emitting all camel-case and snake-case parts means the IDF of those tokens
   collapses across many documents. BM25 with collapsed IDF gives flat scores, and
   then ranking ties go to whichever document the index found first.
2. **The CamelCase token expansion is unbounded.** For `testRequestedRedirect` it
   emits 4 tokens (full + 3 parts), each with position metadata. For a long
   identifier like `AuthenticatedRequestRedirectMiddlewareHandler` it emits more.
   Documents with long identifiers get inflated term frequencies, distorting BM25.

This is a hypothesis — needs verification with an ablation that disables stemming
and CamelCase emission to see whether Julie's quality goes *up* or down. Eros's
choice (do CamelCase splitting at index-time into a separate `pretokenized_code`
field, but use a `simple` tokenizer for matching, with no stemming) avoids that.

## Recommended fixes, ordered by likely impact

### 1. Unify search across targets, or auto-fallback (BIGGEST LEVER)

The largest losses are queries that hit the wrong target. Two options:

- **Option A**: Add a "unified" target that fans out across name + path +
  signature + body + content in one BM25 query, with field boosts roughly
  equivalent to Eros's `_field_score`. Make it the new default. Keep
  `definitions/files/content` as explicit narrowing options for callers who want
  them.
- **Option B**: Keep targets but auto-fall-back. If `content` returns < N hits
  with the user's terms, retry as `definitions`; if `files` returns < N, fall
  back to `definitions`. Cheaper to implement, less clean.

Option A is what Eros does. It's a bigger refactor but it removes the structural
gap. Option B can be a stepping stone.

### 2. Add title-exact and basename-exact boost short-circuits

In `src/search/scoring.rs` (the reranker for definitions), and similarly in the
`files` and `content` paths, add a strong boost (e.g. +200) for `title == query`
and `basename == query`. Eros's data says these dominate scoring decisions and
they're worth more than the BM25 differences across duplicate-file candidates.

The most concrete gap: when the corpus has duplicate files (Alamofire docset
copies), the right copy is the one whose `title` equals the query, not the one
whose BM25 happens to be tighter. Eros uses `_exact_field_score = 100 + 30/35`
which fully dominates `_field_score`'s `25 path` / `40 basename` contributions.

### 3. Stop fragmenting content search from symbol context

`search_content` queries only `content`. The symbol's `name`, `doc_comment`,
`signature` live in the same workspace and could be a 2nd-pass scoring signal:
"this content match comes from a file that also contains a symbol named X" should
boost the result when X is in the query.

Concretely: for content hits, lift the symbol records that live in the same file
and add their title/name tokens to the scoring pool. Cheap (one extra index lookup
per content hit, batched).

### 4. Investigate the 9-second symbol-intent latency

`function display template` taking 9.5s is unacceptable independent of ranking.
Suspect: `expand_query_terms` combinatorial blowup with alias/normalized variants,
or AND-then-OR fallback re-running the entire query. Profile with a tracing
harness against an Alamofire-like repo. Even partial wins here reshape the p95
story.

### 5. Tokenizer ablation

Run the bakeoff again with two Julie variants:

- Julie-current (status quo)
- Julie-no-stemming (`ENGLISH_STEMMER.stem` returns input unchanged)
- Julie-no-camelcase (skip `split_camel_case` emission)

If quality goes up with either ablation, the aggressive tokenizer is a net negative.
If quality goes down, it's pulling its weight and the gap is purely ranking.

## What Eros's `lancedb-fts` is *not* doing that you might expect

To be fair, here's what Eros is *not* doing — useful for setting expectations:

- **Not splitting CamelCase at query time.** `displayTemplate` as a query is one
  token. It still wins because the symbol's title is *also* indexed as one token.
- **Not running a stemmer.** `stem: False` in the FTS options.
- **Not removing stopwords.** `remove_stop_words: False`.
- **Not using a fancy reranker.** `_field_score` and `_exact_field_score` are
  ~50 lines of simple counting and comparisons. Plus a stable sort.
- **Not using vectors at all in this candidate.**

The lesson is that *simple, multi-field FTS with strong title/basename
short-circuits* outperforms a clever tokenizer with fragmented search modes on this
corpus.

## Code map for follow-up

If you want to start fixing this, here's where to land:

- `~/source/julie/src/tools/search/execution.rs:51-90` — the target dispatch.
  Either add a `Unified` variant here, or change the existing variants to share a
  multi-field query builder.
- `~/source/julie/src/search/index.rs:713-790` (`search_content`) and
  `:792-883` (`search_files`) — these are the targets that don't see symbol
  context. Fastest wins live here.
- `~/source/julie/src/search/index.rs:1403-1518`
  (`build_annotation_symbol_query`) — definition query builder. Already
  multi-field, but boosts are spread across alias/normalized variants. Try a
  pre-boost short-circuit for `name == query`.
- `~/source/julie/src/search/scoring.rs` and `src/search/reranker.rs` — the
  definitions-target reranker. Add `title == query` and `basename == query` hard
  boosts here. Reuse the same logic in a `content`/`files` post-rank.
- `~/source/julie/src/tools/search/text_search.rs:507-515` — the RRF `* 200`
  rescale. Once vectors stop swamping or undershooting BM25, this constant should
  go away. It's not the bug, but it's a symptom of the same structural issue.

## Files referenced

- Bakeoff artifact: `/Users/murphy/.eros-eval/eval/bakeoff/20260521T202136Z-956e68b80b0c.json`
- Bakeoff report: `~/source/eros/docs/eval/multi-lang-bakeoff-20260521.md`
- Corpus: `/Users/murphy/.eros-eval/eval/multi-lang-corpus/20260521T185725Z-94fe62fedfd5.json`

## Scale addendum — openclaw (2026-05-21)

A follow-up bakeoff ran lancedb-fts, lancedb-coderank, zoekt-lexical, and codenav-mcp against
`~/source/openclaw` — a 16K-file, ~13K-TypeScript monorepo — to test whether the multi-lang
findings hold at scale. Julie was not re-run at this scale (deliberate scope).

**Artifact:** `/Users/murphy/.eros-eval/eval/bakeoff/20260521T223428Z-fc9e274a59ee.json`
**Eros bakeoff report:** `~/source/eros/docs/eval/multi-lang-bakeoff-20260521.md` (Scale
Addendum section)

Top1 by candidate (24 queries):

| Candidate | top1 | top5 | MRR | p50 ms |
| --- | --- | --- | --- | --- |
| lancedb-fts | 18 / 24 (75%) | 18 / 24 | 0.750 | 119 |
| lancedb-coderank | 17 / 24 (71%) | 21 / 24 (88%) | 0.792 | 328 |
| zoekt-lexical | 13 / 24 (54%) | 16 / 24 | 0.594 | 358 |
| codenav-mcp | 12 / 24 (50%) | 14 / 24 | 0.542 | 367 |

### Two findings that change the picture for Julie

**1. FTS-only cannot reach test-intent queries on this schema. Embeddings can.**

The `test intent lookup` category (4 queries; each query is a natural-language phrase that
identifies a test by its *body symbol semantics*, not its path) scored:

- lancedb-coderank: **2 / 4 top1**
- lancedb-fts: **0 / 4**
- zoekt-lexical: **0 / 4**
- codenav-mcp: **0 / 4**

Concretely, query `"test profile config"` is expected to find
`extensions/browser/src/browser/server-context.hot-reload-profiles.test.ts`. The file's *path*
shares no useful tokens with the query. The file's *FTS-indexed body* in Eros's schema is
synthesized by `_file_documentation_text` from at most 8 documentation phrases extracted from
the source — the actual test names (`describe("hot reload profile config", ...)` etc.) are not
in that field.

The query *can* match against the tree-sitter-extracted symbol rows for the test calls — but
only via semantic (vector) similarity. Pure FTS over the current schema can't reach those rows
because the symbol's `name_text` doesn't contain the literal query tokens either.

Implication for Julie's strategy:

- "Remove embeddings entirely" loses the test-intent category *and* whatever other body-symbol-
  semantic queries that schema doesn't surface via lexical fields. This is a real, measurable
  cost — not a hypothetical one.
- The lift is reachable two ways: (a) keep embeddings as the system for semantic queries,
  (b) restructure indexing so test-body symbol names land in a lexically-searchable field.
  Path (b) is cheaper at runtime but only works if you're willing to index symbol bodies for
  test files (which Julie already does via `code_body`, but it's gated behind the
  `definitions` target). If Julie unifies targets (Recommended Fix #1), this category may
  recover without embeddings — worth measuring after that change.

**2. lancedb-coderank pays for its semantic lift in build time, not query time.**

Build cost per candidate on openclaw:

| Candidate | Build | Index size |
| --- | --- | --- |
| zoekt-lexical | 15 s | 289 MB |
| lancedb-fts | 36 s | 1.4 GB |
| codenav-mcp | 60 s | 2.4 GB |
| lancedb-coderank | **57 min** | **3.5 GB** |

CodeRankEmbed had to process 668,825 symbol rows + 15,298 file rows on the user's local M-series
box (~684K rows total at 768d). At corporate-monorepo scale this is the actual binding
constraint, not query latency. For Julie's decision: if the embedding path is kept,
chunking + parallelized embedding or a smaller model are first-order engineering decisions, not
optimizations.

**3. zoekt at 19 KB/file is 5× smaller than lancedb-fts (95 KB/file), but 21 percentage points
worse on top1.** The current Eros schema costs ~5× more storage than zoekt's raw-file index but
buys 21pp of accuracy. Julie's index size, by comparison, sits between these two —
[[julie-sqlitevec-experiment]] noted the sqlite-vec variant was "extremely bloated" relative to
Tantivy, putting Julie's Tantivy index closer to zoekt than to lancedb on per-file footprint.

## Open questions worth a follow-up doc

- Does Julie's daemon mode (hot index, no per-query process cold-start) actually
  reach lancedb-fts latency? p95 9.5s is standalone; daemon should be much better
  but it's unverified at this corpus scale.
- Would replacing Julie's per-target dispatch with a unified search hurt the
  *definitions* target specifically (Julie's strongest)? The reranker is currently
  tuned for symbol queries.
- Is the corpus generator's `SOURCE_LANGUAGES` gap (no Java/Swift/C#/C++/Kotlin/
  Dart) hiding a place where Julie's tokenizer pays off? Possibly — Java/Kotlin
  have heavy CamelCase usage; Julie's camel-splitting might shine more there.
  Worth re-running once the generator is extended.
