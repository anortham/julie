## Gotchas

- **Tantivy OR-mode bug**: `Occur::Should` clauses become optional when `Must` clauses exist in the same `BooleanQuery`. Fix: wrap OR clauses in a nested `BooleanQuery`.
- **sqlite-vec vec0 virtual tables** don't support `INSERT OR REPLACE`; must DELETE then INSERT.
- **Atomic saves vs file watcher**: Editors do write-temp, delete-original, rename-temp. DELETE handler must check `path.exists()` before purging symbols. Also `should_index_file()` calls `path.is_file()` which is false for deleted files; need separate `should_process_deletion()`.
- **SQL cross-table column comparisons** can silently destroy query plans. Always check `EXPLAIN QUERY PLAN` when joining symbol tables.
- **CodeRankEmbed on Windows DirectML**: RoPE uses unsupported tensor ops, produces zero vectors silently. Use Jina-code-v2 via ORT+DirectML instead.
- **DirectML VRAM overflow**: Passing all texts in one ORT call forces padding to longest sequence; 250 texts with 8192-token max can exceed 6GB VRAM. Always pass explicit sub-batch size (32) to fastembed.
- **UTF-8 truncation**: `truncate_on_word_boundary` must use char boundaries, not byte indices. CJK/Cyrillic will panic otherwise.
- **OneDrive sync storm**: Embedding cache on Windows must use `LOCALAPPDATA`, not `$HOME/.cache/` (which is OneDrive-synced).
- **WMI AdapterRAM 32-bit overflow**: `uint32` wraps at 4GB. Try `nvidia-smi` first; treat WMI < 2GB as wrapped.
- **`spawn_blocking` survives `JoinHandle::abort()`**: Must use `AtomicBool` cancellation flag checked between batches.
- **Cross-platform review gap**: v6.0.0 shipped with broken Windows IPC (16 compile errors) because no reviewer ran `cargo check --target x86_64-pc-windows-msvc`. Always include cross-compilation in review checklists.
- **normalize_path lowercasing caused duplicate workspaces**: SHA256 of lowercased vs case-preserved path produces different workspace IDs. After fixing path casing, daemon.db needs migration or old IDs create orphan index directories.
- **C# property extraction uniquely vulnerable**: `extract_property` was the only extractor where type and name are sibling children of the same AST node with no structural anchor. All other member extractors in `csharp/members.rs` are safe -- `extract_method` uses `.rev()`, constructors/destructors have no return type, fields/events search inside `variable_declarator`, delegates handle 1-vs-2 identifier case explicitly.

## Decisions

- **Resolver uses soft penalty, not hard filter**: Import-constrained call-edge filtering gives +200 boost (not hard reject). Hard filter broke same-package Java, implicit Python imports, C headers.
- **CodeRankEmbed (768d) default on macOS/Linux sidecar**: +10% namespace overlap, +20% cross-language vs BGE-small. Windows uses Jina-code-v2 via ORT+DirectML.
- **Language detection single source of truth**: `crates/julie-extractors/src/language.rs`. Adding a new language requires editing only this one file.
- **`lib/` and `packages/` are NOT vendor directories**: They are primary source dirs in Elixir/Ruby/Dart and JS monorepos.
- **Daemon-owned EmbeddingService with eager init**: Not pool-owned (muddies WorkspacePool responsibility), not lazy (re-introduces complexity), no explicit queue (provider mutex serializes).
- **No symbol-level editing tools**: Agents ignore custom editing tools in favor of built-in Edit; Serena overcomes this via aggressive behavioral priming, not organic utility. Bottleneck is search/understanding, not editing. If revisited, `insert_after_symbol` has the strongest case.

## Open Questions

- **NL query vocabulary gap**: Code embedding models match tokens, not semantic synonyms ("save" != "record"). Needs NL query expansion or dual-model approach.
