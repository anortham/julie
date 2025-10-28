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

### Phase 2: Relative Unix-Style Path Storage (✅ COMPLETE)
**Goal**: Store Unix-style relative paths in database to save 350+ tokens per search result

**Token savings analysis:**
- Previous: 50 symbols × 15 tokens/path = 750 tokens just for paths
- Current: 50 symbols × 8 tokens/path = 400 tokens for paths
- **Savings: ~350 tokens per search (7-12% of total response)**

**Benefits achieved:**
- ✅ Eliminates Windows UNC `\\?\C:\` prefix spam (70 chars → 25 chars typical)
- ✅ No JSON backslash escaping (doubles character count on Windows)
- ✅ Human-readable tool results (`src/tools/search.rs` vs `\\?\C:\Users\murphy\source\julie\src\tools\search.rs`)
- ✅ Grep-friendly logs and debugging output
- ✅ Unix-style paths work on all platforms (Path::join handles `/` on Windows)

**Key Design Decisions:**
- ✅ **Unix-style storage**: Always store with `/` separators (platform-independent)
- ✅ **No migration**: Breaking change requires workspace reindex (documented in CLAUDE.md)
- ✅ **No cross-platform DB sharing**: Not needed, limitation documented
- ✅ **Path conversion at boundaries**: Convert on extract (to relative) and file ops (to absolute)

**Implementation Completed:**

#### Phase 2.1: Path Utilities ✅ COMPLETE
- ✅ Created `src/utils/paths.rs` module with helper functions
- ✅ Created `docs/RELATIVE_PATHS_CONTRACT.md` - comprehensive contract document
- ✅ Implemented BaseExtractor path conversion in constructor
- ✅ Cross-platform path handling via std::path (no external crate needed)
- ✅ Tests: TypeScript relative_paths tests validate behavior (3 tests passing)

#### Phase 2.2: Extractor Updates ✅ COMPLETE
- ✅ Updated all 25 language extractors to accept `workspace_root: &std::path::Path` parameter
- ✅ Updated BaseExtractor constructor to convert absolute → relative Unix-style
- ✅ Production indexing code updated (`src/tools/workspace/indexing/extractor.rs`)
- ✅ CLI parallel extraction updated (`src/cli/parallel.rs`)
- ✅ Watcher incremental indexing updated (`src/watcher/mod.rs`)
- ✅ Updated all test files across 4 agent batches (CSS, Bash, PowerShell, Razor, Regex, Zig)
- ✅ Fixed 25 compilation errors across production and test code

#### Phase 2.3: Tool Result Updates ✅ AUTOMATIC
- ✅ Tools automatically return relative paths from database (no changes needed)
- ✅ JSON output automatically smaller (extractors store relative paths)

#### Phase 2.4: Tool Input Updates - NOT NEEDED YET
- Note: Tools will need updates when they receive relative paths from agents
- Deferred until actual usage patterns emerge
- Pattern ready: `let absolute = workspace_root.join(&relative_path);`

#### Phase 2.5: Testing & Validation ✅ COMPLETE
- ✅ TDD GREEN phase: 3 TypeScript relative_paths tests passing
- ✅ Full test suite: **1158 tests passing** (up from 636 baseline)
- ✅ Real-world validation: 44 files processed across 23 languages successfully
- ✅ All 25 extractors validated via test suite
- ✅ Cross-platform handling via std::path (works on Windows, Linux, macOS)

**Total effort**: ~18 hours across 2 sessions
**Migration**: ⚠️ **BREAKING CHANGE** - Requires workspace reindex (documented in CLAUDE.md)
**Token savings**: 7-12% per search result, compounds over conversation
**Test coverage**: 1158 passing tests validate correctness

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
