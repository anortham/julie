# Search Quality Recovery Design

**Date:** 2026-04-21
**Status:** Proposed
**Scope:** Tier 2, staged recovery after the search-quality-hardening branch
**Supersedes:** `docs/plans/2026-04-21-search-quality-hardening-design.md` for future search-quality work

---

## 1. Context

`main` now has the observability slice from the rescue branch:

- `zero_hit_reason` on content zero-hits
- `hint_kind` persistence in `tool_calls.metadata`
- replay harnesses and fixture coverage
- multi-pattern comma and brace `file_pattern` parsing
- shared empty-pattern normalization

That work is worth keeping. It measures the wound without claiming the wound is healed.

The live daemon telemetry in `~/.julie/daemon.db` says the current problem is not the 47-row historical replay. It is the live content zero-hit rate:

| Window | All known-target `fast_search` | Content | Definitions |
|---|---:|---:|---:|
| Last 24h | `53/174 = 30.5%` | `48/134 = 35.8%` | `5/40 = 12.5%` |
| Last 7d | `73/268 = 27.2%` | `66/200 = 33.0%` | `7/68 = 10.3%` |

Live content misses also have a shape:

- `48/48` last-24h content zero-hits carried a `file_pattern`
- `9/48` used pipe-separated patterns like `a|b|c`
- `3/48` used whitespace-separated patterns
- multi-token queries remain a large chunk of misses

The merged observability branch also proved the old plan missed the main causal split. `FilePatternFiltered` is not one thing. In the live content path:

1. `SearchIndex::search_content` ranks unscoped file candidates in `src/search/index.rs`
2. `line_mode_matches` in `src/tools/search/line_mode.rs` applies `file_pattern` after fetch
3. line verification happens after scope filtering

So a `FilePatternFiltered` zero-hit can mean:

- the pattern syntax was wrong
- the caller scoped to the wrong tree
- in-scope files existed, but the fetch window never reached them

That mixed bucket is why the old branch drifted into guesswork.

## 2. Problem Statement

We need to improve live content search on `main` without another mixed-purpose branch.

The next design must do three things in order:

1. remove easy caller-side misses that still show up in live traffic
2. split scoped zero-hits into causes we can act on
3. fix retrieval and line verification only after the split is visible in telemetry

Anything else is noise.

## 3. Goals

1. Reduce live 24h `fast_search(search_target="content")` zero-hit rate from the current `33% to 36%` band to `<= 20%`.
2. Reduce live 24h without-recourse rate to `<= 8%`.
3. Make `file_pattern` misses attributable as one of:
   - separator mistake
   - true out-of-scope request
   - candidate-window starvation
4. Align line verification with Tantivy tokenization enough to shrink the `LineMatchMiss` bucket.
5. Keep changes scoped to the live content path. Do not re-open promotion work.

## 4. Non-Goals

- resurrecting `SearchExecutionKind::Promoted`
- dashboard UI polish beyond any compile or serialization fallout
- a broad tokenizer rewrite across definitions and content
- semantic fallback or embeddings work
- fixing the historical 47-row replay by brute force

The 47-row replay stays as a diagnostic fixture. The live daemon data is the source of truth.

## 5. Design Decisions

### 5.1 Recover common `file_pattern` separator mistakes

We should support top-level `|` as an OR separator in `src/tools/search/query.rs`.

Why:

- it appears in live zero-hit traffic
- it has no useful glob meaning in current practice
- a literal `|` in repo paths is rare enough that the tradeoff is worth it

We should also keep whitespace-separated globs invalid, but return a specific hint when the pattern looks like `a/** b/**`. Literal spaces in a single filename glob must keep working.

### 5.2 Keep `zero_hit_reason` coarse, add a scoped sub-diagnostic

`zero_hit_reason` should stay a pipeline-stage field. It already answers, "where did the run die?"

We need a second field for scoped content misses, not a blown-up `zero_hit_reason` enum. The new field should only appear on content zero-hits with a `file_pattern`.

Proposed shape:

- `file_pattern_diagnostic = WhitespaceSeparatedMultiGlob`
- `file_pattern_diagnostic = NoInScopeCandidates`
- `file_pattern_diagnostic = CandidateStarvation`

This keeps existing telemetry stable while making the mixed `FilePatternFiltered` bucket useful.

`HintKind` is separate. `file_pattern_diagnostic` stores root cause; `hint_kind` stores the single user-facing hint we chose to show. They are related but not interchangeable.

### 5.3 Detect starvation with a bounded wider probe

When a scoped content search zero-hits and every fetched candidate is outside the requested scope, run a wider bounded probe before deciding what happened.

Decision rule:

- if the wider probe still finds no in-scope candidate paths, classify `NoInScopeCandidates`
- if the wider probe finds in-scope candidate paths that the first window missed, classify `CandidateStarvation`

This probe should run only on zero-hit scoped content searches, not on every request.

The probe must also be mechanically possible. The current scoped path already starts with a large fetch window, so the implementation has to change the initial scoped fetch size or add cleaner repeated-window plumbing. "Widen later" without that change is fake progress.

### 5.4 Fix starvation in the live path, not in dead branches

The live content path is `line_mode_matches` in `src/tools/search/line_mode.rs`. That is where scoped retrieval needs repair.

The fix should be an adaptive fetch loop:

1. fetch a ranked window from `search_content`
2. apply scope and verification
3. if no in-scope candidate survived and a `file_pattern` exists, widen the fetch and retry
4. stop when:
   - enough hits are found
   - in-scope candidates are exhausted through verification
   - the hard cap is reached

The initial scoped window must start below the hard cap so starvation is observable. If `src/search/index.rs` needs a small helper to support this cleanly, add one. Do not bury a giant retry state machine in the formatter or telemetry layer, and do not duplicate the loop for primary and target workspaces.

### 5.5 Add an out-of-scope hint only after telemetry proves it

If the live split shows a meaningful `NoInScopeCandidates` bucket, add a targeted content hint:

- keep the request scoped
- do not auto-unscope yet
- tell the caller the search found no candidates inside the requested tree
- point at the next step, broaden or remove `file_pattern`

This improves without-recourse rate without hiding retrieval bugs.

Hint precedence is fixed:

1. syntax hint (`WhitespaceSeparatedMultiGlob`)
2. out-of-scope hint (`NoInScopeCandidates`)
3. multi-token hint

One response gets one hint. Telemetry still carries both `file_pattern_diagnostic` and `hint_kind`.

### 5.6 Align line verification with Tantivy tokenization

`line_matches` in `src/tools/search/query.rs` still leans on lowercase `contains()` checks. Tantivy does tokenization, normalization, and stemming before ranking candidate files.

That mismatch is why `LineMatchMiss` deserves its own task. The verifier should use token-aware matching for token strategies and preserve literal substring behavior for quoted and punctuation-heavy queries.

## 6. Validation Strategy

Validation has three levels:

1. narrow RED to GREEN tests for parser, telemetry, and line-mode behavior
2. `cargo xtask test dev` plus `cargo xtask test dogfood` for each branch that changes search behavior
3. live daemon measurement after release build and daemon restart

Success is measured on live 24h telemetry from `~/.julie/daemon.db`, not on the historical zero-hit graveyard alone.

## 7. Rollout Order

1. separator recovery and syntax hinting
2. scoped-miss diagnostic split
3. starvation fix in `line_mode_matches`
4. out-of-scope content hint if the live split earns it
5. line verifier repair
6. live validation and diagnosis update

That order keeps us honest. We fix what we can prove.
