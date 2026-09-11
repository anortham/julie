# Machine Service Phase 6a Gate

**Date:** 2026-09-11
**Branch:** `contract` at `/home/murphy/source/julie/.worktrees/contract`
**HEAD:** `3d218f4c` (prior to this docs commit).
**Gate checkout:** `/home/murphy/source/julie/.worktrees/contract-gate`, shared `CARGO_TARGET_DIR`. Most branch gates at `c2486db3` (2026-09-11T07:30Z–08:00Z). `dev` and `full` rerun at `3d218f4c` (about 08:10Z).
**Verdict:** passed on 2026-09-11. Compact output and paging, instructions under 1,900 characters plus a session-start routing hook, two unused edit tools deleted, `blast_radius` git-diff seeding, and the head-to-head retrieval matrix are delivered. `fast` median and `full` wall hold. Net lines are negative.

## What 6a shipped

- Task 1: deleted `rewrite_symbol` and `rename_symbol`. Catalog is ten tools.
- Task 2: `tool_calls` records `client`, `client_session`, `julie_version`, and `result_count`.
- Task 3: compact output is the default. Search, refs, symbols, and impact page with `offset` and a `next:` line.
- Task 4: `blast_radius` with no seed reads the working-tree git diff.
- Task 5: `JULIE_AGENT_INSTRUCTIONS.md` is 1,847 characters. Full routing lives in the session-start hook.
- Task 6: head-to-head retrieval matrix against Miller on the ten public repos, including a Julie backend comparison.
- Task 7: this gate finding and ledger.

## What 6b defers

Miller names, inspect and trace folds, and the telemetry rename wait. The condition is two weeks of `tool_calls` from real Julie sessions after 6a merges, compared to the baseline table below, then a decision.

## Gate table

| Budget | Value | Command | Result |
|---|---|---|---|
| `fast` median | under 10 s warm | `cargo xtask test fast` ×3 at `c2486db3` | pass. 3.6s, 3.5s, 3.5s warm. Median 3.5s |
| `full` wall | under 120 s warm | `cargo xtask test full` at `3d218f4c` | pass. 47 buckets, 93.4s warm, cold 98.2s |
| `dev` | batch regression | `cargo xtask test dev` at `3d218f4c` | pass. 30 buckets, 52.4s warm, cold 82.2s |
| `system` | workspace init + integration | `cargo xtask test system` at `c2486db3` | pass. 4 buckets, 8.6s warm |
| `dogfood` | `search_quality` | `cargo xtask test dogfood` at `c2486db3` | pass. 2 buckets, 41.7s warm |
| fmt | clean | `cargo fmt --check` at `c2486db3` | pass |
| clippy | 0 errors | `cargo clippy --workspace --all-targets` at `c2486db3` | pass. 0 errors, 326 warnings (collapsible if, map_or, too many arguments; pre-existing) |
| docs contract | pass | `cargo test -p xtask --test docs_contract_tests` at `c2486db3` | pass. 14/14 |
| Instructions | ≤ 1,900 characters | `wc -c JULIE_AGENT_INSTRUCTIONS.md` | 1,847 |
| Net lines | negative vs `5eafea53` | `tokei` at `5eafea53` and `c2486db3` | pass. −2,974 lines, −2,562 code |

## Net lines table

| Point | Command | Files | Lines | Code |
|---|---|---|---|---|
| `5eafea53` (plan baseline) | `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` | 540 | 98,056 | 84,551 |
| `c2486db3` | same command | 530 | 95,082 | 81,989 |
| Delta | `c2486db3` − baseline | −10 | −2,974 | −2,562 |

## `fast` median and `full` wall

`fast` at `c2486db3`: 3.6s, 3.5s, 3.5s warm. Median 3.5s (under 10 s).

`full` at `c2486db3` failed in bucket `cli` on `test_agent_instructions_recommend_standalone_for_quick_dogfood_checks`. At `3d218f4c`: PASS, 47 buckets, 93.4s warm (cold 98.2s), under 120 s.

## Resident memory

`julie-server service status` with the julie checkout plus the ten corpus workspaces open and semantics on: process RSS 3,452,176 KB (about 3.3 GiB). Sidecar child 172,736 KB (`bge-small`).

## Head-to-head summary

Numbers only. Full write-up: `docs/findings/2026-09-11-head-to-head-miller.md`. Kept pairs: lexical `20260911T040446Z`, semantic `20260911T045558Z`, backends `20260911T050543Z`.

Default Julie `auto` is lexical. Inspect used definition targets on the semantic pair.

| class | n | Julie auto top-1 | Julie auto top-5 | Julie hybrid top-5 | Julie semantic top-5 | Miller top-1 | Miller top-5 |
|---|---|---|---|---|---|---|---|
| retrieval.concept | 13 | 5/13 | 6/13 | 10/13 | 10/13 | 6/13 | 7/13 |
| retrieval.implementation | 10 | 1/10 | 2/10 | 9/10 | 10/10 | 5/10 | 7/10 |
| inspect.symbol | 23 | 20/23 | 23/23 | — | — | 19/23 | 23/23 |

Julie `fast_search` auto p50 37 ms / 589 bytes. Miller `search` p50 250 ms / 1,552 bytes. Julie `deep_dive` p50 5 ms. Miller `inspect` p50 104 ms.

## Telemetry baseline

Read-only query on 2026-09-11 against `~/.julie/registry.db`. Phase 6b compares against this table after two weeks of real sessions.

```
select julie_version, client, tool_name, count(*) calls, sum(success=0) errors, sum(success=1 and result_count=0) empty, round(avg(duration_ms)) ms, round(avg(output_bytes)) bytes from tool_calls where workspace_id not like 'ws__tmp%' and workspace_id not like 'tmp_%' and workspace_id not like 'target_%' group by 1,2,3 order by 4 desc
```

| julie_version | client | tool_name | calls | errors | empty | ms | bytes |
|---|---|---|---|---|---|---|---|
| 7.18.1 | miller-julie-bench/0 | fast_search | 208 | 0 | 7 | 114.0 | 571.0 |
|  |  | edit_file | 171 | 3 | 0 | 2566.0 | 716.0 |
| 7.18.1 | miller-julie-bench/0 | deep_dive | 161 | 0 | 24 | 1.0 | 1053.0 |
|  |  | get_symbols | 72 | 3 | 0 | 2.0 | 2290.0 |
| 7.18.1 | miller-julie-bench/0 | manage_workspace | 71 | 0 |  | 0.0 | 81.0 |
|  |  | fast_search | 29 | 0 | 1 | 144.0 | 1910.0 |
| 7.18.1 |  | manage_workspace | 13 | 0 |  | 0.0 | 81.0 |
|  |  | fast_refs | 10 | 0 |  | 3.0 | 1150.0 |
| 7.18.1 |  | fast_search | 4 | 0 | 0 | 18.0 | 415.0 |
|  |  | manage_workspace | 3 | 1 | 0 | 9060.0 | 689.0 |
|  |  | patterns | 3 | 0 |  | 35.0 | 893.0 |
|  |  | get_context | 2 | 0 |  | 9.0 | 5333.0 |
| 7.18.1 |  | fast_refs | 2 | 0 | 0 | 1.0 | 634.0 |
| 7.18.1 | lead-check/2 | fast_refs | 1 | 0 | 0 | 0.0 | 634.0 |

`miller-julie-bench/0` is the Task 6 harness client. Empty `client` / `julie_version` rows are older calls from before Task 2.

## Follow-ups

- Plugin repo: skill list and hooks after 6a merges (`cargo xtask sync-plugin` reports hook divergence and does not copy hooks).
- Phase 7: continuous testing (design section 14 item 7), as its own design review first.
- Rust `xtask-eval revival` harness: out of Task 6 scope; still deferred.
