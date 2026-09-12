# Plan 2: Precise and complete retrieval contracts

**Status:** Proposed. No implementation has started.
**Goal:** Preserve exact evidence and make scoped search, paging, and body retrieval reliable for agents.
**Depends on:** Reference work can start independently; integrate after Plan 1 freshness and semantics fixes.
**Execution:** Follow the [roadmap contract](2026-09-11-revival-roadmap.md), including Sol ownership, TDD, one Cargo command at a time, no tool renaming, and no durable continuations.

## Architecture quality

Changes belong between canonical facts/snapshots and existing tool responses. `Site` must preserve canonical precision; compact renderers must serialize the normalized request; body extraction must identify exactly what it returned. Search should use existing lexical evidence and semantic candidates, with identical filtering. Risk is medium for retrieval and high where consumers may treat a reference or body as authoritative for editing.

No parser duplication, language-specific symbol recovery, generic new query engine, snapshot-retention store, or automatic rename is part of this plan.

## Ownership and ordering

| Task | Owned files | Existing test locations | Serialization / reason |
|---|---|---|---|
| 2A references | `crates/julie-tools/src/navigation/sites.rs`, `fast_refs.rs`, exact-site consumers in `call_path.rs` and `call_path_web.rs` | `src/tests/tools/target_workspace_fast_refs_tests/tests/identifier_refs.rs` | First; defines precision used by later ports |
| 2B content | `crates/julie-tools/src/search/execution/mod.rs`, `line_enrichment.rs`, `tool_execution.rs`, `backend.rs` only if needed | `src/tests/tools/search/line_mode_second_pass_tests.rs`, `source_regions.rs`, `file_pattern_tests.rs` | After 2A by default; can edit independently, Cargo serialized |
| 2C paging | `crates/julie-tools/src/shared.rs`, `search/formatting.rs`, `navigation/fast_refs.rs`, `symbols/primary.rs`, `symbols/target_workspace.rs`, `impact/mod.rs`, `impact/formatting.rs` | Proposed `src/tests/tools/paging_contract.rs`, registration in the inline tools module in `src/tests/mod.rs` | After 2A/2B; shares renderers and final normalized search behavior |
| 2D bodies | `crates/julie-tools/src/symbols/body_extraction.rs`, `symbols/mod.rs`, `deep_dive/mod.rs`, `deep_dive/formatting.rs`, `deep_dive/data/mod.rs` | Proposed `src/tests/tools/body_completeness.rs`, registration in the inline tools module in `src/tests/mod.rs` | After 2C; shared stateless request formatting |

Refresh exact renderer/module ownership through Julie before editing; do not expand to all formatting files. Each worker owns only its named task. The lead owns shared test-module registration when editing happens in parallel.

## 2A. Preserve canonical reference sites

**Inputs:** `IdentifierRow` stable identity/full span and `RelationshipRow` ordinal/span/exactness/confidence/metadata from `crates/julie-facts/src/rows/mod.rs`. Current loss begins in `reference_sites`, before final formatting.

**Produces:** Existing reference and call-site consumers receive stable identity, exact span where known, fallback status, confidence, and provenance. Preserve the public `fast_refs` name and reference-kind filtering.

1. Write `fast_refs_keeps_two_distinct_reference_sites_on_one_line` using the existing identifier-reference fixtures. Include a use on a definition's line and two calls within one containing symbol.
2. RED/GREEN command: `cargo nextest run -p julie --lib fast_refs_keeps_two_distinct_reference_sites_on_one_line`; run `cargo check` after implementation.
3. Carry site identity/span through `Site`; deduplicate only equivalent canonical sites. Reconcile identifier and relationship observations of the same occurrence without collapsing distinct positions. Sort deterministically by path, span, and identity within existing ranking.
4. Add exact RED/GREEN tests `fast_refs_preserves_exact_span_and_site_identity` and `fast_refs_labels_relationship_fallback_without_claiming_exactness`. Do not manufacture exact spans from a containing symbol line.
5. Verify `call_path` consumes the new site structure safely. For web mode, add `web_call_path_reports_unmatched_call_beside_a_matched_call` and `web_call_path_uses_the_matched_call_site`. Preserve per-call evidence rather than using the first HTTP call or suppressing every unmatched call in a symbol. Keep existing graph traversal/provider scope.

**Acceptance:**

- [ ] Two distinct same-line references remain distinct in structured/full results; compact output may group them only if the site count and recoverable positions remain explicit.
- [ ] Exact and inferred references are distinguishable; reference counts count sites, not merely lines.
- [ ] Reference-kind filters and workspace routing retain all evidence fields.
- [ ] Call paths show the site supporting each hop; unmatched web endpoints are not hidden by unrelated matched calls.
- [ ] Canonical language inventory has an evidence ledger for reference-site applicability and precision; missing extractor evidence is tracked upstream, never faked in Julie.

## 2B. Make scoped content retrieval useful

**Inputs:** Existing query, `file_pattern`, language, regions, test filter, backend, workspace and line-enrichment path. The live repro is a scoped “workspace routing” query that misses a matching document under semantic auto.

**Produces:** Auto search can include relevant lexical document/configuration evidence for an explicitly scoped request without sacrificing ordinary semantic code search. Explicit backends retain their established meaning.

1. Add `auto_scoped_search_returns_matching_document_with_semantic_candidates`: a fixture contains prose with the answer and in-scope code with similarly named symbols. Use arbitrary directory names to forbid a `docs/` special case.
2. RED/GREEN command: `cargo nextest run -p julie --lib auto_scoped_search_returns_matching_document_with_semantic_candidates`.
3. Reuse bounded lexical file/line evidence for scoped auto queries, merged and deduplicated through existing result formatting. Determine content from stored language/source-region metadata. Apply workspace, glob, language, regions and test filters to both arms before selection. The matching document must appear within the requested first page; irrelevant semantic symbols cannot consume every slot.
4. Do not globally convert every scoped query to lexical or add an `intent` parameter duplicating `backend`. Prefer the existing backend for explicit prose/literal recipes. Keep ordinary unscoped natural-language code search semantic by default.
5. Add `auto_natural_language_code_query_keeps_semantic_primary`, `content_route_preserves_all_requested_filters`, and `content_route_never_rescues_outside_file_pattern`. An explicitly scoped zero result must not return outside-scope rows as though they satisfied the query; any optional suggestions must be separate and labeled.
6. Add a small held-out document/configuration case set to Plan 5's matrix. Compare default, explicit lexical and semantic results before and after, with identical model/index readiness.

**Acceptance:**

- [ ] The live repro's equivalent finds the actual document in the first page without changing the directory name to a special value.
- [ ] Equivalent content in Markdown, configuration data and source comments is governed by metadata, not favored language lists.
- [ ] Explicit lexical remains lexical; explicit semantic/hybrid remain documented symbol searches.
- [ ] Existing natural-language code retrieval passes dogfood and the paired case set; any measured ranking loss is investigated.
- [ ] Backend choice, fallback and content evidence are understandable in both compact and structured responses.

## 2C. Make every next-page request self-contained

**Inputs:** Normalized effective tool arguments and `next_line`. **Produces:** A stateless, escaped next call retaining workspace and all result-affecting arguments.

Use one serializer for existing normalized arguments rather than hand-maintained key lists per renderer. Replace only the offset. Include effective backend/semantics, filters, limits, reference kind, definition inclusion, symbol target/depth/mode, and impact seeds as applicable. A git-diff-seeded impact page must serialize the resolved seed set or clearly restart on a changed diff; it must not silently page a different query.

Proposed exact root-package tests, each RED then GREEN:

- `fast_search_next_replays_all_effective_filters_and_workspace`
- `fast_refs_next_replays_kind_definition_semantics_and_workspace`
- `get_symbols_next_replays_target_mode_depth_limit_and_workspace`
- `blast_radius_next_replays_resolved_seed_and_workspace`
- `paged_request_matches_single_call_slice_on_unchanged_snapshot`

Run each with `cargo nextest run -p julie --lib <exact_name>`. Include spaces, quotes, backslashes and Unicode; parse the emitted request rather than asserting only string fragments.

**Acceptance:**

- [ ] Every emitted next call succeeds independently with no session-default workspace.
- [ ] Concatenated pages match one larger query on unchanged data, without duplicates or missing rows caused by changed arguments.
- [ ] Empty/final pages do not advertise an invalid next page.
- [ ] No durable state or retained cross-request snapshot is added; offset paging under concurrent edits is documented as best effort.

## 2D. Bound body output and identify completeness

**Inputs:** Existing canonical source spans and body extraction; both `get_symbols` and `deep_dive` remain public tools.

**Proposed contract:** Add the same optional zero-based `body_offset` and `body_limit` in source lines to both tools, plus optional `source_hash` supplied by a previous page. Defaults preserve current compact budgets while bounding full bodies. Structured output includes source hash, total source-line count, returned range, and whether the complete canonical declaration/body span was returned. Compact output marks truncation and emits a self-contained next call. These fields are proposed additions, not current APIs.

Use existing source hashing. Reject a supplied hash mismatch with an actionable “source changed; restart at offset 0” response; do not stitch content from different files/versions. Preserve CRLF and UTF-8; line windows must not slice invalid bytes. A complete value declaration means the entire canonical declaration span was returned, including a multiline initializer, not merely its name/signature.

Proposed exact tests: `full_symbol_body_is_bounded_and_reports_remaining_lines`, `body_page_refuses_changed_source_hash`, `complete_value_declaration_requires_full_canonical_span`. Run each through the existing tool/request fixture with root-package exact RED/GREEN commands.

**Acceptance:**

- [ ] Large bodies are bounded, and every omitted line is retrievable with stateless follow-up calls.
- [ ] Whole-span completeness is explicit; a truncated body cannot be mistaken for a complete constant/value.
- [ ] Pages reassemble exactly on unchanged source; stale source is refused without writing anything.
- [ ] Compact/full and CLI/MCP behavior agree, with no persisted cursor or expiry state.

## Verification and handoff

Run lead dev and dogfood after the coherent retrieval batch, and the roadmap's final full gate before merge. Plan 5 owns the fresh product comparison; do not reuse the earlier 23-case winner claim as proof that new ranking changes are good.

## Verification ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
