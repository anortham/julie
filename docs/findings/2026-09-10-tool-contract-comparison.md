# Tool contract comparison: Julie names versus Miller names

**Date:** 2026-09-10
**Question:** Design section 9 says the ten Miller tools become Julie's public contract and Julie's names retire. What pushes that decision, and what evidence backs it?
**Verdict:** The evidence supports adopting Miller's tool *shape* (fewer tools, `depth`/`mode`/`operation` parameters, bounded output). It does not prove that Miller's *names* are better than Julie's. Names are the cheap part. The push comes from where the usage, guidance, and tuning already live, not from a measured naming A/B.

## What the design says

`docs/plans/2026-09-09-machine-service-design.md` section 9: "The ten Miller tools carry over as the public contract ... Parameter shapes, compact output, and JSON contracts stay as Miller documents them. Julie's tool names retire behind that contract." Section 1 gives the reason for the machine service, not for the names. No document records a naming comparison. The revival assessment (`docs/findings/2026-09-07-julie-revival-assessment.md`, line 120) lists "a narrow tool workflow with concise answers" as a *hypothesis*, not a result.

## The two surfaces

| Julie (12 tools) | Miller (10 tools) | Shape difference |
|---|---|---|
| `fast_search` | `search` with `mode=auto\|text\|symbol\|file\|markers\|content\|source\|external\|web\|all-text`, `retrieval=` | Same job. Miller folds region and marker searches into modes. |
| `get_symbols`, `deep_dive` | `inspect` with `depth=summary\|overview\|full` | Two tools become one with a depth knob. |
| `fast_refs`, `call_path` | `trace` with `mode=refs\|path\|bridge` | Two tools become one with a mode knob. `bridge` is new. |
| `blast_radius` | `impact`; no arguments means the working-tree git diff | Same job. The no-arg form is the one agents use after edits. |
| `get_context` | `context` | Same job and same task inputs. |
| `patterns` | `patterns` | Same. |
| `edit_file`, `rewrite_symbol`, `rename_symbol` | `edit` with `operation=replace_text\|replace_symbol_body\|replace_symbol_signature\|rename_symbol\|insert_before\|insert_after\|add_doc`; preview by default, `apply=true` | Three tools become one. Rename refuses without exact evidence. |
| `manage_workspace` | `workspace` | Same. |
| `spillover_get` | none; `content` covers large text | Julie pages big results; Miller bounds output and puts large text behind `content`. |
| none | `content` | Import and search logs, CI output, web pages. Not in Julie. |
| none | `tests` | Continuous testing. Not in Julie. Phase 7. |

Miller's own migration guide (`~/source/miller/docs/migration-from-julie.md`) lists this mapping and states the policy: "Miller exposes nine MCP tools rather than mirroring Julie's thirteen names."

## Telemetry

**Miller, `~/.miller/telemetry.db`, 2026-08-10 to 2026-09-10.** 78,993 calls over 60 workspaces (56,619 in August, 22,374 in September).

| tool | calls | share | errors | avg ms | avg bytes returned |
|---|---:|---:|---:|---:|---:|
| `inspect` | 33,107 | 41.9% | 561 | 1,138 | 3,037 |
| `search` | 32,669 | 41.4% | 994 | 499 | 1,749 |
| `workspace` | 3,570 | 4.5% | 42 | 3,932 | 2,142 |
| `content` | 2,413 | 3.1% | 33 | 274 | 4,190 |
| `impact` | 2,342 | 3.0% | 97 | 6,036 | 3,426 |
| `trace` | 1,794 | 2.3% | 75 | 1,362 | 2,664 |
| `context` | 1,343 | 1.7% | 145 | 24,410 | 5,176 |
| `edit` | 1,004 | 1.3% | 128 | 3,826 | 842 |
| `patterns` | 469 | 0.6% | 2 | 276 | 3,978 |
| `tests` | 282 | 0.4% | 2 | 10,978 | 1,430 |

What the breakdown by operation says about agent behavior:

- `inspect depth=full` is 19,861 of 33,107 inspect calls (60%). Agents read whole bodies most of the time even though the guidance says start with `overview`.
- `search mode=source` 12,593, `content` 9,063, `symbol` 8,010, `file` 1,297, `auto` 812. Agents pick a mode; the auto route is rarely used.
- `impact` with `git_diff` averages 17.3 s and `changed_paths` 12.2 s. `context` averages 24.4 s. `workspace open` averages 25.7 s. These are Miller's slow paths and they are the tools the design puts on Julie's engine.
- The June baseline (`~/source/miller/docs/findings/2026-06-16-tool-usage-telemetry-baseline.md`) showed the same split: inspect plus search were 82.8% of calls then and are 83.3% now.

**Julie, `~/.julie/registry.db`, this machine.** 88 calls, all from today's session (the revival assessment found zero rows on 2026-09-07). `fast_search` 41 calls at 107 ms average, `get_symbols` 29 at 6 ms, `fast_refs` 10 at 3 ms, `get_context` 2 at 199 ms, `manage_workspace` 3 at 9.6 s. Too small to compare with Miller. It does show the new engine's read paths are fast on this repo.

**Search quality bakeoff, `docs/plans/bakeoff-raw-results.json`, June 2026, 135 queries, older versions of both.** Top-5 hit rate overall: Julie 90.4%, Miller 62.2%. By category: exact symbol Julie 100% vs Miller 97%; symbol intent Julie 78% vs Miller 97%; file/path Julie 92% vs Miller 8%; documentation phrase Julie 88% vs Miller 36%; likely test Julie 100% vs Miller 0%. This measures engines, not names. It is the case for Julie's Tantivy engine under whichever contract.

## What pushes the decision toward Miller's contract

1. **Where the tuning happened.** Miller's descriptions, compact renderers, and output caps were shaped against 79k logged calls and three usefulness reviews (`2026-06-05-tool-output-token-savings.md`, `2026-06-16-tool-usage-telemetry-baseline.md`, `2026-09-07-agent-usefulness-review.md`). `impact` output went from 11,451 to 2,778 bytes through that loop. Julie's descriptions had no such loop; the machine had no Julie telemetry until today.
2. **Where the guidance lives.** Razorback's skills, the routing block, the session hooks, and the user's muscle memory all name `search`, `inspect`, `context`, `trace`, `impact`. Keeping Julie's names means rewriting that guidance or living with a translation layer.
3. **Fewer tools with knobs.** Ten tools with `depth`, `mode`, and `operation` give an agent a smaller menu than thirteen flat names. The telemetry shows agents do use the knobs (`depth=full` 60%, explicit search modes 97%).
4. **Miller already published a migration guide from Julie's names.** Users who moved followed it. Reverting the names would be a second migration for them.

## What the evidence does not show

- No A/B of names. Nothing measures `fast_search` against `search` with the same engine and descriptions.
- Miller's contract is not finished. The 2026-09-07 review found the routing core too terse, jargon pinned by tests, silent `limit` clamps, and undocumented modes. Adopting the contract means adopting that backlog.
- Miller's slow tools (`context`, `impact` on a diff, `workspace open`) are slow because of Miller's store, not because of the names. The numbers say nothing about what they will cost on Julie's engine.
- Julie has `spillover_get` paging; Miller has none. The design must say whether bounded output plus `content` replaces paging or whether paging survives under the Miller name.

## Options

| Option | Cost | What you get |
|---|---|---|
| A. Miller contract as designed (names, shapes, compact output) | Rename 12 Julie tools, port Miller's renderers and descriptions, update `JULIE_AGENT_INSTRUCTIONS.md`, skills, hooks. One migration for Julie users, none for Miller users. | Reuse of every tuned description and cap; guidance already matches. |
| B. Keep Julie names, adopt Miller shapes (`depth`, `mode`, `operation`, bounded output) | Same consolidation work, plus a rewrite of razorback and hook guidance to Julie names. Miller users migrate back. | Julie identity. No other measured gain. |
| C. Miller contract, but decide per tool from telemetry | Same as A, plus a two-week Julie telemetry sample first. | A measured basis for the two open shapes: paging versus bounded output, and `trace auto`. |

## Recommendation

Take option A for the shape and the names, with one change to the phase 6 plan: collect Julie telemetry through `registry.db` `tool_calls` for two weeks of dogfooding first, and use it to settle paging versus bounded output and the `inspect depth` default. The naming choice is not where the value is; the shape and the tuned descriptions are. Renaming is a day of work either way. Building a second tuning loop for Julie's names is not.

## Sources

- `docs/plans/2026-09-09-machine-service-design.md` sections 1 and 9
- `docs/findings/2026-09-07-julie-revival-assessment.md` lines 120 to 138
- `docs/plans/bakeoff-raw-results.json`
- `~/.miller/telemetry.db` (`tool_telemetry`, read-only queries on 2026-09-10)
- `~/.julie/registry.db` (`tool_calls`, 88 rows)
- `~/source/miller/docs/migration-from-julie.md`, `docs/agent-guidance.md`, `docs/findings/2026-06-05-tool-output-token-savings.md`, `docs/findings/2026-06-16-tool-usage-telemetry-baseline.md`, `docs/findings/2026-09-07-agent-usefulness-review.md`
