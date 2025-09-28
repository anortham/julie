⏺ Summary of Symbol Extraction Bug for Another Agent

  Current Problem

  Julie indexes 226 files successfully but stores 0 symbols in the database, despite extractors collecting 5576 symbols. The symbols fail to
  store with: FOREIGN KEY constraint failed

  What's Working

  - ✅ File indexing: 226 files correctly stored in database
  - ✅ Language detection: Correctly identifies Rust, Python, TypeScript, etc.
  - ✅ Symbol extraction: Extractors collect 5576 symbols successfully
  - ✅ Topological sorting: Parent-child ordering works (sorts symbols so parents are inserted before children)
  - ✅ All tests pass: Unit tests for extractors work perfectly

  What We Fixed But Didn't Solve The Problem

  1. Tantivy Lock Issue ✅
    - Workspace was initialized twice causing lock failure
    - Fixed by checking if workspace already loaded
  2. SQLite PRAGMA Issue ✅
    - PRAGMA journal_mode returns results but used execute()
    - Fixed by using execute_batch()
  3. Parent-Child Ordering ✅
    - Added topological sort to insert parents before children
    - Successfully identifies orphaned symbols
  4. Transaction Type ✅
    - Changed from unchecked_transaction() to transaction()
    - Now properly enforces foreign key constraints

  The Mystery

  Despite all fixes, symbols STILL fail with FOREIGN KEY constraint failed. Investigation shows:
  - First symbol attempting insertion: DemoStruct from /Users/murphy/Source/julie/tests/real-world/go/main.go
  - File exists in database with matching path
  - Workspace IDs match perfectly: workspace_316c0b08
  - Parent ID is None (so not a parent reference issue)

  Key Code Locations

  - Bulk storage: /src/database/mod.rs::bulk_store_symbols() (line 518)
  - Extraction: /src/tools/workspace/indexing.rs::extract_symbols_with_existing_tree() (line 340)
  - Foreign key: symbols.file_path references files.path

  Database Schema

  symbols.file_path REFERENCES files(path) ON DELETE CASCADE
  symbols.parent_id REFERENCES symbols(id)  -- Self-referential

  Recent Discovery

  The extraction worked BEFORE it was moved to async/out-of-band execution. The async refactoring may have introduced the issue.

  For The Next Agent

  The core mystery: Why does FOREIGN KEY constraint failed occur when:
  1. The file path exists in the files table
  2. The workspace IDs match
  3. Parent ordering is correct
  4. The transaction properly enforces constraints

  Consider checking:
  - Is there a subtle encoding/normalization issue with file paths?
  - Is the foreign key check looking at a different column than expected?
  - Is there a race condition in the async code?
  - Are the files being deleted/updated between collection and storage?