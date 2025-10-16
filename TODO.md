# TODO

No outstanding issues at this time.

## Recently Resolved

- ✅ **FTS5 Dot Character Handling** (2025-10-16)
  - Bug was already fixed in `sanitize_fts5_query` function
  - Queries with dots are split and OR'd to match tokenized content
  - Comprehensive test coverage added to `fts5_sanitization.rs`
  - All 5 FTS5 tests passing
