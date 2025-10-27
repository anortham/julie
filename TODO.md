# TODO - Julie Development Tasks

## Current Active Development

### Phase 1: Workspace Root Propagation (✅ COMPLETE)
**Goal**: Fix workspace detection to work correctly with MCP clients that set JULIE_WORKSPACE env var

**Issues fixed:**
1. ✅ `get_workspace_root()` now propagates correct path to workspace initialization
2. ✅ `perform_auto_indexing()` uses passed workspace_root parameter
3. ✅ Tilde expansion for `JULIE_WORKSPACE=~/projects/foo` paths
4. ✅ Path canonicalization prevents duplicate workspaces
5. ✅ **INCREMENTAL INDEXING FIX**: `ManageWorkspaceTool` now respects `JULIE_WORKSPACE` env var
6. ✅ **INCREMENTAL INDEXING FIX**: `resolve_workspace_path()` checks env var before falling back to `current_dir()`

**Changes made:**
- ✅ Pass `workspace_root` to `perform_auto_indexing(&handler, &workspace_root)`
- ✅ Update `perform_auto_indexing` to use passed workspace_root instead of `current_dir()`
- ✅ Add `shellexpand::tilde()` to `get_workspace_root()` for env var paths
- ✅ Add `.canonicalize()` to `get_workspace_root()` return value
- ✅ **NEW**: Fix `ManageWorkspaceTool::handle_index_command()` to use initialized workspace root or JULIE_WORKSPACE env var
- ✅ **NEW**: Fix `resolve_workspace_path()` to check JULIE_WORKSPACE env var before current_dir()
- ✅ Add comprehensive tests for workspace detection scenarios (10 tests, all passing)

**Test coverage:**
- `test_workspace_detection_priority` - Verifies CLI > env var > cwd priority
- `test_env_var_concept` - Tests env var when cwd is different (VS Code edge case)
- `test_tilde_expansion_in_env_var` - Tests ~/path expansion
- `test_path_canonicalization` - Tests duplicate prevention
- `test_workspace_init_with_explicit_path` - Tests explicit path initialization
- `test_nonexistent_env_var_fallback` - Tests graceful fallback
- `test_forward_slashes_on_windows` - Tests cross-platform path handling
- **NEW**: `test_incremental_indexing_respects_env_var` - Full integration test of incremental indexing with JULIE_WORKSPACE
- **NEW**: `test_resolve_workspace_path_respects_env_var` - Unit test for resolve_workspace_path env var handling

**Why this matters**: Now works correctly with ANY MCP client that sets `JULIE_WORKSPACE`, not just those that also set the working directory. Both startup indexing AND incremental/manual indexing now respect the env var.

### Phase 2: Relative Path Storage (PLANNED)
**Goal**: Store relative paths in database to save 350+ tokens per search result

**Token savings analysis:**
- Current: 50 symbols × 15 tokens/path = 750 tokens just for paths
- Proposed: 50 symbols × 8 tokens/path = 400 tokens for paths
- **Savings: ~350 tokens per search (7-12% of total response)**

**Additional benefits:**
- Platform-independent paths (no Windows UNC `\\?\C:\` prefix)
- Workspace portability (can move workspace folder)
- Human-readable tool results
- JSON escaping savings (no doubled backslashes)

**Implementation steps:**
1. Database migration to store relative paths (workspace_id + relative_path composite key)
2. Update all 25 extractors to convert absolute → relative during symbol extraction
3. Add path conversion utilities (`to_relative()`, `to_absolute()`)
4. Update tool results to return relative paths
5. Update file operation tools to resolve relative → absolute when needed
6. Comprehensive testing across all platforms

**Estimated effort**: 8-12 hours
**Token savings**: Massive win for long-term context efficiency

---

### FTS Search Issues

still seeing fts errors when searching for "." in query like ".julie"

---

## Future Considerations

1. Consider adding automatic FTS health checks on startup
2. Add defensive checks to detect orphaned FTS rows and NULL content
3. Better error messages when snippet() fails (detect NULL content, suggest re-index)
4. Investigate incremental indexing NULL content issue - why do some updates set content to NULL?

---

## Completed Work

See git history for completed features and bug fixes.
