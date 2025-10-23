# Julie Development TODO

## ✅ Completed Items (2025-10-23)

### FTS5 Database Corruption Fixed
All searches that were failing with `fts5: missing row 550 from content table` now work properly. The query preprocessor fixes resolved this issue.

### Documentation Updates
**CLAUDE.md** updated to reflect tool changes:
- Removed `SmartRefactorTool` references
- Added `RenameSymbolTool` and `EditSymbolTool` documentation
- Updated module structure and test descriptions

### Workspace Refresh Incremental Update
**Fixed:** `refresh` operation now uses incremental updates instead of force reindex
- File: `src/tools/workspace/commands/registry.rs:542`
- Changed: `index_workspace_files(..., true)` → `index_workspace_files(..., false)`
- **User impact:**
  - `manage_workspace(operation="refresh")` → Fast incremental update (only changed files)
  - `manage_workspace(operation="index", force=true)` → Full reindex (all files)

### macOS GPU Acceleration Investigation
**Result:** CoreML disabled for transformer models
- **Problem:** Only 25% of BERT operations can use Neural Engine
- **Solution:** CPU-only mode is 10x faster than CoreML hybrid execution
- **Performance:** Consistent 0.3-3s batches vs 11-26s spikes with CoreML
- **Documentation:** See `docs/GPU_ACCELERATION_PLAN.md` for full analysis

---

## 🚧 Remaining Work

### Code Cleanup - TODOs, Stubs, and Garbage
**Priority:** Medium
**Status:** Not Started

We need to systematically go through the codebase and:
1. Find all TODO comments
2. Categorize them:
   - Implement now (critical)
   - Document for future (roadmap)
   - Delete (obsolete)
3. Remove stub functions and dead code
4. Document why incomplete features exist

**Next steps:**
- Search for all `TODO`, `FIXME`, `XXX`, `HACK` comments
- Audit stub implementations
- Clean up test scaffolding
- Document architectural decisions for incomplete features
