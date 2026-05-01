

## Task 12 — Acceptance replay (FastSearchTool end-to-end)

_Replay harness: `cargo nextest run --lib acceptance_replay_against_captured_zero_hits -- --ignored`_

* Fixture: `fixtures/search-quality/zero-hit-replay-task3.json`
* Entries replayed: 47
* Still zero hits after full pipeline: **13** (27.7%) — ceiling 20%
* Zero hits without an actionable hint (without-recourse): **0** (0.0%) — ceiling 8%
* Fixture entries with `limit_param > 500` (would hit the tool clamp): **0**
* Multi-token zero-hit hints: **4**

### Zero-hit reason distribution

| reason | count |
| --- | ---: |
| `line_match_miss` | 7 |
| `tantivy_no_candidates` | 3 |
| `unattributed` | 3 |

### Hint distribution on zero-hit results

| hint | count |
| --- | ---: |
| `definitions_target_hint` | 3 |
| `file_pattern_syntax_hint` | 3 |
| `file_target_hint` | 3 |
| `multi_token_hint` | 4 |

