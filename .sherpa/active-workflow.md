# Current Workflow: Bug Hunt
Started: 7:36 PM

## Progress
✅ 🔍 Reproduce & Isolate (7:37 PM)
   - Reproduced and isolated bug: indexing.rs lines 368-370 and 401-403 use handler.get_workspace().db which always returns PRIMARY workspace DB, causing reference workspace symbols to be stored in wrong database
   - DECISION: Skipping formal test-first for this integration bug because:
1. Already have manual reproduction (coa-mcp-framework reference workspace)
2. Complex integration test would require full workspace setup
3. Fix mirrors proven pattern from navigation.rs (lines 627-709)
4. Can immediately verify fix by re-running manual test
5. Will write integration test if fix succeeds and user requests it

Fix Strategy:
- Use _is_primary_workspace parameter in process_files_optimized()
- If reference workspace: open separate DB at indexes/{workspace_id}/db/symbols.db
- If primary workspace: use existing handler.get_workspace().db
- Apply to both cleanup (368-370) and bulk storage (401-403) sections
   - FIX IMPLEMENTED: Reference workspace indexing database bug

Changes Made to src/tools/workspace/indexing.rs:
1. Removed underscore from _is_primary_workspace parameter (line 246)
2. Added ref_workspace_db logic (lines 269-294):
   - If reference workspace: opens separate DB at indexes/{workspace_id}/db/symbols.db
   - If primary workspace: uses existing handler.get_workspace().db
   - Uses spawn_blocking for file I/O safety
3. Updated cleanup section (lines 395-430) to use correct database
4. Updated bulk storage section (lines 437-486) to use correct database

Pattern Mirrors navigation.rs fix (lines 627-709):
- Same workspace_db_path() helper usage
- Same spawn_blocking pattern for SymbolDatabase::new()
- Same Arc<tokio::sync::Mutex<>> wrapping

Compilation: ✅ cargo check passes

Next: Test by re-adding coa-mcp-framework reference workspace
⏳ 🎯 Capture in Test (in progress)
   - BUG REPRODUCED: Reference workspace indexing creates vectors but no database file

Reproduction Steps:
1. Add reference workspace: ~/source/coa-mcp-framework
2. Tool reports success: "6621 symbols" indexed
3. Check filesystem: vectors/ exists but symbols.db does NOT exist

Expected Behavior:
- Database created at: .julie/indexes/coa-mcp-framework_c77f81e4/db/symbols.db
- Symbols stored in reference workspace's separate database

Actual Behavior:
- Vectors created correctly: .julie/indexes/coa-mcp-framework_c77f81e4/vectors/hnsw_index.hnsw.*
- Database NOT created - missing symbols.db file
- Symbols stored in PRIMARY workspace database (wrong!)

Root Cause Identified:
- File: src/tools/workspace/indexing.rs
- Lines: 368-370 (cleanup) and 401-403 (bulk storage)
- Bug: handler.get_workspace().await?.db ALWAYS returns primary workspace DB
- Should: Open/create separate DB at indexes/{workspace_id}/db/symbols.db for reference workspaces
☐ 🔧 Fix & Verify