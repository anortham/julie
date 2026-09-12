# Testing Guide

**Last Updated:** 2026-09-10

Complete guide to Julie's testing methodology and standards.

## Test Coverage Requirements

- **Extractors**: 100% comprehensive test coverage
- **Editing Tools**: 90% coverage with SOURCE/CONTROL methodology
- **Core Logic**: >80% coverage on search and navigation
- **MCP Tools**: Full integration testing
- **Cross-platform**: Automated testing on Windows, macOS, Linux

## SOURCE/CONTROL Testing Methodology

**Critical Pattern for All File Modification Tools:**

1. **SOURCE files** - Original files that are NEVER modified
2. **CONTROL files** - Expected results after specific operations
3. **Test Process**: SOURCE -> copy -> edit -> diff against CONTROL
4. **Verification**: Must match exactly using diff-match-patch

**Example Structure:**
```rust
struct EditingTestCase {
    name: &'static str,
    source_file: &'static str,    // Never modified
    control_file: &'static str,   // Expected result
    operation: &'static str,
    // ... operation parameters
}
```

**Implemented For:**
- FuzzyReplaceTool
- RenameSymbolTool
- EditSymbolTool

## Running Tests

```bash
# One exact test during the edit loop (about 3.5 s incremental rebuild)
cargo check
cargo nextest run --lib <exact_test_name>

# Optional broad diagnostic when the final full gate is not imminent
cargo xtask test dev

# Search-quality gate when the final full gate is not imminent
cargo xtask test dogfood

# One branch gate after code freeze, before merge
cargo xtask test full

# Product-linked search matrix / eval harnesses (Cargo alias → xtask-eval package)
cargo xtask-eval search-matrix mine --days 7 --out artifacts/search-matrix/seeds-YYYY-MM-DD.json
cargo xtask-eval search-matrix baseline --profile smoke
cargo xtask-eval search-matrix baseline --profile breadth --out artifacts/search-matrix/breadth-YYYY-MM-DD.json

# Narrow filter to zoom in on a failure a tier reported
cargo nextest run --lib test_stemming
# Note: per-extractor tests now live in the external anortham/julie-extractors repo
```

## Test Tiers

`xtask/src/runner.rs` holds the exact command list for each tier.

| Tier | Command | What it runs | When to use |
|------|---------|--------------|-------------|
| dev | `cargo xtask test dev` | Builds `julie-server`. Runs the whole workspace except the dogfood set. Then runs the ignored `tests::cli::` tests. About 20 s warm. | Optional broad diagnostic after a coherent milestone when `full` is not imminent |
| dogfood | `cargo xtask test dogfood` | Ensures the search-quality fixture. Then runs the dogfood set: `search_quality`, `fixtures::julie_db`, `dogfood`. About 15 s, plus 42 s when the fixture rebuilds. | Search-quality branch gate when `full` is not imminent |
| full | `cargo xtask test full` | Dev, then dogfood. | Once after code freeze, before merge |

The dogfood set runs at most six tests at a time. `.config/nextest.toml` sets test group `dogfood` to `max-threads = 6`. No environment variable is needed.

Workers run exact tests only, at most two runs per change (RED, GREEN). The lead runs exact tests or one affected narrow group during implementation. Never run more than one cargo test command at once.

## Broad gate budget

Broad gates are branch events, not edit-loop checks. Freeze code before running one. Since `full` includes `dev` and `dogfood`, do not run either immediately before `full`.

When a broad gate fails, classify the failure before editing:

- If the current diff caused it, or the plan's acceptance criteria require it, reproduce it with one exact test and fix it in the current task.
- If it is pre-existing, platform-only, fixture-only, or unrelated, record separate follow-up work. It does not enter the active task.
- If ownership is unclear, spend at most 15 minutes or one exact diagnostic run to classify it. If it remains unclear, report the gate as incomplete.

After all known in-scope failures pass exact tests, allow one broad-gate retry. If that retry reveals another failure, stop. A third broad run requires an explicit owner decision. The same rule applies to Windows qualification: use exact Windows tests during implementation and one full Windows gate after code freeze when the plan or release requires it.

Report `implementation complete; <gate> pending` when implementation is done but qualification is not. Do not use `almost done` without listing the remaining gates and whether they can expand scope.

## Standalone CLI Dogfood Contract

Use standalone CLI runs for fast tool-behavior checks:

```bash
julie-server search "query" --target definitions --standalone --json
```

Standalone CLI runs execute tools against a local handler in the selected workspace. This catches defects such as:
- CLI argument parsing or wrapper mapping bugs
- tool parameter serialization bugs
- standalone workspace bootstrap and indexing readiness bugs
- tool behavior regressions in result content and `isError` handling
- output formatting regressions for text/json/markdown output

Standalone CLI does not prove daemon transport, restart behavior, or session routing. Those require daemon or MCP integration coverage, including:
- daemon IPC transport and MCP handshake behavior
- adapter forwarding and daemon fallback behavior
- session lifecycle and reconnect flows
- workspace routing across sessions/workspaces

Execution mode evidence contract:
- CLI execution always records mode via `CliToolOutput.mode`.
- CLI runs print mode to stderr as `julie: mode=<mode>, elapsed=<seconds>s`.
- JSON output stays the raw tool result for backward compatibility, so capture stderr mode lines in verification ledgers when you need proof of standalone versus daemon execution.

## Verification Ledger and Evidence Reuse

Use the copy-ready ledger section in `docs/plans/verification-ledger-template.md` for plan verification evidence.

Each ledger row must record:
- invariant
- command
- scope label (`worker-red-green`, `dev`, `dogfood`, `full`, or `live`)
- commit SHA
- result
- timestamp (UTC)
- evidence reused (`yes` or `no`)

Evidence may be reused when the scope matches, the tested commit is an ancestor of the current commit, and the intervening diff cannot affect the command. Evidence-only changes under `.memories/`, `docs/plans/`, or `docs/findings/` may reuse a pass when the command does not consume those files. Record the tested SHA and current SHA. Never rerun an expensive gate only because a later commit recorded its evidence.

## Search Matrix Harness

`cargo xtask-eval search-matrix` is an investigation harness (via the `xtask-eval` Cargo alias), not a replacement for `cargo xtask test dogfood`. Do not use deprecated `cargo xtask search-matrix|eval` — those point at the lean runner and error with a migration hint.

- `mine` reads the local daemon DB and writes a seed report under `artifacts/search-matrix/`.
- `baseline` runs the committed case and corpus manifests against pre-indexed daemon workspaces and writes JSON plus Markdown reports under `artifacts/search-matrix/`.
- Version 1 expects the target repos to already be indexed and registered in daemon mode. Missing or non-ready repos are reported as skipped, not auto-indexed on the fly.
- Use the matrix harness to mine failure shapes, compare repo families, and promote stable cases into dogfood coverage.

## Dogfooding Tests

The `search_quality` bucket indexes this repository into a git-ignored snapshot (`fixtures/databases/julie-snapshot/`, about 300 MB, built once in about 40 s and rebuilt when the schema or engine version changes), backfills a Tantivy index, and runs real searches. It is a regression guard, not a fast unit-tier pass.

**When to run:**
- After significant search/ranking changes
- After modifying Tantivy tokenization or query logic
- Before major releases

## Code Coverage Tooling

**Configuration**: `tarpaulin.toml`
- General threshold: 80%
- Editing tools threshold: 90% (critical for safety)
- Coverage reports: HTML, LCOV, JSON formats

**Commands:**
```bash
# Run coverage analysis
cargo tarpaulin

# Generate detailed HTML report
cargo tarpaulin --output-dir target/tarpaulin --output-format Html
```

## Process Model in Tests

One machine service process owns every workspace index, and each handler is the
only writer for its checkout. There is no cross-process lock, no read-only
session, and no promotion path, so there is nothing multi-process to test except
the service lifecycle itself.

- **Service process tests** (`tests::service::process`, inside `cargo xtask test dev`):
  the one multi-process suite. The dev tier builds `julie-server` first, then the
  suite runs real subprocesses: the shim starts the service, stale `service.json`
  recovery, idle exit, version mismatch, and `stop`. Design section 12 caps it at 20 s.
- **No embedding process in tests**: `.cargo/config.toml` sets
  `JULIE_EMBEDDING_PROVIDER=none` for every test binary. `cargo xtask test dev`
  runs in about 20 s warm (it was about 195 s when tests spawned an embedding
  host). Set `JULIE_EMBEDDING_PROVIDER=native` explicitly for a test that needs
  the native sidecar.
- **Complexity words** (on demand, not in any tier): run
  `scripts/complexity-words.sh main`, which flags design-section-4 words in
  product code added on the branch.
- **Source-edit recovery** (`cargo nextest run -p julie --lib tests::edit_recovery_contract::`):
  `EditRecoveryFixture` (`src/tests/edit_recovery_contract.rs`) simulates
  interrupted multi-file source edits via `<workspace_root>/.julie/edit-journals/`
  and tests `recover-edit` (`resume` and `rollback`) under source hash guards.

### Detached Worktree Management

When testing multi-worktree scenarios:
- Tests use `git worktree add --detach` under a dedicated temporary directory.
- Detached HEAD worktrees prevent git branch locking collisions across test runners.
- Each worktree directory is resolved to a canonical path and assigned a unique workspace ID hash, so SQLite and Tantivy stay isolated even when worktrees share the underlying git object database.
- Cleanup hooks remove worktrees and temporary trees only after all child processes have terminated.
