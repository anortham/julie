# Autonomous Execution Report - CLI Tool Interface (Plan A)

**Status:** Complete
**Plan:** docs/plans/2026-04-23-cli-annotations-early-warnings.md (Plan A)
**Branch:** feat/cli-tool-interface
**PR:** (pending creation)
**Duration:** ~45m
**Phases:** 1/1 complete
**Tasks:** 5/5 complete

## What shipped
- A1: Shell-first clap command surface with 7 subcommands (search, refs, symbols, context, blast-radius, workspace, tool) and global flags (--json, --format, --standalone)
- A2: CLI execution core with daemon mode (IPC reusing adapter handshake), standalone mode (production handler constructor), and automatic daemon-to-standalone fallback on transport failures
- A3: Tool execution wiring for all 12 MCP tools via generic path, plus 6 named wrapper conversions. Fixed 2 daemon-mode JSON field name bugs (budget->max_tokens, files->file_paths)
- A4: Output formatters (text as-is, JSON pretty-printed, markdown with headers/fenced blocks). Exit code 0/1 semantics. Diagnostics to stderr.
- A5: 13 end-to-end integration tests via std::process::Command. Covers named wrappers, generic tool path, JSON/markdown output, exit codes, stderr isolation. Xtask cli bucket configured.

## Judgment calls (non-blocking decisions made)
- `src/cli_tools/commands.rs:267` - Used `Box::leak` for `GenericToolArgs::tool_name()` to satisfy `&'static str` trait requirement. Acceptable because CLI binary exits after one invocation.
- `src/cli_tools/daemon.rs:21` - Set 10s handshake timeout (vs adapter's 30s) because CLI users expect snappy responses.
- `src/cli_tools/commands.rs:251-252` - Hardcoded `max_depth: 2` and `limit: 12` for `BlastRadiusTool` matching `default_max_depth()` and `default_limit()` in `impact/mod.rs`.
- `src/cli_tools/mod.rs:233-248` - Manually set `is_indexed` flag after `initialize_workspace_with_force` because the MCP `on_initialized` callback (which normally fires `run_auto_indexing`) doesn't trigger in CLI mode.
- `src/tests/cli/mod.rs` - Used `#[ignore]` with xtask `cargo build` pre-step rather than runtime binary detection to make the dependency explicit.

## External review (codex, adversarial)

- **Findings:** 4
- **Verified real, fixed:** 4 (commit: 95b0e739)
  - [HIGH] Daemon tool errors triggered standalone fallback. JSON-RPC error responses were treated as transport failures. Added `DaemonCallError` enum to distinguish `Transport` vs `ToolError`. Tool errors now surface directly and exit 1.
  - [HIGH] blast-radius `--symbols`/`--rev` didn't map to tool's real schema. `--rev` now resolves via `git diff --name-only` to file paths. `--symbols` validates names and returns actionable error.
  - [MEDIUM] refs `--file-path`/`--file-pattern` were silent no-ops. Removed the flags since `FastRefsTool` doesn't support them.
  - [MEDIUM] Workspace daemon-only ops exited 0. Changed upstream handlers to return `CallToolResult::error` for daemon-only operations.
- **Dismissed:** 0
- **Flagged for your review:** 0

## Tests
- 125 CLI tests passing (112 unit + 13 integration)
- Dev test tier: 10/10 buckets passing (311s)
- No regressions in any existing test suite

## Blockers hit
- None

## Files changed
```
 src/cli.rs                                         |  27 +
 src/cli_tools/commands.rs                          | 452 ++++++++++++++++
 src/cli_tools/daemon.rs                            | 217 ++++++++
 src/cli_tools/generic.rs                           | 374 +++++++++++++
 src/cli_tools/mod.rs                               | 369 +++++++++++++
 src/cli_tools/output.rs                            | 326 ++++++++++++
 src/cli_tools/subcommands.rs                       | 264 +++++++++
 src/lib.rs                                         |   1 +
 src/main.rs                                        |  53 +-
 src/tests/cli/mod.rs                               | 509 ++++++++++++++++++
 src/tests/cli_execution_tests.rs                   | 590 +++++++++++++++++++++
 src/tests/cli_tools_tests.rs                       | 346 ++++++++++++
 src/tests/mod.rs                                   |   3 +
 src/tools/search/execution.rs                      |   8 +-
 src/tools/workspace/commands/registry/open.rs      |   2 +-
 src/tools/workspace/commands/registry/refresh_stats.rs |   4 +-
 src/tools/workspace/commands/registry/register_remove.rs |   4 +-
 xtask/test_tiers.toml                              |  15 +-
 xtask/tests/manifest_contract_tests.rs             |   8 +-
 19 files changed, 3557 insertions(+), 15 deletions(-)
```

## Next steps
- Review PR
- Plans B and C (annotation normalization, early warning signals) are ready for future sessions
- The CLI dev loop is now unblocked: `cargo build && ./target/debug/julie-server search "query" --standalone` works during active MCP sessions
