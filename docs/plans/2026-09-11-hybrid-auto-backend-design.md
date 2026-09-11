# Hybrid Auto Backend Design

**Status:** draft for owner review
**Owner decision (2026-09-11):** start with natural-language-shaped queries only; revisit after live dogfooding.
**Path:** lightweight. This document is the implementation spec.

## Problem

`fast_search` with no `backend` runs lexical search. On the head-to-head natural-language rows (`docs/findings/2026-09-11-head-to-head-miller.md`), lexical scored 6/13 and 2/10 top-5 by task class. Hybrid scored 10/13 and 9/10. Agents rarely pass `backend`, so the default decides what they get.

Hybrid and semantic are symbol-only. They return no file hits and no line hits. Lexical is the only backend that answers path queries, identifier probes, and `regions`. So the default cannot be hybrid for every query.

## Decision

When `backend` is omitted, `fast_search` runs hybrid if every condition below holds. Otherwise it runs lexical exactly as today, with no note in the output.

1. `is_nl_like_query(query)` is true (`crates/julie-index/src/search/scoring.rs:450`). It needs two or more terms, at least one alphabetic, and not every term identifier-shaped.
2. `looks_like_file_or_path_query(query)` is false (`crates/julie-tools/src/search/query.rs:32`).
3. `regions` is not set. This holds by construction: the regions path in `region_search.rs` never calls `execute_search_unified`.
4. The embedding provider answers within the existing 3 second budget and `snapshot_has_embeddings(snapshot)` is true.
5. The hybrid pass returns at least one hit. On zero hits, run the lexical pass as today.

Explicit `backend=lexical|semantic|hybrid` and `semantics=required` keep their current behavior. `deep_dive` and `get_context` are unchanged.

## Change shape

One decision, one place. `No Architecture Impact` beyond the items below.

- `crates/julie-tools/src/search/backend.rs`: `SearchBackend::resolve(requested)` gains the query: `resolve(requested: Option<Self>, query: &str)`. When `requested` is `None` and the query is NL-shaped and not path-shaped, `value` is `Hybrid` and `explicit` stays `false`. Add `pub fn auto_prefers_hybrid(query: &str) -> bool` holding conditions 1 and 2, so tests hit the rule directly.
- `crates/julie-tools/src/search/execution/mod.rs` `execute_search_unified`: the non-lexical branch already handles provider timeout, missing vectors, and `Required`. Two edits:
  - The `backend_fallback` value becomes `params.backend.explicit` only, as today. A silent auto fallback must not set it.
  - After `run_symbol_backend_pass`, when `!params.backend.explicit` and `execution.hits.is_empty()`, fall through to the lexical pass instead of returning.
- `should_try_semantic_zero_hit_fallback` keeps its `value == Lexical` guard. An NL query that resolved to hybrid never reaches it, which is correct: the zero-hit fallback exists for identifier probes.
- `with_backend_fallback_note` (`tool_execution.rs:27`) needs no change. The `strategy_id` `fast_search_hybrid` already labels the output `(hybrid)`.
- Telemetry: `src/handler/search_telemetry.rs` adds `"backend_auto": params.backend.is_none()` next to `backend_fallback`. Phase 6b reads it to see how often auto picked hybrid.
- Callers of `resolve`: update every call site to pass the query. Find them with `fast_refs symbol=resolve` scoped to `crates/julie-tools/src/search/`.
- Docs: `fast_search` description in `src/request_engine/catalog.rs` and the `backend` field doc in `params.rs` say: omitted backend runs hybrid for natural-language queries when embeddings are ready, else lexical. `JULIE_AGENT_INSTRUCTIONS.md` stays under 1,900 characters; change it only if a sentence there contradicts the new default. Mirror any instruction change into `.claude/hooks/julie-routing-block.md`.

## Tests

All in `src/tests/tools/search/backend_param_tests.rs`, which already builds a workspace with a static embedding provider and vectors.

- `auto_prefers_hybrid_for_nl_queries`: table test over the rule. True: `"where does the app create the router"`. False: `"SearchBackend"`, `"parse_query score_candidate"`, `"src/search/backend.rs"`, `"find backend.rs"`, `""`.
- `auto_nl_query_runs_hybrid_when_vectors_are_ready`: omitted backend, NL query, workspace with vectors. `strategy_id == "fast_search_hybrid"`, `backend_fallback == false`.
- `auto_nl_query_stays_lexical_without_vectors`: same query, workspace without vectors. `strategy_id == "search_unified"`, `backend_fallback == false`, output has no fallback note.
- `auto_nl_query_falls_through_to_lexical_on_zero_hybrid_hits`: NL query whose terms match a line but no symbol embedding. `strategy_id == "search_unified"`.
- `auto_identifier_query_stays_lexical_with_vectors`: `"semantic_backend_target"` with vectors resolves lexical.
- `explicit_lexical_never_runs_hybrid`: `backend=lexical`, NL query, vectors ready. `strategy_id == "search_unified"`.

Worker red/green: `cargo nextest run --lib <exact_test_name>`. Lead: `cargo xtask test changed`, then `cargo xtask test bucket tools-search-hybrid`, then `cargo xtask test dev`.

## Gate

Every test binary runs with `JULIE_EMBEDDING_PROVIDER=none`, so `dogfood` and `search_quality` exercise only the lexical path. They guard against regression, not for the new default. The evidence for the default itself comes from the release binary with the sidecar beside it:

1. `python3 docs/eval/semantic-value/run_scorecard.py --backend lexical --backend hybrid` on the ten public repos. Bar from the scorecard README: hybrid beats lexical by at least 20 percent relative MRR. Record the result file path.
2. `python3 docs/eval/head-to-head/run_matrix.py --skip-miller --require-semantics` on the same corpus. Julie `auto` top-5 on `retrieval.concept` and `retrieval.implementation` must match the hybrid column from the 6a run within one row per class (10/13 and 9/10). `inspect.symbol` stays 23/23.
3. `cargo xtask test dogfood` passes.
4. `fast_search` auto p50 latency on the head-to-head rows is recorded. Report only. The 6a lexical p50 was 37 ms; hybrid adds an embedding call.

Precondition for 1 and 2: `julie-semantic-sidecar` must sit beside `target/release/julie-server` (memory note `sidecar-binary-location`). On 2026-09-11 the live service on this machine reported `backend=hybrid unavailable` for the julie checkout, so check this first.

## Acceptance criteria

- [ ] Omitted backend plus NL query plus ready vectors runs hybrid; output is labeled `(hybrid)`; no fallback note.
- [ ] Omitted backend with vectors not ready, provider slow, or zero hybrid hits runs lexical with no note.
- [ ] Path queries, single identifiers, all-identifier queries, `regions`, and explicit backends behave exactly as before.
- [ ] `semantics=required` with omitted backend and no vectors still fails with `SEMANTICS_NOT_READY` only when the query resolved to hybrid; lexical-resolved queries do not fail.
- [ ] Telemetry row carries `backend_auto`.
- [ ] The six tests above pass; `changed`, `tools-search-hybrid`, `dev`, and `dogfood` pass.
- [ ] Scorecard and head-to-head evidence recorded in `docs/findings/2026-09-1X-hybrid-auto-backend.md` with result file paths and the latency number.
- [ ] Tool description, field doc, instructions, and routing block agree on the new default.

## Out of scope

- Hybrid for non-NL queries. Revisit after live dogfooding with the `backend_auto` telemetry.
- Merging lexical line hits with hybrid symbol hits in one result.
- Any change to `deep_dive`, `get_context`, or `fast_refs` semantics.
- Tuning `SearchWeightProfile::fast_search()` weights.
