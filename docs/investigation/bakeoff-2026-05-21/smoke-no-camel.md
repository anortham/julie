# Search Matrix Baseline Report

- Profile: `smoke`
- Ablation: `ablation:no-camel`
- Executions: `5`
- Skipped repos: `0`

| repo | case_id | ablation | hit_count | zero_hit_reason | file_pattern_diagnostic | hint_kind |
| --- | --- | --- | ---: | --- | --- | --- |
| `julie` | `rust-exact-workspace-pool` | `no-camel` | 500 | `∅` | `∅` | `∅` |
| `julie` | `rust-camelcase-fast-search` | `no-camel` | 343 | `∅` | `∅` | `∅` |
| `julie` | `rust-snake-case-line-matches` | `no-camel` | 10+ | `∅` | `∅` | `∅` |
| `julie` | `rust-scoped-content-ui` | `no-camel` | 10+ | `∅` | `∅` | `∅` |
| `julie` | `rust-file-exact-search-mod` | `no-camel` | 3 | `∅` | `∅` | `∅` |

## Summary Flags

- `unexpected_hint`
