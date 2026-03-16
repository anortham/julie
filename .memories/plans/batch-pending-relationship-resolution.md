---
id: batch-pending-relationship-resolution
title: Batch Pending Relationship Resolution
status: completed
created: 2026-03-15T23:32:59.209Z
updated: 2026-03-16T00:12:27.181Z
tags:
  - performance
  - indexing
  - batch-resolution
---

# Batch Pending Relationship Resolution

## Goal
Eliminate the O(N) per-relationship bottleneck in `process_files_optimized` by batching database lookups. On Guava (434K pending relationships), this phase takes ~320s (82% of indexing time). Batch resolution should cut it to seconds.

## Approach
Two layers of optimization:
1. **Batch SQL** — `find_symbols_by_names_batch` in `database/symbols/queries.rs`: accepts a slice of names, chunks into groups of ~500 (SQLite param limit), uses `WHERE name IN (?, ?, ...)`, returns `HashMap<String, Vec<Symbol>>`
2. **Grouped resolution** — `resolve_batch` in `resolver.rs`: groups pending relationships by `callee_name`, calls batch query with unique names, runs `select_best_candidate` per pending against cached candidates

## Tasks
1. RED: Write failing test for `find_symbols_by_names_batch`
2. GREEN: Implement `find_symbols_by_names_batch` in `database/symbols/queries.rs`
3. RED: Write failing test for `resolve_batch`
4. GREEN: Implement `resolve_batch` in `resolver.rs`
5. Replace inline loop in `processor.rs:457-484` with `resolve_batch` call
6. Run `cargo xtask test dev` for regression check

## Key Files
- `src/database/symbols/queries.rs` — new batch query method
- `src/tools/workspace/indexing/resolver.rs` — new `resolve_batch` function
- `src/tools/workspace/indexing/processor.rs:457-484` — replace inline loop
- `src/tests/` — new tests
