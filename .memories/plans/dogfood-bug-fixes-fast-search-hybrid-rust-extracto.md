---
id: dogfood-bug-fixes-fast-search-hybrid-rust-extracto
title: "Dogfood Bug Fixes: fast_search Hybrid + Rust Extractor Qualified Paths"
status: active
created: 2026-02-27T01:58:58.968Z
updated: 2026-02-27T01:58:58.968Z
tags:
  - dogfood
  - bug-fix
  - fast-search
  - rust-extractor
---

# Dogfood Bug Fixes

## Bugs
1. **fast_search semantic fallback is dead** — threshold `< 3` never triggers because Tantivy always returns 3+ results for NL queries. Fix: route NL definition queries through `hybrid_search()` instead.
2. **fast_refs misses qualified Rust calls** — `crate::module::func()` indexed as full path, not bare name. Fix: extract last segment from `scoped_identifier` nodes.

## Plan
See: `docs/plans/2026-02-27-dogfood-bug-fixes.md`

## Tasks (7 total, 2 parallel tracks)
- Track A (Bug 1): Tasks 1-3 — hybrid_search for NL queries
- Track B (Bug 2): Tasks 4-6 — scoped_identifier extraction
- Task 7: Full verification
