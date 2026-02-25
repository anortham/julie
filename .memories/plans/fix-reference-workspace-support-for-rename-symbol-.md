---
id: fix-reference-workspace-support-for-rename-symbol-
title: Fix Reference Workspace Support for rename_symbol and get_context
status: completed
created: 2026-02-25T02:49:46.611Z
updated: 2026-02-25T03:10:40.797Z
tags:
  - ref-workspace
  - rename-symbol
  - get-context
  - bug-fix
---

# Fix Reference Workspace Support for rename_symbol and get_context

## Tasks
1. Fix `rename_symbol` path resolution — thread workspace param, resolve correct root
2. Add reference workspace support to `get_context` — remove hard block, add workspace routing
3. TDD tests for both fixes

## Key Files
- `src/tools/refactoring/rename.rs` + `mod.rs`
- `src/tools/get_context/pipeline.rs`
- Pattern: `src/tools/navigation/deep_dive/mod.rs` (reference workspace routing)
- Pattern: `src/tools/symbols/reference.rs` (WorkspaceRegistryService lookup)

