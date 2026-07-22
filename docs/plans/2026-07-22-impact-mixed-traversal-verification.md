# Impact Mixed Traversal Verification

## Promotion Contract

- Hard gate: every expected internal symbol in the curated corpus is found.
- Hard gate: no unexpected internal symbol links are emitted.
- Hard gate: default-mode compact output remains byte-identical to the pre-change snapshot.
- Report only: per-case recall and default/web latency p50 and p95.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Curated corpus covers all approved categories and validates its references | `cargo nextest run --lib phase3_mixed_traversal_corpus_is_complete` | worker-exact-baseline | `1d516b9b5af2bbebbab5dd99686148eba3ae9b1a` | pass: 1 passed | 2026-07-22T18:24:50Z | no |
| Default compact output is locked before production behavior changes | `cargo nextest run --lib phase3_default_mode_matches_legacy_snapshot` | worker-exact-baseline | `1d516b9b5af2bbebbab5dd99686148eba3ae9b1a` | pass: 1 passed | 2026-07-22T18:24:50Z | no |
| Pre-change scorecard records gaps without unexpected internal links | `cargo nextest run --lib phase3_mixed_traversal_scorecard --no-capture` | evaluation-baseline | `1d516b9b5af2bbebbab5dd99686148eba3ae9b1a` | pass harness; hard gates: default unchanged, unexpected internal 0; HTTP missing fetch_user/page_loader; SQL missing report_page; report-only p50/p95 us default 1456/1587, web 1315/1459 | 2026-07-22T18:24:50Z | no |
