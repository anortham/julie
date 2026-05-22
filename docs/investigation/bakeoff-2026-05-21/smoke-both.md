# Search Matrix Baseline Report

- Profile: `smoke`
- Ablation: `ablation:both`
- Executions: `5`
- Skipped repos: `0`

| repo | case_id | ablation | hit_count | zero_hit_reason | file_pattern_diagnostic | hint_kind |
| --- | --- | --- | ---: | --- | --- | --- |
| `julie` | `rust-exact-workspace-pool` | `both` | 500 | `∅` | `∅` | `∅` |
| `julie` | `rust-camelcase-fast-search` | `both` | 360 | `∅` | `∅` | `∅` |
| `julie` | `rust-snake-case-line-matches` | `both` | 10+ | `∅` | `∅` | `∅` |
| `julie` | `rust-scoped-content-ui` | `both` | 10+ | `∅` | `∅` | `∅` |
| `julie` | `rust-file-exact-search-mod` | `both` | 3 | `∅` | `∅` | `∅` |

## Summary Flags

- `unexpected_hint`
