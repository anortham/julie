# TODO

## Architecture Questions

- [x] **PreToolUse hook to enforce Julie tool usage in subagents** -- Subagents spawned via the Agent tool default to Grep/Glob/Read even when Julie MCP tools are available. A PreToolUse hook on the `Agent` tool now enforces the reminder workflow, and the hook is shipped through the plugin.
- [x] **Cap dry-run diff output for very large edits (rewrite_symbol)** -- The rewrite_symbol tool now truncates very large dry-run previews with a concise summary.
- [x] **Cap dry-run diff output for very large edits (edit_file)** -- `edit_file` dry-run previews now truncate large unified diffs with a line-count summary to avoid oversized tokens.

## Enhancements

- [ ] **Linux ROCm (AMD GPU) support in sidecar bootstrap** -- PyTorch supports AMD GPUs on Linux via ROCm (`https://download.pytorch.org/whl/rocm6.2`). When ROCm torch is installed, `torch.cuda.is_available()` returns True (ROCm provides HIP-based CUDA compat), so the runtime `_select_device` works fine. But the Rust bootstrap (`sidecar_bootstrap.rs`) has no `detect_amd_rocm()` equivalent and never installs ROCm torch. Linux users with AMD GPUs silently get CPU-only embeddings. Detection: check for `rocminfo` command or `/opt/rocm`. Intel XPU (`intel-extension-for-pytorch`) is a similar gap but much more niche.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs`)
- [x] **NL query vocabulary gap** -- Stale. Query classification, hybrid weighting, NL embedding fallback, and deterministic query expansion have since landed (`src/search/weights.rs`, `src/tools/search/nl_embeddings.rs`, `src/search/expansion.rs`). Re-open with fresh examples if current searches still miss obvious synonyms.
- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Search Quality: Post-File-Mode Metrics (2026-04-27)

Data: 1,876 fast_search calls with enriched telemetry (824 before file mode, 1,052 after).

**Headline:** True zero-hit rate dropped 15.2% to 8.0% (47% improvement), but raw rate looks flat (20.0% to 19.2%) because `file_pattern_filtered` failures spiked and mask the gain.

### Findings

- [ ] **F1: file_pattern scope rescue** -- 58% of remaining raw zero-hits (118/202). Agents pass narrow globs or exact file paths as `file_pattern`, Tantivy finds candidates across the codebase, then the filter eliminates everything. Fix: on `FilePatternFiltered + NoInScopeCandidates`, re-run without file_pattern and return labeled out-of-scope rescue results with structured trace fields (`scope_relaxed`, `original_file_pattern`). Sample is skewed (113/118 on Julie workspace during April 22-23 development). See plan for details.

- [ ] **F2: Agent file_pattern guidance** -- Guidance (not prohibition) on file_pattern usage. Single-file file_pattern is valid for grep-within-file; agents should prefer `get_symbols` when they want symbol structure. Update `JULIE_AGENT_INSTRUCTIONS.md` and rescue hint text. Ships alongside F1.

- [x] **F3: Julie workspace content latency** -- ~~Appeared as 511ms avg, turned out to be outlier skew.~~ P50 is 12ms, P95 is 46ms, 97.8% of searches are under 100ms at 15ms avg. Four extreme outliers (60-98s) from April 24 during re-indexing dragged the mean. Not a regression. No action needed.

- [ ] **F4: line_match_miss (two phases)** -- 26% of remaining true zero-hits (52/202). Phase 1: narrow OR-disjunction detection in `line_match_strategy`. Clean `identifier OR identifier OR identifier` patterns route to `FileLevel` instead of Substring. Does NOT blanket-parse OR/AND/NOT (would break SQL `INSERT OR REPLACE`, `IS NOT NULL`, etc.). Phase 2: separator normalization fallback in `line_matches_literal` (hyphen<->underscore, strip escape backslashes).

### Context

- File mode shipped April 22 (102 calls, 5.9% zero-hit rate, 13ms avg latency)
- `file_pattern_filtered` zeros are 96% concentrated on the Julie workspace (113/118), heavily on April 22-23 during file mode development
- Today's (Apr 27) elevated true-zero rate (22.5%) is from hermes-agent searching for symbols that genuinely don't exist in that codebase (loguru, structlog, UtcTime, etc.) -- correct behavior
- Definitions zero-hit rate rose slightly (6.3% to 9.5%) but absolute numbers are small (14 to 16)

## Daemon Reliability

- [x] **Daemon eval sessions leak or retain too many file descriptors** -- Observed 2026-05-15
  during Eros head-to-head benchmarking. A long-lived Julie daemon reached more than `1000` open file
  descriptors and repeatedly logged Tantivy failures opening `meta.json` with `Too many open files
  (os error 24)` while eval workspaces ran startup repair checks. The run became contaminated until
  the daemon was restarted under a higher `ulimit`.

  Evidence:
  - `lsof -p <daemon_pid> | wc -l` was about `1102` before restart and climbed to about `1500`
    during the clean benchmark run.
  - Logs showed repeated failures like:
    `Failed to index workspace: Tantivy error: Failed to open file for read ... Too many open files
    ... filepath: "meta.json"`.
  - The problematic sessions were repeated CLI daemon-mode calls across the Eros eval corpus:
    `julie-server --workspace <repo> --json workspace index --path <repo> --force`, search, and
    context calls.

  Fixes to investigate:
  1. Verify HTTP MCP sessions release all session, Tantivy, watcher, and DB handles after each CLI
     command.
  2. Add daemon telemetry/status for FD count, active sessions, loaded workspaces, watcher refs, and
     open Tantivy readers/writers.
  3. Add a regression test or stress harness for many sequential daemon-mode CLI calls over multiple
     workspaces.
  4. Consider bounding idle workspace retention or eagerly closing per-session resources after
     non-interactive CLI requests.

- [x] **Cold daemon `workspace index --force` can block on embedding startup/catch-up after index is
  already complete** -- Observed 2026-05-15 with a clean temporary `HOME` while reproducing Eros
  head-to-head benchmark stability. The first daemon-mode command:
  `julie-server --workspace /Users/murphy/Source/eros-eval-corpus/browser39 --json workspace index
  --path /Users/murphy/Source/eros-eval-corpus/browser39 --force` timed out from Eros after `30s`,
  even though canonical indexing itself completed in about `2s`.

  Evidence from the clean Julie log:
  - Initial index completed quickly: `Indexing complete: 2372 symbols, 51 files, 1964 relationships`.
  - A later repeat command then logged `Workspace has symbols but 0 embeddings - scheduling catch-up
    embedding`.
  - About `50s` elapsed before embedding provider initialization published unavailable:
    `Embedding provider unavailable ... sidecar process started but health check failed`.
  - Only after that did the forced index continue and return.

  Fixes to investigate:
  1. Do not make `workspace index --force` wait on cold embedding provider initialization unless the
     caller explicitly requests embedding completion.
  2. Separate canonical index readiness from embedding catch-up readiness in CLI/MCP responses.
  3. Add a timeout-bounded test with `JULIE_EMBEDDING_PROVIDER=none` or a delayed sidecar to ensure
     canonical indexing can return promptly.

- [x] **Daemon drain timeout too short for stale-binary restart** -- `drain_timeout_secs=10` (`src/daemon/mod.rs:689`) is aggressive. Observed 2026-05-08: dev-time `cargo build --release` triggers stale-binary auto-restart, in-flight sessions running embeddings/indexing/heavy search can't drain in 10s, force-shutdown logged as `Session drain timeout exceeded, forcing shutdown — in-flight writes may be lost`. Same-day repro showed 3+ forced shutdowns between 17:49–17:56. Fixes to consider:
  1. Bump drain timeout to 60–120s.
  2. Adapter resilience: when the stdio adapter loses HTTP to the daemon, retry with backoff for ≥30s before dropping the MCP session. Currently the client-side session goes permanently dead and `mcp__julie__*` tools become unavailable until the Claude session restarts.
  3. Optional: skip stale-binary restart if any active session was busy in the last N seconds; treat as "wait until truly idle" rather than time-bounded drain.

  Repro is straightforward: open a Claude Code session using the `julie` MCP server (registered to `target/release/julie-server`), run `cargo build --release` in another terminal while the session is active, and watch `~/.julie/daemon.log.*` for the drain-timeout error and the client losing its MCP tools.

  Additional observation 2026-05-15: `julie-server stop` also failed to stop a saturated benchmark
  daemon within `10s`, requiring `kill <pid>`. The daemon eventually logged workspace pool and watcher
  shutdown after the forced cleanup path, but the CLI surfaced `Daemon did not stop within 10s`.

  Additional observation 2026-05-15: after the Eros benchmark cleanup/restart cycle, the Codex MCP
  session's Julie tools all failed with `Transport closed` on `deep_dive`, `fast_refs`, and
  `get_symbols`. This matches the client-side permanent-dead-session failure mode above; the
  current harness could not recover the Julie MCP transport without restarting the session.

  Additional observation 2026-05-15: after restarting Codex and resuming Eros lifecycle work,
  Julie MCP still failed immediately with `Transport closed` on three concurrent `fast_search`
  calls (`_inspect_test_facts`, SQLite chunking, and inspect confidence lookup). This suggests the
  transport/session recovery issue can survive a harness restart or recur immediately after startup,
  not only after a stale-binary or daemon-drain event.

  Additional observation 2026-05-15T17:32:34Z: in a new Eros session after recording the compact
  confidence-pack workflow artifact, Julie MCP failed immediately with `Transport closed` on
  `get_context(query="Eros MCP assess_change readiness compact test confidence pack likely tests
  verification command confidence summaries tool schemas routes readiness confidence linker tests")`.
  The harness continued by falling back to Eros/source inspection.

  Additional observation 2026-05-15T18:08Z: after merging the Eros compact confidence-pack work
  and starting the next Eros artifact/deadlock task, Julie MCP again failed immediately with
  `Transport closed` on
  `get_context(query="Eros test confidence artifact import CLI coverage import confidence pack hub
  concurrency sqlite deadlock readiness assess_change")`. This happened after Julie commit
  `4d42a365` was present locally, so the session still cannot rely on Julie MCP for codebase
  orientation.

- [ ] **Validate adapter retry fix under real-world daemon restart** -- The adapter retry code (commit b3e0c3cc, MAX_RETRIES=5, exponential backoff) shipped 2026-05-15 but has not been validated with the new release binary. During this same session, Julie's MCP transport died silently when the daemon received SIGTERM -- the old binary was still running. Repro: `cargo build --release && cargo xtask dev-restart`, then immediately call a Julie tool. The adapter should reconnect within ~31s instead of dying permanently. Also validate: malformed-JSON skip (043800b3), lost-line preservation (9811af54).

  Additional validation 2026-05-15T20:23Z: after the user patched/rebuilt Julie, Codex's already-open
  `mcp__julie__` transport still returned `Transport closed` on `manage_workspace(operation="list")`
  and `manage_workspace(operation="health")`. The configured binary exists at
  `/Users/murphy/Source/julie/target/release/julie-server`, reports `julie-server 7.9.3`, and is the
  command in `~/.codex/config.toml`. Direct newline-framed adapter probing returned a valid MCP
  `initialize` response, and direct CLI `julie-server --workspace /Users/murphy/Source/eros-confidence-artifacts-deadlock --json workspace health`
  succeeded in daemon mode. Current evidence points to the current harness MCP session staying closed
  after a prior transport death, not to the rebuilt binary being missing or unable to initialize.

- [ ] **Python extractor should not mark source `test_*` callables as tests by name alone** --
  Observed 2026-05-15T20:23Z while dogfooding Eros confidence packs: source methods such as
  `python/eros/store/sqlite.py::test_result_histories` can be emitted with `metadata.is_test`
  because `detect_python()` in `crates/julie-extractors/src/test_detection.rs` returns true for
  any callable name starting with `test_` without checking `is_test_path(file_path)`. Eros then
  treats those source methods as test cases, and `assess_change` can list them as likely tests for
  source-file edits. Keep annotation/decorator-driven Python test detection path-independent, but
  gate name-only `test_*` detection on a test path to avoid source API false positives.

## Future Ideas

- [x] **Full CLI mode for all Julie tools** -- Implemented. CLI execution now supports daemon/fallback/standalone modes, named wrappers, generic tool dispatch, and output formats (`src/cli_tools/`, `src/main.rs`, `src/tests/cli/`, `src/tests/cli_execution_tests.rs`).

- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 34 extractors.
- [ ] **Function body hashing for duplication detection** -- Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority.
- [x] **Scoped path extraction for Rust** -- Implemented as structured scoped-call resolution: Rust `scoped_identifier` calls preserve namespace paths, indexing carries structured pendings, and the resolver uses namespace-aware candidate selection to avoid false edges like `std::collections::HashMap::new()` resolving to local `new`.
