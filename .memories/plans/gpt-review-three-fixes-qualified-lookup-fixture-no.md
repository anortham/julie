---
id: gpt-review-three-fixes-qualified-lookup-fixture-no
title: GPT Review — Three Fixes (qualified lookup, fixture noise, rename preview)
status: completed
created: 2026-03-16T17:15:48.734Z
updated: 2026-03-16T17:31:57.772Z
tags:
  - bugfix
  - search-quality
  - ux
  - gpt-review
---

# GPT Review — Three Fixes

## Goal
Fix three gaps from GPT's external review of Julie:
1. **Qualified symbol lookup** — wire `parse_qualified_name` into `deep_dive` and `fast_refs` so `SearchIndex::search_symbols` resolves
2. **Fixture/benchmark noise** — harsher NL penalty (0.95→0.75) + add `benchmarks` to `is_fixture_path`
3. **Rename preview** — line-level diff in `rename_symbol` dry-run output

## Key Files
- `src/tools/deep_dive/data.rs:51-74` — `find_symbol` (Task 1)
- `src/tools/navigation/fast_refs.rs:225-290` — `find_references_and_definitions` (Task 2)
- `src/search/scoring.rs:25,227-238` — penalty constant + `is_fixture_path` (Task 3)
- `src/tools/refactoring/mod.rs` + `rename.rs` — rename dry-run (Task 4)

## Status
- [ ] Task 1: Qualified names in deep_dive
- [ ] Task 2: Qualified names in fast_refs
- [ ] Task 3: Fixture/benchmark scoring
- [ ] Task 4: Rename line-level preview
- [ ] Final verification: `cargo xtask test dev`

## Plan Document
`docs/superpowers/plans/2026-03-16-gpt-review-three-fixes.md`
