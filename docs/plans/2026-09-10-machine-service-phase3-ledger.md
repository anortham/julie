# Machine Service Phase 3 Verification Ledger

Plan: `docs/plans/2026-09-10-machine-service-phase3-plan.md`. Reuse a row only when the scope label and the commit SHA match the current HEAD exactly.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Tasks 1-3 (facts crate, graph, snapshot/store double write) keep the dev tier green | `cargo xtask test dev` | affected-change | 3e43fec4 | pass (30 buckets, 58.5s warm) | 2026-09-10T12:55:00Z | no |
| Batch A (fast_search, get_symbols, patterns, fast_refs, call_path on the snapshot) plus the Task 2 fix round keep the dev tier green | `cargo xtask test dev` | affected-change | 4e4a1250 | pass (30 buckets, 50.6s warm; Batch B edits were in flight in the working tree, so the Batch B row supersedes this one) | 2026-09-10T13:25:00Z | no |
| Batch B (deep_dive, blast_radius, get_context on the snapshot) keeps every dev bucket green except tools-search-hybrid, whose test module is held empty until Task 10 ports it | `cargo xtask test dev` (16 buckets), then `cargo xtask test bucket <name>` for the 13 buckets after the hybrid failure | affected-change | f16515dd | pass on 29 of 30 buckets; tools-search-hybrid "no tests to run" (Task 10 gate supersedes) | 2026-09-10T13:35:05Z | no |
