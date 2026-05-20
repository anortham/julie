# SQLite FTS5 Search A/B Replacement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Prove whether SQLite FTS5 trigram, prose FTS, pre-tokenized code FTS, and existing sqlite-vec vectors can replace Tantivy with better or equal search quality and better runtime performance.

**Architecture:** Keep Tantivy as the production baseline while building a parallel SQLite search projection behind an explicit backend selector. The SQLite candidate uses multiple FTS tables instead of one tokenizer for everything: trigram for substrings and paths, pre-tokenized code FTS for symbols/code, prose FTS for markdown/docs/web pages, and sqlite-vec for semantic retrieval. The backend selector must sit at the public `fast_search` execution layer, not only inside `text_search_impl`, because definitions, files, and line-mode content search currently travel through different code paths. Tantivy is removed only after a machine-enforced A/B harness shows no quality regression and a real latency or operational improvement.

**Tech Stack:** Rust, rusqlite, SQLite FTS5 `trigram`, SQLite FTS5 `unicode61`/`porter`, existing Julie code tokenizer output, existing sqlite `vec0` symbol vectors, existing Markdown extractor, existing `projection_states` readiness tracking, xtask search ablation harness, existing search dogfood bucket.

**Architecture Quality:** Affected modules are search projection, query construction, ranking, markdown/document indexing, SQLite migrations/projection setup, workspace indexing, search execution/routing, health/readiness reporting, and eval tooling. Caller-facing interfaces are `fast_search`, `execute_search`, line-mode content search, file search, definition search, search telemetry, health projection state, and xtask A/B artifacts. Architecture risk is high because search semantics are product-critical; this plan keeps Tantivy and SQLite side-by-side until measured, mechanically-enforced evidence justifies a cutover.

---

## Current Evidence And Design Constraints

Julie currently has three relevant systems:

- Tantivy lexical search:
  - `src/search/index.rs`
  - `src/search/query.rs`
  - `src/search/schema.rs`
  - `src/search/tokenizer.rs`
  - `src/search/projection.rs`
  - `src/tools/search/execution.rs`
  - `src/tools/search/line_mode.rs`
  - `src/tools/search/text_search.rs`
- SQLite structured storage and vectors:
  - `src/database/vectors.rs`
  - `src/database/migrations.rs`
- Markdown/document extraction:
  - `crates/julie-extractors/src/markdown/mod.rs`

Tantivy is not just storage. It provides fielded search, weighted queries, AND/OR fallback, tokenizer compatibility, exact path/name behavior, file/content candidate discovery for line mode, and result objects consumed by the reranker. The SQLite replacement must preserve those product behaviors before Tantivy can be removed.

Current public routing matters:

- `fast_search(search_target="definitions")` routes through `execute_search` into `text_search_impl`.
- `fast_search(search_target="files")` routes through `execute_file_search`, which currently requires a Tantivy `SearchIndex`.
- `fast_search(search_target="content")` routes through line mode, which currently uses Tantivy `search_content` to discover candidate files before collecting exact lines from SQLite file contents.

The SQLite backend must cover all three public targets or be explicitly marked unavailable for the target. A/B evidence that only exercises definition search is not sufficient for replacement.

Current eval harness constraints:

- The existing command is `cargo xtask eval ablation`, not `cargo xtask eval search-ablation`.
- The current parser accepts only `--corpus`, `--out`, and `--limit`.
- The current corpus rows do not include `search_target`, so non-file-path rows are routed through definition search.
- The current default fixture DB does not provide vector-backed semantic evidence; vector/hybrid gates need an explicit source DB or workspace with populated `symbol_vectors`.

Eros bakeoff evidence from 2026-05-19 suggests the SQLite direction is plausible:

- SQLite FTS5 trigram alone: strong exact/path behavior, but weaker symbol/test-intent and some conceptual queries.
- SQLite trigram + sqlite-vec + PyTorch BGE/MPS: better top1 and MRR than the lexical-only candidate on a held-out 24-query set.
- FastEmbed is not the PyTorch/MPS path; it uses ONNX Runtime. For Apple Silicon acceleration, use a PyTorch/SentenceTransformers provider.

The plan below treats these as directional evidence only. Julie must pass its own A/B gates before changing production behavior.

## Key Risks And Recommendations

### 1. Database Size & Write Amplification (The "Bloat" Risk)
Materializing raw, pre-tokenized, and trigram data across five virtual tables (`search_symbols_code_fts`, `search_symbols_trigram_fts`, `search_files_trigram_fts`, `search_docs_prose_fts`, and `search_docs_trigram_fts`) will cause significant SQLite database growth.
* **Recommendation:** Use an explicit projection row identity and avoid duplicate raw text. First choice is FTS5 **contentless-delete** tables with explicit `rowid = search_projection_docs.projection_rowid`. If external-content tables are used instead, point them at projection row tables whose columns exactly match the FTS table, not directly at `symbols` or `files`. The proposed token/raw columns do not exist on the current canonical tables, so `content='symbols'` / `content='files'` would be wrong unless the schema is redesigned to make those columns real.

### 2. Multi-Surface Query Latency
Running up to five separate FTS5 queries (`SELECT` statements across code, trigram, prose, and paths) and merging them in Rust via Reciprocal Rank Fusion (RRF) could introduce noticeable performance overhead.
* **Recommendation:** Implement **query classification heuristics** to prune unnecessary table scans (e.g., skip path-trigram search if the query doesn't look like a path, or skip prose FTS if the query is purely code). If needed, group queries into combined statements (`UNION ALL`) to allow the SQLite planner to optimize execution batches.

### 3. SQLite Capability Drift
Julie currently compiles SQLite from source through `rusqlite` with the `bundled` feature, and the bundled `libsqlite3-sys` build enables `SQLITE_ENABLE_FTS5`. Current bundled SQLite also supports the `trigram` tokenizer and `tokenize='porter unicode61'`.
* **Recommendation:** Keep a runtime capability probe anyway. It should verify FTS5, `trigram`, `porter unicode61`, and `contentless_delete=1`, then report a precise unavailable reason for the SQLite backend. Do not frame host SQLite variance as the normal CI risk unless the build stops using bundled SQLite.

## Non-Goals

- Do not remove Tantivy in the first implementation phase.
- Do not replace Julie's code tokenizer with SQLite trigram only.
- Do not rewrite the Markdown extractor.
- Do not make web-search markdown documents second-class search data.
- Do not ship a custom SQLite tokenizer extension before the pre-tokenized-code-table approach has been measured.
- Do not declare victory from generated or in-sample corpora alone.

## Target Search Model

Use several SQLite retrieval surfaces, each optimized for a different query shape.

| Surface | SQLite table | Tokenizer/input | Purpose |
| --- | --- | --- | --- |
| Code lexical | `search_symbols_code_fts` | Julie `CodeTokenizer` output materialized as plain text tokens | Symbol names, signatures, code body terms, identifier splits, affix/stem variants |
| Code trigram | `search_symbols_trigram_fts` | raw name/signature/docs/body/path text, `tokenize='trigram'` | substring, partial identifier, exact code fragments |
| File path trigram | `search_files_trigram_fts` | normalized path, basename, path token text, `tokenize='trigram'` | path and filename search |
| Prose docs | `search_docs_prose_fts` | title, heading path, body, URL/domain metadata, `unicode61` or `porter` | markdown docs and web pages as prose |
| Doc trigram | `search_docs_trigram_fts` | raw section text, `tokenize='trigram'` | exact quotes, API names inside docs, punctuation-heavy snippets |
| Semantic | existing `symbol_vectors` plus a doc-section vector table if needed | existing embedding provider | conceptual search over code symbols and document sections |

The first version should populate the code-token FTS table by calling Julie's existing Rust tokenizer and storing emitted tokens as whitespace-separated text. That avoids SQLite extension packaging risk while preserving tokenizer behavior. A Rust FTS5 tokenizer extension can be evaluated later if the pre-tokenized table is correct but operationally awkward.

## A/B Backend Contract

Introduce an explicit backend selector with three modes:

- `tantivy`: current behavior, production default.
- `sqlite-fts5`: SQLite candidate only.
- `dual`: build both indexes and query both in eval/diagnostic flows.

The selector must be available to tests and xtask. Suggested environment variable:

```text
JULIE_SEARCH_BACKEND=tantivy|sqlite-fts5|dual
```

Production default remains `tantivy` until the cutover gate passes. If the env var is absent or invalid, use `tantivy` and emit a diagnostic warning only in non-production test/eval paths.

Backend selection must be target-aware:

- Definitions: Tantivy path is `text_search_impl` / `definition_search_with_index`; SQLite path must return the same `Symbol` output shape and `relaxed` semantics.
- Files: Tantivy path is `execute_file_search` / `SearchIndex::search_files`; SQLite path must return the same `FileSearchResult` semantics, including exact path, exact basename, glob, and test-intent sorting.
- Content: public `fast_search(search_target="content")` uses line mode. SQLite must provide file/content candidates that line mode can collect exact lines from, or the backend must report content-target unavailable. A docs/prose FTS implementation that only returns document sections is not a replacement for line-mode content search unless it preserves line-mode output and zero-hit diagnostics.

Do not hide backend mismatch with fallback. If `sqlite-fts5` is explicitly requested for a target it does not support, return an unavailable diagnostic in test/eval contexts. In production mode, keep the default `tantivy`; do not silently interpret `sqlite-fts5` as Tantivy.

`dual` mode is evaluation/diagnostic-only. It builds both projections and records both health states. Query failures from one backend must be represented in the artifact instead of falling back invisibly.

## File Structure

### Create

- `src/search/backend.rs`
  - Defines `SearchBackendKind`, backend selector parsing, shared result traits, and backend capability diagnostics.
- `src/search/sqlite/mod.rs`
  - Module entry point for SQLite search projection.
- `src/search/sqlite/schema.rs`
  - Creates/drops FTS5 projection tables, records projection version, probes SQLite FTS capabilities, and owns rowid mapping.
- `src/search/sqlite/capability.rs`
  - Verifies FTS5, `trigram`, `porter unicode61`, and `contentless_delete=1` support with actionable unavailable reasons.
- `src/search/sqlite/projection.rs`
  - Converts existing `SymbolDocument`, `FileDocument`, and markdown/document section data into SQLite FTS rows.
- `src/search/sqlite/token_text.rs`
  - Calls existing `CodeTokenizer` and emits stable token text for code FTS rows.
- `src/search/sqlite/query.rs`
  - Builds SQLite FTS queries for code, trigram, path, prose, and docs.
- `src/search/sqlite/searcher.rs`
  - Implements symbol/content/file/doc search against SQLite FTS and returns existing result structs used by `fast_search`.
- `src/search/sqlite/ranking.rs`
  - Applies field boosts, reciprocal-rank fusion, exact-match boosts, and test/doc role priors.
- `src/search/sqlite/line_candidates.rs`
  - Provides SQLite-backed file/content candidate discovery for line-mode content search.
- `src/search/sqlite/health.rs`
  - Converts SQLite projection/capability state into health/readiness diagnostics.
- `src/tests/tools/search/sqlite_fts5_backend.rs`
  - Unit and integration tests for the SQLite backend.
- `src/tests/tools/search/sqlite_fts5_ab_parity.rs`
  - Tantivy-vs-SQLite parity tests for exact known cases.
- `fixtures/search-quality/docs-web-search-corpus.json`
  - Hand-labeled docs/web-search corpus covering markdown and fetched-page style content.
- `fixtures/search-quality/vector-search-corpus.json`
  - Hand-labeled vector-required corpus rows that cannot pass without measured sqlite-vec contribution.

### Modify

- `src/search/mod.rs`
  - Export new backend modules.
- `src/search/index.rs`
  - Keep Tantivy behavior unchanged; optionally implement a shared trait adapter for A/B.
- `src/search/tokenizer.rs`
  - Expose tokenization helpers needed by `sqlite/token_text.rs`; do not change token output without tests.
- `src/search/query.rs`
  - Keep Tantivy query construction stable; move shared boost constants only if needed.
- `src/search/schema.rs`
  - Keep Tantivy schema stable until removal phase.
- `src/search/hybrid.rs`
  - Allow SQLite lexical results to participate in the existing semantic merge.
- `src/tools/search/text_search.rs`
  - Route definition search through selected backend while preserving `fast_search` output shape.
- `src/tools/search/execution.rs`
  - Resolve backend selection once and route definitions, files, and content consistently.
- `src/tools/search/line_mode.rs`
  - Accept a backend-specific content candidate provider so line-mode output and zero-hit diagnostics stay stable.
- `src/database/migrations.rs`
  - Add SQLite FTS projection table creation/versioning or add an explicit projection setup path if virtual tables should stay outside normal migrations.
- `src/database/vectors.rs`
  - Keep existing `symbol_vectors` behavior; add doc-section vectors only after docs/web A/B shows vector need.
- `src/health/projection.rs`
  - Report separate Tantivy and SQLite projection state; keep existing user-facing readiness semantics for the default backend.
- `src/health/types.rs`
  - Add backend/projection identity fields needed to diagnose which search projection is ready/stale/failed.
- `src/tools/search/trace.rs`
  - Record backend id and unavailable/error status in search execution traces.
- `crates/julie-extractors/src/markdown/mod.rs`
  - Preserve existing heading/section extraction. Modify only if the search projection needs a field already derivable from markdown AST.
- `xtask/src/search_ablation.rs`
  - Add backend dimension, target-aware corpus dispatch, source DB selection, vector-source validation, and enforced gate comparison.
- `xtask/src/cli.rs`
  - Add parser support for `eval ablation --backend`, `--source-db`, `--baseline`, and `--fail-on-hard-gate`.
- `docs/eval/julie-search-corpus-v1.json`
  - Preserve existing query ids/expected paths while adding required `search_target` fields.
- `.claude/skills/web-research/SKILL.md`
  - Write or preserve source URL/fetched timestamp/content hash frontmatter for newly fetched web markdown.
- `docs/TESTING_GUIDE.md`
  - Add the final A/B commands only after they exist and are stable.

## Schema Shape

Use base metadata tables plus FTS virtual tables. FTS rows should be rebuildable from SQLite canonical data and extracted markdown sections.

FTS identity rule: every projected searchable item gets one stable integer `projection_rowid`. FTS5 tables use implicit integer `rowid`; they do not use `doc_id` as a primary key. Store `doc_id` and all metadata in `search_projection_docs`, then insert into FTS tables with `rowid = projection_rowid`. Query results join `fts.rowid` back to `search_projection_docs.projection_rowid`.

Storage rule: use `content='', contentless_delete=1` when the capability probe confirms support. This avoids duplicate raw content while still supporting update/delete. If the implementation chooses external-content tables, create explicit projection content tables whose columns match the FTS table; do not point `content='symbols'` or `content='files'` at current canonical tables unless the projected token/raw columns actually exist there.

Suggested metadata tables:

```sql
create table if not exists search_projection_docs (
    projection_rowid integer primary key,
    doc_id text not null unique,
    workspace_id text not null,
    projection_version integer not null,
    source_kind text not null,
    source_id text not null,
    path text,
    url text,
    title text not null,
    heading_path text,
    language text,
    kind text not null,
    role text,
    test_role text,
    start_line integer,
    end_line integer,
    source_hash text not null,
    fts_body_hash text not null
);
```

Suggested FTS tables:

```sql
create virtual table if not exists search_symbols_code_fts using fts5(
    name_tokens,
    signature_tokens,
    doc_tokens,
    body_tokens,
    path_tokens,
    annotation_tokens,
    owner_tokens,
    content='',
    contentless_delete=1
);

create virtual table if not exists search_symbols_trigram_fts using fts5(
    raw_text,
    tokenize='trigram',
    content='',
    contentless_delete=1
);

create virtual table if not exists search_files_trigram_fts using fts5(
    path_text,
    tokenize='trigram',
    content='',
    contentless_delete=1
);

create virtual table if not exists search_docs_prose_fts using fts5(
    title,
    heading_path,
    body,
    metadata,
    tokenize='porter unicode61',
    content='',
    contentless_delete=1
);

create virtual table if not exists search_docs_trigram_fts using fts5(
    raw_text,
    tokenize='trigram',
    content='',
    contentless_delete=1
);
```

If the capability probe contradicts the bundled-build expectation for `porter unicode61`, `trigram`, or `contentless_delete=1`, disable the SQLite backend with a precise reason. Do not split tokenizer behavior without a failing compatibility test that captures the exact unsupported feature.

## Ranking And Fusion

The SQLite backend should not expose raw FTS rank directly to users. Convert candidates into the existing result structs and apply a Julie ranking layer.

Initial ranking inputs:

- code-token FTS rank
- trigram rank
- prose FTS rank
- exact name match
- exact basename/path match
- query token coverage
- symbol kind boost
- role/test role priors
- language affinity
- semantic vector rank when available

Fusion:

- Use reciprocal-rank fusion for multi-surface candidates.
- Keep path-looking and identifier-looking queries lexically dominated.
- Let doc/prose and natural-language queries admit semantic/doc candidates.
- Preserve existing reranker calls in `src/tools/search/text_search.rs` where possible.

Hard rule: if SQLite ranking cannot explain why a known Tantivy top result moved down, capture the miss in the A/B artifact before tuning.

## Markdown, Docs, And Web-Search Skill Impact

Downloaded web pages saved as markdown should become better, not worse, under the SQLite plan.

Index markdown at section granularity:

- `source_kind = "markdown_section"` for repository docs.
- `source_kind = "web_markdown_section"` for fetched web pages.
- Preserve path or URL.
- Preserve page title.
- Preserve heading path.
- Preserve start/end lines when available.
- Preserve fetch metadata for web pages when the data exists. Source URL and fetched timestamp must come from frontmatter or sidecar metadata; domain can be derived from `docs/web/<domain>/...`; content hash can be computed during projection. Do not fabricate unavailable metadata.

Search behavior:

- Prose queries use `search_docs_prose_fts` first.
- Exact quote/API-name queries use `search_docs_trigram_fts`.
- Conceptual doc queries use document-section vectors after a measured need is proven.
- Code search does not get polluted by docs unless the user target/category asks for docs or the query is natural-language-like.

Regression risks to test explicitly:

- A long fetched page must return the relevant section, not only the page.
- Heading text must be high signal.
- URL/domain metadata must be searchable without outranking body matches by default.
- Exact quoted snippets must still be findable.
- Code-looking queries must not get buried by markdown prose.

## A/B Evaluation Design

### Corpus Contract

Extend the corpus row shape before using it for replacement gates. Each query must declare the public target it exercises:

```json
{
  "id": "exact-router-file",
  "query": "router.rs",
  "search_target": "files",
  "category": "file-path",
  "expected_paths": ["src/router.rs"],
  "expected_line_contains": null,
  "requires_vectors": false
}
```

Rules:

- `search_target` is required and must be one of `definitions`, `files`, or `content`.
- `content` queries must assert either `expected_paths` plus `expected_line_contains`, or a document-section id/path plus line-range evidence.
- Docs/web rows must use `search_target="content"` when testing line-mode behavior and `search_target="definitions"` only when intentionally testing markdown-section symbols.
- Vector rows must set `requires_vectors=true`; the harness must mark them unavailable, not passed, when the source DB lacks populated `symbol_vectors`.
- Category metrics must be grouped by both `search_target` and `category` so a strong definitions score cannot hide broken file/content search.

### Baseline Artifact

Before changing production routing, capture a Tantivy baseline artifact at current `main`.

Command shape:

```bash
cargo xtask eval ablation \
  --corpus docs/eval/julie-search-corpus-v1.json \
  --backend tantivy \
  --source-db fixtures/databases/julie-snapshot/symbols.db \
  --limit 5 \
  --out target/search-ablation/tantivy-baseline.json
```

If this exact command does not exist yet, Task 1 extends `xtask/src/cli.rs` and `xtask/src/search_ablation.rs` until it does. Keep the existing `eval ablation` subcommand and add options; do not invent a separate `eval search-ablation` subcommand unless all parser tests and docs are updated together. The baseline artifact must record:

- commit SHA
- corpus hash
- source DB path/hash or source workspace id/revision
- backend id
- search target per query
- query count
- top1/top5/MRR
- per-target and per-category top1/top5/MRR
- p50/p95/max query latency
- index build time
- index size
- unavailable/error count
- per-query top paths, first relevant rank, target, relaxed flag, backend diagnostics, and line/document evidence for content rows

### Candidate Artifact

Run the same corpus against SQLite:

```bash
cargo xtask eval ablation \
  --corpus docs/eval/julie-search-corpus-v1.json \
  --backend sqlite-fts5 \
  --baseline target/search-ablation/tantivy-baseline.json \
  --source-db fixtures/databases/julie-snapshot/symbols.db \
  --limit 5 \
  --fail-on-hard-gate \
  --out target/search-ablation/sqlite-fts5-candidate.json
```

Run docs/web separately:

```bash
cargo xtask eval ablation \
  --corpus fixtures/search-quality/docs-web-search-corpus.json \
  --backend tantivy \
  --source-db target/search-ablation/docs-web-source/symbols.db \
  --limit 5 \
  --out target/search-ablation/tantivy-docs-web-baseline.json

cargo xtask eval ablation \
  --corpus fixtures/search-quality/docs-web-search-corpus.json \
  --backend sqlite-fts5 \
  --baseline target/search-ablation/tantivy-docs-web-baseline.json \
  --source-db target/search-ablation/docs-web-source/symbols.db \
  --limit 5 \
  --fail-on-hard-gate \
  --out target/search-ablation/sqlite-fts5-docs-web.json
```

Run vector-backed evidence from a source that actually has `symbol_vectors`:

```bash
cargo xtask eval ablation \
  --corpus fixtures/search-quality/vector-search-corpus.json \
  --backend tantivy \
  --source-db target/search-ablation/vector-source/symbols.db \
  --limit 5 \
  --out target/search-ablation/tantivy-vector-baseline.json

cargo xtask eval ablation \
  --corpus fixtures/search-quality/vector-search-corpus.json \
  --backend sqlite-fts5 \
  --baseline target/search-ablation/tantivy-vector-baseline.json \
  --source-db target/search-ablation/vector-source/symbols.db \
  --limit 5 \
  --fail-on-hard-gate \
  --out target/search-ablation/sqlite-fts5-vector.json
```

### Replacement Gates

Tantivy remains production default unless all hard gates pass and `--fail-on-hard-gate` returns success. Artifact generation alone is not evidence.

Quality hard gates:

- Overall `top1 >= tantivy top1`.
- Overall `top5 >= tantivy top5`.
- Overall `MRR >= tantivy MRR`.
- Per-target `top1/top5/MRR >= tantivy` for `definitions`, `files`, and `content`.
- No category loses more than one query at top5.
- Docs/web corpus has zero new top5 misses versus Tantivy.
- Symbol/test-intent categories do not regress.
- Exact path and exact basename queries remain top1.
- Vector-required rows are measured against a source DB with populated `symbol_vectors`; skipped vector rows fail the replacement gate.
- Content rows prove line/document evidence, not only file-level relevance.

Performance hard gates:

- Query p50 is at least 15% faster than Tantivy, or query p95 is at least 15% faster than Tantivy.
- Query p95 is not worse than Tantivy.
- Index build time is not worse than Tantivy by more than 10% unless query latency improves by at least 25%.
- SQLite projection size is not worse than Tantivy index size by more than 25% unless it replaces multiple index artifacts.
- Incremental update for one changed file completes within the current watcher search-readiness budget.

Operational hard gates:

- Search remains available after workspace startup and after watcher updates.
- Failed SQLite FTS projection rebuild leaves Tantivy available in `dual` mode.
- Health/readiness output identifies Tantivy and SQLite projection states separately.
- Cross-platform CI passes without loadable extension requirements.
- No new daemon restart or workspace repair path is required for normal users.

Report-only metrics:

- per-query latency deltas
- index write amplification
- vector hit contribution
- doc/prose vs code result mix
- SQLite DB page count and vacuum behavior
- memory/RSS during projection

## Task Plan

### Task 1: Extend Search Ablation For Backend A/B

**Files:**

- Modify: `xtask/src/cli.rs`
- Modify: `xtask/src/search_ablation.rs`
- Test: `src/tests/tools/search/sqlite_fts5_ab_parity.rs`
- Test: `xtask/src/cli.rs`

**Behavior:**

Add backend selection to the existing `cargo xtask eval ablation` harness. It must run Tantivy and SQLite candidates from the same source DB/workspace and same target-aware corpus, then emit comparable metrics and optionally fail on hard-gate regression.

**Acceptance criteria:**

- Existing ablation modes still work unchanged when backend is omitted.
- Parser accepts `eval ablation --backend <tantivy|sqlite-fts5> --source-db <path> --baseline <path> --fail-on-hard-gate`.
- Parser rejects the old nonexistent `eval search-ablation` shape unless the project intentionally updates every caller/documentation reference together.
- `--backend tantivy` produces the same top paths as the old harness.
- `--backend sqlite-fts5` can be wired before SQLite is implemented and reports an unavailable backend instead of panicking.
- Corpus rows include `search_target`; the runner dispatches definitions, files, and content through their public execution paths.
- Content rows verify line/document evidence instead of only path presence.
- Vector-required rows fail replacement gates when the source DB lacks populated `symbol_vectors` or a matching query embedding provider.
- Artifact includes backend id, commit, corpus hash, source DB/workspace identity, search target, query metrics, target/category metrics, latency, index build time, index size, unavailable counts, and hard-gate pass/fail status.
- `--fail-on-hard-gate` exits non-zero when candidate metrics violate hard gates or required rows are unavailable.

**Narrow verification:**

```bash
cargo nextest run --lib test_search_ablation_backend_selector
cargo nextest run -p xtask test_parse_eval_ablation_backend_options
```

### Task 2: Add Backend Selector Without Changing Production Default

**Files:**

- Create: `src/search/backend.rs`
- Modify: `src/search/mod.rs`
- Modify: `src/tools/search/execution.rs`
- Modify: `src/tools/search/text_search.rs`
- Modify: `src/tools/search/line_mode.rs`
- Test: `src/tests/tools/search/sqlite_fts5_backend.rs`

**Behavior:**

Introduce `SearchBackendKind` and route `fast_search` through Tantivy by default. Backend resolution happens once in `execute_search`, then passes target-specific execution down to definitions, files, and content. `JULIE_SEARCH_BACKEND=sqlite-fts5` should be accepted only when SQLite backend capability reports available for the requested target. `dual` should be limited to eval/diagnostic code paths.

**Acceptance criteria:**

- No environment variable: Tantivy path is used.
- `JULIE_SEARCH_BACKEND=tantivy`: Tantivy path is used.
- `JULIE_SEARCH_BACKEND=sqlite-fts5` before implementation: explicit unavailable diagnostic in tests, not silent fallback.
- Invalid value: Tantivy path plus diagnostic in eval/test contexts.
- Definitions, files, and content targets each record the selected backend in search execution trace.
- `search_target="files"` no longer bypasses backend selection through a hardcoded Tantivy-only path.
- `search_target="content"` keeps line-mode output shape and zero-hit diagnostics when using a backend-specific candidate provider.

**Narrow verification:**

```bash
cargo nextest run --lib test_search_backend_selector_defaults_to_tantivy
cargo nextest run --lib test_search_backend_selector_applies_to_files_and_content
```

### Task 3: Create SQLite FTS Projection Schema

**Files:**

- Create: `src/search/sqlite/capability.rs`
- Create: `src/search/sqlite/schema.rs`
- Create: `src/search/sqlite/mod.rs`
- Modify: `src/database/migrations.rs`
- Test: `src/tests/tools/search/sqlite_fts5_backend.rs`

**Behavior:**

Create and version SQLite FTS projection tables. Confirm the SQLite build supports FTS5, `trigram`, `porter unicode61`, and `contentless_delete=1`. If a required capability is unavailable, the backend capability must be unavailable with an actionable reason.

**Acceptance criteria:**

- Projection setup creates `search_projection_docs` plus all FTS tables.
- FTS tables use explicit `rowid = search_projection_docs.projection_rowid`; `doc_id` is metadata, not FTS identity.
- Capability check distinguishes missing FTS5, missing trigram tokenizer, unsupported `porter unicode61`, and unsupported `contentless_delete=1`.
- Re-running setup is idempotent.
- Projection version mismatch triggers rebuild, not partial reuse.
- External-content mode, if chosen during implementation, points at projection row tables with matching columns and never at current `symbols`/`files` tables with nonexistent projected columns.

**Narrow verification:**

```bash
cargo nextest run --lib test_sqlite_fts5_projection_schema_is_idempotent
```

### Task 4: Materialize Code Token Rows

**Files:**

- Create: `src/search/sqlite/token_text.rs`
- Create: `src/search/sqlite/projection.rs`
- Modify: `src/search/tokenizer.rs`
- Test: `src/tests/tools/search/sqlite_fts5_backend.rs`

**Behavior:**

Convert `SymbolDocument` and `FileDocument` into SQLite projection rows. Code FTS rows use Julie tokenizer output, not raw text. Trigram rows use raw text. Every inserted FTS row uses the projection rowid allocated by `search_projection_docs`.

**Acceptance criteria:**

- `HubConfig`, `hub_config`, and `hub config` style identifier variants produce searchable tokens equivalent to Tantivy tokenizer tests.
- CamelCase, snake_case, affix stripped, suffix stripped, and stemmed variants are present in code-token text.
- Raw trigram table can find partial identifiers and punctuation-heavy snippets.
- Projection stores enough metadata to reconstruct existing result structs.
- Projection stores role/test_role and enough annotation/owner token text to preserve annotation search behavior.
- File deletes remove every FTS row associated with the deleted file's projection rowids.

**Narrow verification:**

```bash
cargo nextest run --lib test_sqlite_code_token_projection_matches_code_tokenizer
```

### Task 5: Implement SQLite Symbol Search

**Files:**

- Create: `src/search/sqlite/query.rs`
- Create: `src/search/sqlite/searcher.rs`
- Create: `src/search/sqlite/ranking.rs`
- Test: `src/tests/tools/search/sqlite_fts5_backend.rs`
- Test: `src/tests/tools/search/sqlite_fts5_ab_parity.rs`

**Behavior:**

Implement symbol search over code-token and trigram tables. Return `SymbolSearchResult` compatible with the existing reranker and `text_search_impl`.

**Acceptance criteria:**

- Exact name matches rank above body-only matches.
- Definition kind boosts match current Tantivy expectations.
- AND query is attempted first; OR fallback is marked relaxed.
- Annotation-filtered queries such as `@test` require matching annotation keys and do not degrade into ordinary text search.
- Owner-name context participates in ranking where Tantivy currently indexes it.
- Language/kind filters are applied before final truncation.
- Exclude-tests behavior matches current `fast_search`.
- Existing reranker can consume SQLite results without separate result formatting.

**Narrow verification:**

```bash
cargo nextest run --lib test_sqlite_symbol_search_exact_name_beats_body
```

### Task 6: Implement SQLite File And Path Search

**Files:**

- Modify: `src/search/sqlite/query.rs`
- Modify: `src/search/sqlite/searcher.rs`
- Modify: `src/search/sqlite/ranking.rs`
- Modify: `src/tools/search/execution.rs`
- Test: `src/tests/tools/search/sqlite_fts5_backend.rs`

**Behavior:**

Implement path search using `search_files_trigram_fts` with exact path, exact basename, path fragment, and glob handling consistent with current behavior.

**Acceptance criteria:**

- Exact path is top1.
- Exact basename outranks partial path fragment.
- Space-separated path queries match slash/underscore/dash separated paths.
- Glob queries use the existing glob classifier and do not become broad trigram noise.
- Test-path intent still finds tests when requested.
- `execute_file_search` can use SQLite file results without requiring a Tantivy `SearchIndex`.
- File search total counts and relaxed indicators match current `fast_search` formatting expectations.

**Narrow verification:**

```bash
cargo nextest run --lib test_sqlite_file_search_space_separated_path_query
```

### Task 7: Implement Prose Document And Web Markdown Search

**Files:**

- Modify: `.claude/skills/web-research/SKILL.md`
- Modify: `src/search/sqlite/projection.rs`
- Modify: `src/search/sqlite/query.rs`
- Modify: `src/search/sqlite/searcher.rs`
- Create: `fixtures/search-quality/docs-web-search-corpus.json`
- Test: `src/tests/tools/search/sqlite_fts5_backend.rs`

Generated artifact, not committed: `target/search-ablation/docs-web-source/symbols.db`.

**Behavior:**

Index markdown sections and fetched web-page markdown as document sections. Use prose FTS for natural language and doc trigram for exact snippets. Establish a metadata source for fetched web markdown: frontmatter keys (`source_url`, `fetched_at`, `content_hash`) when present, otherwise derive domain from `docs/web/<domain>/...` and compute content hash during projection.

**Acceptance criteria:**

- Markdown heading search returns the section, not only the whole file.
- Body prose query finds the relevant section.
- Exact quote/snippet query works through trigram.
- URL/domain metadata is searchable when frontmatter or `docs/web/<domain>/...` path data exists, but does not outrank a strong title/body hit.
- Code-looking queries do not get dominated by prose docs.
- The web-research skill writes or preserves frontmatter metadata for newly fetched pages, so future fetched pages have source URL and fetched timestamp.
- Existing fetched pages without frontmatter remain searchable with path-derived domain and computed content hash; missing timestamp is recorded as unavailable metadata, not fabricated.
- The docs/web source DB used by Task 10 is created or verified by the harness and contains every expected path referenced by `fixtures/search-quality/docs-web-search-corpus.json`.

**Narrow verification:**

```bash
cargo nextest run --lib test_sqlite_docs_search_returns_relevant_markdown_section
```

### Task 8: Add Semantic Fusion With Existing sqlite-vec

**Files:**

- Modify: `src/search/hybrid.rs`
- Modify: `src/search/sqlite/searcher.rs`
- Modify: `src/search/sqlite/ranking.rs`
- Modify: `xtask/src/search_ablation.rs`
- Create: `fixtures/search-quality/vector-search-corpus.json`
- Modify: `src/database/vectors.rs` only if doc-section vectors are added
- Test: `src/tests/tools/search/sqlite_fts5_backend.rs`

Generated artifact, not committed: `target/search-ablation/vector-source/symbols.db`.

**Behavior:**

Fuse SQLite lexical results with existing `symbol_vectors`. Do not add document vectors until docs/web lexical evidence shows a gap that vector search can close. The A/B harness must be able to run vector-required rows against a source DB with populated `symbol_vectors` and a query embedding provider matching that DB's `embedding_config`.

**Acceptance criteria:**

- Symbol vector results can be merged with SQLite lexical results.
- Path-looking queries remain lexically dominated.
- Natural-language definition queries can admit vector-only candidates.
- Vector unavailable path still returns lexical results with a diagnostic only in eval/test contexts.
- Vector gate artifacts record embedding model, dimensions, provider, and whether vector rows were measured or unavailable.
- Replacement gates fail when `requires_vectors=true` rows are skipped.
- The vector source DB used by Task 10 is created or verified by the harness, records its DB hash, and contains populated `symbol_vectors` plus matching `embedding_config`.
- Query embedding uses the same model/dimensions as the stored vector source. A provider/model mismatch is an unavailable vector gate, not a passing lexical-only run.

**Narrow verification:**

```bash
cargo nextest run --lib test_sqlite_hybrid_search_keeps_exact_path_lexical_top1
```

### Task 9: Dual Projection And Watcher Update Path

**Files:**

- Modify: `src/tools/workspace/indexing/index.rs`
- Modify: `src/tools/workspace/indexing/pipeline.rs`
- Modify: `src/tools/workspace/indexing/route.rs`
- Modify: `src/search/projection.rs`
- Modify: `src/health/projection.rs`
- Modify: `src/health/types.rs`
- Modify: watcher update code that currently updates Tantivy
- Test: `src/tests/integration/watcher_handlers.rs`
- Test: `src/tests/tools/search/sqlite_fts5_backend.rs`

**Behavior:**

In `dual` mode, workspace indexing builds both Tantivy and SQLite projections. Watcher updates keep both fresh. Tantivy remains the fallback if SQLite projection fails.

**Acceptance criteria:**

- Initial workspace indexing can build both projections.
- File add/update/delete/rename updates SQLite projection rows.
- Hardcoded Tantivy projection paths are either generalized or intentionally isolated behind backend-specific projection calls.
- SQLite projection failure does not corrupt Tantivy readiness.
- Projection version mismatch triggers rebuild.
- Search readiness reports enough diagnostic state to know which backend is stale.
- A failed SQLite projection rebuild in `dual` mode leaves Tantivy health ready and records SQLite stale/failed detail.

**Narrow verification:**

```bash
cargo nextest run --lib test_sqlite_projection_updates_on_file_change
cargo nextest run --lib test_sqlite_projection_updates_on_file_rename
cargo nextest run --lib test_dual_mode_sqlite_projection_failure_keeps_tantivy_ready
```

### Task 10: Run A/B Gates And Decide Cutover

**Files:**

- Modify: `xtask/src/search_ablation.rs`
- Modify: `docs/TESTING_GUIDE.md` only after final commands stabilize
- Create: `docs/plans/verification-ledger-template.md` rows or a plan-specific ledger section if needed

**Behavior:**

Run Tantivy and SQLite candidates on code, docs/web, file/content, and vector-required corpora. Decide from hard gates, not impressions.

**Required commands:**

```bash
cargo xtask eval ablation \
  --corpus docs/eval/julie-search-corpus-v1.json \
  --backend tantivy \
  --source-db fixtures/databases/julie-snapshot/symbols.db \
  --limit 5 \
  --out target/search-ablation/tantivy-baseline.json

cargo xtask eval ablation \
  --corpus docs/eval/julie-search-corpus-v1.json \
  --backend sqlite-fts5 \
  --baseline target/search-ablation/tantivy-baseline.json \
  --source-db fixtures/databases/julie-snapshot/symbols.db \
  --limit 5 \
  --fail-on-hard-gate \
  --out target/search-ablation/sqlite-fts5-candidate.json

cargo xtask eval ablation \
  --corpus fixtures/search-quality/docs-web-search-corpus.json \
  --backend tantivy \
  --source-db target/search-ablation/docs-web-source/symbols.db \
  --limit 5 \
  --out target/search-ablation/tantivy-docs-web-baseline.json

cargo xtask eval ablation \
  --corpus fixtures/search-quality/docs-web-search-corpus.json \
  --backend sqlite-fts5 \
  --baseline target/search-ablation/tantivy-docs-web-baseline.json \
  --source-db target/search-ablation/docs-web-source/symbols.db \
  --limit 5 \
  --fail-on-hard-gate \
  --out target/search-ablation/sqlite-fts5-docs-web.json

cargo xtask eval ablation \
  --corpus fixtures/search-quality/vector-search-corpus.json \
  --backend tantivy \
  --source-db target/search-ablation/vector-source/symbols.db \
  --limit 5 \
  --out target/search-ablation/tantivy-vector-baseline.json

cargo xtask eval ablation \
  --corpus fixtures/search-quality/vector-search-corpus.json \
  --backend sqlite-fts5 \
  --baseline target/search-ablation/tantivy-vector-baseline.json \
  --source-db target/search-ablation/vector-source/symbols.db \
  --limit 5 \
  --fail-on-hard-gate \
  --out target/search-ablation/sqlite-fts5-vector.json
```

**Cutover decision:**

- If all hard gates pass, create a follow-up cutover plan that makes SQLite the default and removes Tantivy only after one release cycle of dual-mode evidence.
- If quality passes but performance does not improve, keep Tantivy and document SQLite as an experimental candidate.
- If performance improves but quality regresses, tune SQLite ranking/evidence and rerun. Do not ship.
- If docs/web regresses, fix document indexing before any code-search cutover.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, `xtask/src/search_ablation.rs`, and the search dogfood bucket.

**Worker red/green scope:** exact tests only, using `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`.

**Worker ceiling:** workers may run exact tests for their assigned search component. They do not own `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test dogfood`, or A/B gate interpretation.

**Worker gate invariant:** each worker test must prove one named invariant: tokenizer parity, FTS projection correctness, result-shape compatibility, public target routing, path ranking, line-mode candidate compatibility, docs section retrieval, vector fusion, watcher freshness, readiness isolation, or ablation artifact correctness.

**Lead affected-change scope:** `cargo xtask test changed` after each coherent implementation batch.

**Branch gate:** `cargo xtask test dev` before handoff. Add `cargo xtask test dogfood` for any search/scoring/tokenization change. Add `cargo xtask test system` when workspace indexing, watcher, daemon, or readiness state changes.

**Replay/metric evidence:** A/B artifacts are hard-gate evidence only when generated from the same commit SHA, corpus hash, source DB/workspace hash, and backend ids for both backends. Overall, per-target, and per-category quality metrics are hard gates. Vector-required rows must be measured, not skipped. `--fail-on-hard-gate` success is required; JSON artifact creation by itself is not passing evidence. Latency/index-size/build-time metrics are hard gates for replacement, report-only before replacement.

**Escalation triggers:** tokenizer output changes, ranking/scoring changes, workspace indexing/readiness changes, SQLite migration changes, docs/web-search result regression, vector schema changes, or any A/B metric regression.

**Assigned verification failure:** workers stop and report when exact tests fail. The lead decides whether the test, plan, or implementation is wrong.

**Verification ledger:** record invariant, command, scope label, commit SHA, result, timestamp, corpus hash, source DB/workspace hash, backend ids, artifact path, and hard-gate metrics. Reuse evidence only when HEAD, corpus hash, source hash, and backend ids match exactly.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** backend contract, ranking design, A/B gate interpretation, cutover decision.
- Harness mapping: follow `RAZORBACK.md`.

**Implementation tier:** bounded projection, query, schema, and test tasks after this plan fixes the contract.
- Harness mapping: follow `RAZORBACK.md`.

**Coupled implementation tier:** watcher/readiness/indexing changes and `execute_search` / line-mode / `text_search_impl` routing.
- Harness mapping: follow `RAZORBACK.md`.

**Gate-interpretation reviewer:** any failed A/B gate, docs/web regression, or performance-quality tradeoff.
- Harness mapping: follow `RAZORBACK.md`.

**Escalation tier:** tokenizer behavior changes, ranking correctness, SQLite migration/projection failures, search readiness regressions, vector provider/source mismatches, or failed worker attempts.
- Harness mapping: follow `RAZORBACK.md`.

**Worker eligibility:** workers can own tasks with disjoint write scopes and exact test names. Workers cannot own A/B metric interpretation or production cutover decisions.

**Mechanical exclusion:** docs-only edits may use the mechanical tier only when recording already-decided evidence. A/B evidence interpretation is not mechanical.

## Removal Plan For Tantivy

Do not remove Tantivy in this plan.

Create a separate removal plan only after:

1. SQLite backend passes all hard gates on code, docs/web, content, file, and vector-required corpora.
2. Dual mode has run successfully on at least one dogfood workspace.
3. Watcher update performance is measured and acceptable.
4. The release notes can honestly say whether users should expect index rebuilds, DB growth, or search behavior changes.

Tantivy removal must delete all related code, tests, dependency entries, compatibility marker handling, and docs in one coherent branch. Do not leave stale dead-code paths.
