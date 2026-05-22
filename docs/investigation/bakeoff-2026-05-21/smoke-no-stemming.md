# Search Matrix Baseline Report

- Profile: `smoke`
- Ablation: `ablation:no-stemming`
- Executions: `5`
- Skipped repos: `0`

| repo | case_id | ablation | hit_count | zero_hit_reason | file_pattern_diagnostic | hint_kind |
| --- | --- | --- | ---: | --- | --- | --- |
| `julie` | `rust-exact-workspace-pool` | `no-stemming` | 500 | `∅` | `∅` | `∅` |
| `julie` | `rust-camelcase-fast-search` | `no-stemming` | 326 | `∅` | `∅` | `∅` |
| `julie` | `rust-snake-case-line-matches` | `no-stemming` | 10+ | `∅` | `∅` | `∅` |
| `julie` | `rust-scoped-content-ui` | `no-stemming` | 10+ | `∅` | `∅` | `∅` |
| `julie` | `rust-file-exact-search-mod` | `no-stemming` | 3 | `∅` | `∅` | `∅` |

## Summary Flags

- `unexpected_hint`
