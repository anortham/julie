# FTS Phase 2 — Unified Schema Design

**Date:** 2026-05-21
**Status:** Approved, ready for writing-plans
**Investigation:** `docs/investigation/2026-05-21-fts-ranking-gap-vs-lancedb.md`
**Phase 1 results:** `docs/investigation/2026-05-21-fts-fixes-phase1-results.md`
**Phase 1 plan (for shape reference):** `docs/plans/2026-05-21-fts-ranking-fixes-phase1.md`

## Goal

Close the FTS ranking gap to Eros's lancedb-fts baseline (Julie 267/406 top1 → target ~374/406, the May-21 multi-lang bakeoff result). The structural fix is the unified schema documented in the investigation doc. Phase 1 shipped the cheap wins, ablation infra, and three codex-review fixes; Phase 2 is the schema unification itself.

## Background

The investigation doc identifies the fault line: Julie has two disjoint Tantivy doc types (`SymbolDocument` + `FileDocument`) with disjoint field sets, surfaced to callers as `search_target=definitions|files|content`. Eros's lancedb-fts uses one unified `search_doc` schema with a `kind` column, all 7 fields indexed for every row, queried in a single BM25 sweep. Phase 1 partially closed Pattern A (duplicate-file scenarios) on the definitions and content paths via title-exact boosts, but Pattern A on the files target, Pattern B (test-intent lookups), and Pattern C (documentation-phrase queries) remain blocked by the schema fragmentation.

This design is the Eros recipe applied back to Julie, with three Julie-specific additions called out below.

## Design

### Schema — one Tantivy doc type with `kind` discriminator

Single `search_doc` table. Every row has the same field set regardless of kind. The seven core FTS fields are taken from Eros's schema unchanged:

| Field | Tokenizer | Stored | Symbol-row population | File-row population |
|---|---|---|---|---|
| `id` | raw | yes | symbol id | file path |
| `kind` | raw | yes | `"function"` / `"class"` / `"struct"` / ... | `"file"` |
| `name_text` | simple | yes | symbol name | basename without extension |
| `path_text` | simple | yes | file path tokenized | file path tokenized |
| `signature_text` | simple | yes | symbol signature | empty |
| `doc_text` | simple | no | doc comment | empty |
| `relationship_text` | simple | no | relationships text blob (see below) | aggregated symbol-relationship summary or empty |
| `body_excerpt` | simple | no | `code_context[:2000]` | `file_content[:2000]` |
| `pretokenized_code` | simple | no | CamelCase/snake_case-split of body+name+sig at index time | CamelCase/snake_case-split of file content at index time |
| `language` | raw | yes | symbol language | file language |
| `file_path` | raw | yes | path | path |
| `basename` | raw | yes | basename | basename |
| `start_line` | u64 | yes | symbol start line | 1 |
| `role` | raw | yes | classified | classified |
| `test_role` | raw | yes | classified | classified |

`role` / `test_role` retain their Phase-1 meaning (`src`, `test`, `docs`, `vendor`, `generated`, `config`, `build`; `impl_test`, `helper_test`, `fixture_test`, `smoke_test`). These are scoring inputs, not row discriminators.

The 7 core FTS fields (`name_text`, `path_text`, `signature_text`, `doc_text`, `relationship_text`, `body_excerpt`, `pretokenized_code`) are what a single BM25 sweep queries.

### Tokenizer — simple at query time, code-aware only at index time

Replace `CodeTokenizer`'s current query-time emission stack (CamelCase split, snake_case split, affix variants, prefix/suffix stripping, English stemming, important-pattern boosts) with two pieces:

1. **Matching tokenizer**: simple, lowercase, ascii-fold, **no stemming**, no stopword removal, `max_token_length: 80`. This is Eros's `CODE_FTS_INDEX_OPTIONS` directly. Applied to every `simple`-tokenizer field above at both index and query time.
2. **Index-time CamelCase/snake_case splitting**: only writes into the `pretokenized_code` field. Implemented by a stateless function that takes a string, emits the original token followed by its CamelCase and snake_case parts as space-separated tokens, then the `simple` tokenizer indexes that as-is.

The CodeTokenizer compat-signature gates from Phase 1 T3 (`JULIE_ABLATE_STEMMING`, `JULIE_ABLATE_CAMEL_EMIT`) become obsolete — there is no stemming to ablate and CamelCase emission is now an index-time fixed behavior in `pretokenized_code`. They get deleted along with the rest of the obsoleted tokenizer code.

### Dispatch — one query path, no `search_target`

Delete `search_target` from `fast_search`. The MCP tool signature becomes:

```rust
struct FastSearchTool {
    query: String,
    language: Option<String>,
    file_pattern: Option<String>,
    exclude_tests: Option<bool>,
    limit: Option<u32>,
    workspace: Option<String>,
    return_format: Option<String>,
}
```

The three `execute_definition_search` / `execute_file_search` / `execute_content_search` functions are deleted. A single `execute_search` does one Tantivy query across the seven FTS fields, applies a single reranker pass, returns a mixed-kind ranked list. Each `SearchHit` carries `kind` so the caller can see what they got.

`SearchTarget` enum and the `parse_search_target` plumbing go with it.

### Reranker — one set of boosts for all candidates

Collapse the per-target reranking into a single post-rank function. The boosts come from Eros's `_field_score` and `_exact_field_score` (investigation doc lines 273–286), with Julie's existing constants preserved where they already match (`EXACT_TITLE_BOOST = 100`, `PARTIAL_TITLE_BOOST = 50`, `PATH_BOOST = 40` already match Eros):

- `name_text == normalized_query` → `+100`, plus per-kind boost (`+30` for function/class/struct/...; `+35` for symbol-like kinds; etc.)
- `kind == "file"` and `query in {basename, stem(basename)}` → `+120`
- otherwise `query in {path, basename, stem(basename)}` → `+80`
- per-term: title exact `+100`, title partial `+50`, path-fragment `+25`, basename exact `+40`, `kind=="file" && term==stem` `+30`

The Phase 1 cross-target boosts (`apply_symbol_title_boost_to_file_results` in `src/search/index.rs`, the title-exact block in `apply_reranker_to_content_results`) get folded into this single reranker and the standalone helpers deleted. The Phase 1 `compact_alnum_lc` helper survives — it's the right primitive for title-exact matching.

`RRF × 200` in `src/tools/search/text_search.rs:507-515` gets deleted. It existed to fuse three target-fragmented searches; with one search there's nothing to fuse.

### Julie-specific additions

Three places where Phase 2 diverges from a literal Eros port:

**1. `relationship_text` populated from Julie's `relationships` table.**
At symbol index time, query `relationships` for edges where `source_symbol_id` or `target_symbol_id` matches the symbol being indexed. Collect the related symbols' names (`callee`, `caller`, `type_used`, `extends`, `implements`, etc.) into a single text blob. Set as `relationship_text`. This gives Julie measurable parity with Eros's relationship-aware ranking — Eros builds the same blob from a similar table.

For file-rows: aggregate the relationship summary from all symbols contained in the file (top-N most-referenced symbol names; bounded for cost). If this turns out to be expensive at index time, drop it for file-rows in v1 and revisit after measurement.

**2. `body_excerpt` truncation at 2000 bytes for both row types.**
Matches Eros's `body[:2000]`. For symbol-rows, Julie's existing `code_context` is already small per symbol; truncation is essentially a no-op. For file-rows, this discards content past byte 2000. Acceptable: per-symbol rows in the same file already carry the relevant body slices, so the file row's body is a coarse signal anyway.

**3. Ablation infra survives, retargeted.**
The Phase 1 T3/T4 ablation harness (`xtask/src/search_matrix.rs`, `Ablation` enum, env-var gates) gets repurposed. The stemming/CamelCase env vars are deleted, but the matrix runner and `--ablation` CLI scaffold stay as the regression-measurement vehicle for any Phase 2 follow-on tuning (e.g. token-length cap, reranker constant adjustments).

### Migration — compat marker bump 3 → 4

`SEARCH_COMPAT_MARKER_VERSION` already exists. Bump it from 3 to 4 as part of the schema change. Every existing workspace's compat marker becomes stale; the next workspace open detects the mismatch and triggers `RecreatedIncompatible` → empty index → reindex from SQLite. The mechanism is the same one Phase 1 T3 used and it's proven in production.

User-visible impact: one expensive reindex on next session connect after Phase 2 ships. Comparable to T3's footprint. Acceptable.

In daemon mode, the existing stale-binary auto-restart path covers the daemon-side: new daemon comes up, opens workspace, detects compat mismatch, recreates. No special handling needed.

### Public surface that changes

- `src/handler.rs` — `fast_search` tool description rewritten; `search_target` parameter removed.
- `JULIE_AGENT_INSTRUCTIONS.md` — `fast_search` usage updated; `search_target` references deleted.
- `.claude/skills/*/SKILL.md` — any skill that mentions `search_target` rewritten (search-debug, search-quality, anything similar).
- `docs/SEARCH_FLOW.md`, `docs/ARCHITECTURE.md`, `docs/INTELLIGENCE_LAYER.md` — `search_target` references deleted, dispatch documentation updated.
- CLI: `julie-server search` subcommand loses any `--target` flag (verify in `src/cli_tools/`).

### What stays

- Embeddings infrastructure: untouched. Per the Phase 1 results doc, embeddings stay until the unified schema measurably closes the test-intent gap.
- SQLite schema: untouched. Symbols, relationships, files, types all stay in SQLite as the canonical store; Tantivy is the projection.
- Tree-sitter extractors: untouched.
- Daemon lifecycle, workspace pool, mutation gate: untouched.
- File watcher: untouched.
- Phase 1's `compact_alnum_lc` primitive: kept, used in the unified reranker.
- Phase 1's eager ablation restore: kept (the ablation harness lives on).

## Acceptance criteria

A Phase 2 plan executes successfully when all of the following hold at the merged HEAD:

1. **`search_target` is gone.** No references to it in `src/`, `xtask/`, `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/skills/`, or `docs/` (outside the investigation/results docs which are historical).
2. **One Tantivy doc type.** `SymbolDocument` and `FileDocument` are replaced by a single `SearchDocument` struct with the field set above. The three `add_*` index methods become one.
3. **One query path.** `execute_definition_search`, `execute_file_search`, `execute_content_search` are deleted. A single `execute_search` (or equivalent) drives all of `fast_search`.
4. **One reranker.** The per-target rerankers and the Phase 1 cross-target boost helpers are folded into a single function that applies the Eros-recipe field-score boosts to a mixed-kind candidate set.
5. **Tokenizer simplified.** `CodeTokenizer` is gone or shrunk to a thin wrapper over Tantivy's `simple` tokenizer with ascii-fold + lowercase + the `max_token_length: 80` cap. CamelCase/snake_case splitting lives only in the `pretokenized_code` field's index-time emitter. `JULIE_ABLATE_STEMMING` and `JULIE_ABLATE_CAMEL_EMIT` env vars are deleted.
6. **Migration is transparent.** `SEARCH_COMPAT_MARKER_VERSION` bumped to 4. Existing workspaces auto-rebuild on next open with no manual action required. Daemon mode handles the rebuild during the stale-binary restart flow.
7. **Eros bakeoff regression gate.** Running Eros's 406-query multi-lang bakeoff (`~/source/eros/python/eros/eval/compare.py`) against Phase 2 HEAD shows top1 ≥ 350/406. Stretch target: ≥ 370/406 (within 5 of Eros's lancedb-fts at 374). This is the binding gate; if Phase 2 fails it, we don't merge.
8. **Branch-gate.** `cargo xtask test dev` and `cargo xtask test dogfood` both green.
9. **Search-matrix smoke profile still green.** `cargo xtask search-matrix run --profile smoke` returns expected top-1 hits on the calibration cases.

## Out of scope for Phase 2

- Removing embeddings. Per Phase 1 results doc, embeddings stay until the unified schema measurably closes the test-intent gap. The decision to keep or drop is a Phase 3 measurement-driven call, not a Phase 2 scope item.
- Changing the SQLite schema. Tantivy projection changes; the source of truth is unchanged.
- Changing the MCP tool count or adding new tools. Phase 2 collapses internal complexity, not the public surface.
- Cross-workspace search reshape. The per-workspace isolation model stays.
- Dashboard/health/observability changes. Only update what's directly affected by the schema change.

## Risks

1. **Eros bakeoff doesn't reach 350.** If the unified schema alone isn't enough to close the gap, we measure where the deltas are (per-pattern breakdown) and iterate. The ablation harness is the diagnostic tool for this.
2. **Reindex cost on existing workspaces.** Some users will see one slow startup after upgrade. Documented in the release notes; same cost shape as Phase 1 T3.
3. **Tokenizer simplification regresses CamelCase recall.** Mitigation: `pretokenized_code` is exactly this signal indexed at index time. If empirical data shows additional emission helps, the ablation harness measures it before adding back.
4. **`relationship_text` populated incorrectly bloats the index or distorts ranking.** Mitigation: cap the per-symbol relationship blob length; revisit if measurement shows index growth >2× or ranking distortion. File-row aggregation is the most-likely-to-cut item if cost spikes.

## Plan-author guidance

These are operational decisions the plan author owns; the design itself is locked.

- **Plan ordering: big-bang rewrite, single commit chain.** The compat-marker mechanism handles in-place upgrade on next workspace open, so we do not need source-level dual schema. Plan tasks should bring up a fresh `SearchIndex` implementation, replace call sites incrementally with workspace-walking commits that compile, and delete the old `SymbolDocument`/`FileDocument` types in one cleanup commit. Each commit must compile and pass `cargo check`.
- **Eros bakeoff baseline.** The acceptance gate (≥ 350/406) requires Eros's harness. The plan's first task must run `~/source/eros/python/eros/eval/compare.py` against current `main` (HEAD `722bdee5`) and against Phase 2 HEAD; both numbers go in the verification ledger.
- **Reranker constant tuning is a measurement task, not a guess.** The numbers in the reranker section above are Eros's defaults. After the first Phase 2 bakeoff run, the plan should include a tuning sweep over the matrix harness if measurement shows ranking gaps the unified schema alone didn't close.
