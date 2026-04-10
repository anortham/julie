# TODO

## Code Review Findings (2026-04-05)

Validated against source. All findings confirmed. **All fixed 2026-04-05.**

### High Priority

- [x] **edit_symbol stale index guard** -- Added blake3 hash freshness check in `call_tool` before applying indexed line ranges. Compares current file hash against stored hash; refuses with clear error if stale. (`src/tools/editing/edit_symbol.rs`)

- [x] **Session-connect catch-up misses deletions** -- Added `indexed_files.difference(&workspace_files)` check to `check_if_indexing_needed()`. Returns true to trigger cleanup when files were deleted while daemon was down. (`src/startup.rs`)

- [x] **Watcher 1s dedup drops real second saves** -- Changed from `continue` (drop) to `push_back` (re-queue) so the latest state is always eventually processed. (`src/watcher/mod.rs`)

### Medium Priority

- [x] **Embedding cancellation is global, not per-workspace** -- Changed `embedding_task` from single `Option` slot to `HashMap<String, ...>` keyed by workspace_id. Also updated cancel sites in `index.rs` and `refresh_stats.rs`. (`src/handler.rs`, `src/tools/workspace/indexing/embeddings.rs`)

- [x] **web-research skill leaks full pages into context** -- Combined fetch+save into single step that pipes directly to file, never printing to stdout. (`.claude/skills/web-research/SKILL.md`)

- [x] **edit_symbol is line-based at apply time** -- Documented line-granularity limitation in tool description. Long-term: byte offsets from tree-sitter for sub-line precision. (`src/handler.rs`)

### Low/Medium Priority

- [x] **Bracket-balance validation ignores strings and comments** -- Downgraded from `Err` (hard reject) to `Option<String>` (advisory warning appended to output). Updated call sites in both `edit_symbol.rs` and `edit_file.rs`. (`src/tools/editing/validation.rs`)

### Opportunities (from review)

- [ ] **Cap dry-run diff output for very large edits** -- The edit tools are token savers until the preview diff itself becomes the expensive part. Consider truncating diffs beyond a threshold (e.g., 200 lines) with a "diff truncated, N more lines" summary.

- [x] **Add end-to-end edit_symbol tests** -- Added 4 integration tests: replace via index, stale index rejection, insert_after dry-run, symbol not found. (`src/tests/tools/editing/edit_symbol_tests.rs`)

- [x] ~~**Document web-research fallbacks**~~ -- Removed. The filewatcher must index saved files reliably; if it doesn't, that's a bug to fix, not a workflow to document around.

## Architecture Questions

- [ ] **Do reference workspaces need separate indexes?** -- Currently `manage_workspace add /path/to/dep` creates a full separate `indexes/{ref_id}/db/symbols.db` + `tantivy/` for each reference. The centralized daemon already manages per-workspace indexes. Having a separate "reference" concept with its own index path may be unnecessary indirection. Needs analysis: is the `workspace_references` linkage table the only thing that needs to exist?

- [ ] **PreToolUse hook to enforce Julie tool usage in subagents** -- Subagents spawned via the Agent tool default to Grep/Glob/Read even when Julie MCP tools are available. A PreToolUse hook on the `Agent` tool could intercept the prompt and remind the parent to include explicit `mcp__julie__*` tool names. Ship as part of the Julie plugin so all users benefit.

## Enhancements

- [ ] **Plugin update UX: stale daemon survives version upgrade** (surfaced 2026-04-10 while shipping v6.6.11)

  **Symptom:** user installs a new plugin version (6.6.10 → 6.6.11), restarts Claude Code, and the dashboard still reports the old version. The ONLY way to pick up the new binary is to manually find the running `julie-server.exe` daemon PID and kill it, then open a fresh session.

  **Root cause:** the stale-binary auto-restart feature in `run_daemon` (see `src/daemon/mod.rs` around `binary_mtime()`) captures `current_exe()` mtime at startup and compares it on each session connect/disconnect. This works for in-repo `cargo build --release` (same path, mtime changes) but FAILS for plugin installs, where each version lives in its own directory:
  - 6.6.10 daemon running from `.claude/plugins/cache/julie-plugin/julie/6.6.10/bin/x86_64-pc-windows-msvc/julie-server.exe`
  - 6.6.11 installer drops new binary at `.claude/plugins/cache/julie-plugin/julie/6.6.11/bin/x86_64-pc-windows-msvc/julie-server.exe`
  - The old 6.6.10 path is untouched, mtime unchanged, detector concludes nothing has changed
  - New adapter spawned by 6.6.11's `run.cjs` connects to the still-listening 6.6.10 daemon via the workspace-independent IPC socket
  - Version mismatch is invisible; sessions silently run against the old binary

  **Evidence:** daemon.log.2026-04-10 shows "Starting Julie daemon v6.6.10" at 01:55:42 (post-update), 4+ minutes after Claude Code was supposedly using 6.6.11. Confirmed via `tasklist` — PID 588 was the 6.6.10 daemon holding 160MB of embeddings in memory. Manual `taskkill //F //PID 588` + session spawn in a different project finally launched the 6.6.11 binary (verified: daemon.log shows "Starting Julie daemon v6.6.11" at 02:00:31 with 33ms ready time, confirming lazy-init fix).

  **Fix sketch (two layers of defense):**
  1. **Version-aware adapter handshake.** Adapter sends its version in the IPC connect header. Daemon compares against its own `env!("CARGO_PKG_VERSION")`. If the adapter is newer, daemon drains active sessions and exits cleanly via the existing `restart_pending` mechanism. The adapter sees the daemon disappear and spawns a fresh one from its own install path. Same pattern as the existing stale-mtime restart, just triggered by version mismatch instead.
  2. **Plugin launcher cooperation.** `hooks/run.cjs` (in the julie-plugin repo) pings the daemon before spawn. If a daemon is running but version mismatches, run `julie-server stop` first, wait for exit, then spawn the new binary. Belt-and-suspenders for the IPC handshake approach, and covers the case where the old daemon is non-responsive.

  **Bonus UX signal (small cost, high value):** dashboard shows a prominent banner if the running daemon's `env!("CARGO_PKG_VERSION")` doesn't match the plugin install path it was spawned from. Turns "dashboard shows 6.6.10 when you expected 6.6.11" from a subtle number to a visible warning.

  **Acceptance criteria:**
  - [ ] Installing a new plugin version and restarting Claude Code results in the new version running, with no manual process killing
  - [ ] Daemon exits cleanly (drains sessions, no forced kill) when a newer adapter connects
  - [ ] Dashboard surfaces a warning if the running daemon's compile-time version differs from its install path
  - [ ] Works on Windows (named pipes), macOS and Linux (Unix domain sockets)

- [ ] **Linux ROCm (AMD GPU) support in sidecar bootstrap** -- PyTorch supports AMD GPUs on Linux via ROCm (`https://download.pytorch.org/whl/rocm6.2`). When ROCm torch is installed, `torch.cuda.is_available()` returns True (ROCm provides HIP-based CUDA compat), so the runtime `_select_device` works fine. But the Rust bootstrap (`sidecar_bootstrap.rs`) has no `detect_amd_rocm()` equivalent and never installs ROCm torch. Linux users with AMD GPUs silently get CPU-only embeddings. Detection: check for `rocminfo` command or `/opt/rocm`. Intel XPU (`intel-extension-for-pytorch`) is a similar gap but much more niche.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs`)
- [ ] **NL query vocabulary gap** -- Code embedding models match tokens, not semantic synonyms ("save" != "record", "persist" != "insert"). Partially mitigated by embedding enrichment (file paths, implementor names, field signatures) and query classification (dynamic keyword/semantic weighting). Remaining gap: `fast_search` content mode with NL queries still misses code that `get_context` (embedding-powered) finds. Full fix likely needs NL query expansion or dual-model approach.
- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

- [ ] **Full CLI mode for all Julie tools** -- Add CLI subcommands so every MCP tool can be called from the terminal (e.g., `julie search "query" --definitions`, `julie deep-dive SymbolName`, `julie get-symbols src/foo.rs`). Two modes: daemon client (fast, connects via IPC to running daemon) and standalone (initializes workspace, runs, exits). Benefits: (1) live validation of new builds without restarting Claude Code, (2) automated integration test scripts that call real tools and assert on output, (3) search quality regression suites as shell scripts, (4) dogfooding without needing an MCP client session. Architecture: clap frontend dispatching to the same handler functions the MCP server uses. Needs full brainstorm before implementation.

- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 33 extractors.
- [ ] **Function body hashing for duplication detection** -- Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority.
- [ ] **Scoped path extraction for Rust** -- Capture `crate::module::func()` qualified paths as implicit import edges. Would improve call graph quality for Rust codebases specifically.

## Historical Dogfood Notes

### 2026-03-21 LabHandbook Embedding Quality (CodeRankEmbed predecessor comparison)

Tested 9 semantic queries against LabHandbook V2 (434 files, 7306 symbols). Key takeaways that still apply to CodeRankEmbed:

- **Cross-language semantic search is the killer feature.** "How does the frontend communicate with the backend API" returned a complete API surface map across C# and TypeScript in one call.
- **NL text search (`fast_search`) is the weak spot.** Definition mode is excellent; content mode with NL queries misses relevant code. The quality gap between `fast_search` and `get_context` for NL queries is significant.
- **One persistent weakness:** "content management rich text editing" missed the core content subsystem. Embedding vectors for some symbols don't associate strongly with the concepts they implement. **Update (2026-04-03):** Embedding enrichment shipped (file paths, implementor names, field signatures, query classification). Similarity threshold lowered from 0.5 to 0.35. This improved conceptual search significantly but the core vocabulary gap remains for true semantic synonyms.
- **Multi-instance VRAM is a concern** with larger models (768d). 2+ Julie processes = 2+ model loads. Daemon mode's shared `EmbeddingService` mitigates this for same-machine sessions.
