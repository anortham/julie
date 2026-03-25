## Gotchas

- **Tantivy OR-mode bug**: `Occur::Should` clauses become optional when `Must` clauses exist in the same `BooleanQuery`. Fix: wrap OR clauses in a nested `BooleanQuery`.
- **sqlite-vec vec0 virtual tables** don't support `INSERT OR REPLACE`; must DELETE then INSERT.
- **Atomic saves vs file watcher**: Editors do write-temp, delete-original, rename-temp. DELETE handler must check `path.exists()` before purging symbols. Also `should_index_file()` calls `path.is_file()` which is false for deleted files; need separate `should_process_deletion()`.
- **SQL cross-table column comparisons** can silently destroy query plans. Adding `AND s_test.language = s_prod.language` to test_coverage caused SQLite to pick `idx_symbols_language` over `idx_symbols_name`, turning <1s into 3+ minutes. Always check `EXPLAIN QUERY PLAN`.
- **CodeRankEmbed on Windows DirectML**: RoPE uses unsupported tensor ops, produces zero vectors silently. Use Jina-code-v2 via ORT+DirectML instead.
- **UTF-8 truncation**: `truncate_on_word_boundary` must use char boundaries, not byte indices. CJK/Cyrillic doc comments will panic otherwise.
- **OneDrive sync storm**: Embedding cache on Windows must use `LOCALAPPDATA`, not `$HOME/.cache/` (which is OneDrive-synced).
- **WMI AdapterRAM 32-bit overflow**: `uint32` wraps at 4GB. Try `nvidia-smi` first; treat WMI < 2GB as wrapped.
- **`spawn_blocking` survives `JoinHandle::abort()`**: Killing an outer async task does NOT cancel inner `spawn_blocking` threads. Must use `AtomicBool` cancellation flag checked between batches.

## Decisions

- **Resolver uses soft penalty, not hard filter**: Import-constrained call-edge filtering gives +200 boost (not hard reject) when caller file has identifier references to candidate's parent type. Hard filter broke same-package Java, implicit Python imports, C headers.
- **CodeRankEmbed (768d) default on macOS/Linux sidecar**: +10% namespace overlap, +20% cross-language vs BGE-small. Windows uses Jina-code-v2 via ORT+DirectML (no sidecar needed).
- **Language detection single source of truth**: `crates/julie-extractors/src/language.rs`. Adding a new language requires editing only this one file.
- **`lib/` and `packages/` are NOT vendor directories**: Removing them from vendor detection was critical; they are primary source dirs in Elixir/Ruby/Dart and JS monorepos respectively.

## Open Questions

- **NL query vocabulary gap**: Code embedding models match tokens, not semantic synonyms ("save" != "record", "database" != "SQLite"). Enrichment (callees, fields, docs) helps but cannot bridge English synonym boundaries. Needs NL query expansion or dual-model approach.
- **C# property name extraction bug**: `extract_property` in `members.rs` uses `.find(|c| c.kind() == "identifier")` which grabs the type identifier instead of the property name when the type is a plain identifier.
