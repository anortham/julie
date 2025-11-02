# Julie TODO

✅ FIXED (2025-11-01): get_symbols now returns clear "File not found" error vs "No symbols found"

**Previous issue:**
- `get_symbols("src/tests/extractors/typescript.rs")` → "No symbols found"
- `get_symbols("src/tests/extractors/python.rs")` → "No symbols found"

**Reality:** These files don't exist! The actual paths are **directories**:
- `src/tests/extractors/typescript/` (directory with 10 .rs files)
- `src/tests/extractors/python/` (directory with 11 .rs files)

**Fixed:**
- Added file existence check before database query (src/tools/symbols.rs:176-185, 647-656)
- Error messages now distinguish:
  - ❌ "File not found: X" (file doesn't exist)
  - vs "No symbols found in: X" (file exists but has no symbols)
- Added comprehensive test coverage (`test_get_symbols_file_not_found_error`)
- All 9 get_symbols tests passing ✅

**UX Improvement:**
Now when you use `get_symbols` on a non-existent file, you get:
```
❌ File not found: src/does_not_exist.rs
💡 Check the file path - use relative paths from workspace root
```

Instead of the ambiguous "No symbols found" message.
