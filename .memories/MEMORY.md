## Gotchas

- **Tantivy OR-mode bug**: `Occur::Should` clauses become optional when `Must` clauses exist in the same `BooleanQuery`. Fix: wrap OR clauses in a nested `BooleanQuery`.
- **sqlite-vec vec0 virtual tables** don't support `INSERT OR REPLACE`; must DELETE then INSERT.
- **Atomic saves vs file watcher**: Editors do write-temp, delete-original, rename-temp. DELETE handler must check `path.exists()` before purging. `should_index_file()` calls `path.is_file()` which is false for deleted files; need separate `should_process_deletion()`.
- **CodeRankEmbed on Windows DirectML**: RoPE uses unsupported tensor ops, produces zero vectors silently. Use Jina-code-v2 via ORT+DirectML instead.
- **DirectML VRAM overflow**: Passing all texts in one ORT call forces padding to longest sequence. Always pass explicit sub-batch size (32) to fastembed.
- **OneDrive sync storm**: Embedding cache on Windows must use `LOCALAPPDATA`, not `$HOME/.cache/` (OneDrive-synced).
- **WMI AdapterRAM 32-bit overflow**: `uint32` wraps at 4GB. Try `nvidia-smi` first; treat WMI < 2GB as wrapped.
- **`spawn_blocking` survives `JoinHandle::abort()`**: Must use `AtomicBool` cancellation flag checked between batches.
- **Windows daemon `CREATE_NO_WINDOW` kills signal handling**: Use Win32 named events, not `tokio::signal::ctrl_c()` or `taskkill`.
- **Adapter `forward_bytes` race condition (UNFIXED)**: `tokio::select!` cancels the read-from-daemon task when stdin EOF arrives, breaking bidirectional MCP forwarding. Fix: `tokio::join!` or half-close semantics. Causes "connection closed: initialize request" errors.
- **Plugin launch script stale binary**: Must compare archive mtime vs extracted binary mtime. Old script only checked existence (`-x`), so plugin updates never deployed new binaries.
- **Codehealth analysis sources on disk but not compiled**: `src/analysis/` gated out of Cargo.toml. Don't re-enable.

## Decisions

- **Resolver uses soft penalty, not hard filter**: Import-constrained call-edge filtering gives +200 boost. Hard filter broke same-package Java, implicit Python imports, C headers.
- **Daemon-owned EmbeddingService with eager init**: Not pool-owned (muddies responsibility), not lazy (complexity), no explicit queue (provider mutex serializes).
- **No symbol-level editing tools**: Agents ignore custom editing in favor of built-in Edit. If revisited, `insert_after_symbol` has strongest case.
- **Plugin embeds all 3 platform binaries** (~20MB each) rather than download-on-first-run. Plugin cache is version-snapshots so git bloat is irrelevant.
- **SessionStart hook for behavioral guidance** compensates for MCP 2k tool description limit. Fires on startup/clear/compact.

## Open Questions

- **NL query vocabulary gap**: Code embedding models match tokens, not semantic synonyms ("save" != "record"). Needs NL query expansion or dual-model approach.
- **Incomplete embedding backfill not resumed on daemon restart**: Known issue as of v6.0.2. ORT upgrade to rc12 also pending.
- **Windows plugin stdio piping**: Polyglot wrapper for long-running MCP server is untested.
- **macOS Gatekeeper warnings**: Ad-hoc codesigning may trigger warnings. User has Apple Developer subscription for proper signing later.
