# Testing Guide

**Last Updated:** 2026-07-21

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
# Default ≤60s-declared local confidence gate (warm bucket wall)
cargo xtask test fast

# Smallest core slice (nano ⊆ fast)
cargo xtask test nano

# Diff-scoped buckets; OverBudget (non-zero) when mapped sum exceeds fast budget
cargo xtask test changed
# Explicit scale-up when OverBudget: unique(mapped ∪ dev)
cargo xtask test changed --scale

# Default batch gate after a completed change set
cargo xtask test dev

# When touching startup/workspace/system flows
cargo xtask test system

# When changing search/scoring/tokenization
cargo xtask test dogfood

# Broad pre-merge pass
cargo xtask test full

# List all buckets
cargo xtask test list

# Run one focused bucket as the lead
cargo xtask test bucket <name>

# Report-only inventory audit. Does not run tests.
cargo xtask test inventory --bucket <name>
cargo xtask test inventory --tier dev

# Product-linked search matrix / eval harnesses (Cargo alias → xtask-eval package)
cargo xtask-eval search-matrix mine --days 7 --out artifacts/search-matrix/seeds-YYYY-MM-DD.json
cargo xtask-eval search-matrix baseline --profile smoke
cargo xtask-eval search-matrix baseline --profile breadth --out artifacts/search-matrix/breadth-YYYY-MM-DD.json

# Narrow filter for a specific test
cargo nextest run --lib test_stemming
# Note: per-extractor tests now live in the external anortham/julie-extractors repo
```

### Warm vs cold accounting

Runner summaries print:

- `SUMMARY: … (warm)` — selected bucket commands after every selected Rust test target has been prebuilt; non-test commands remain bucket work
- `PREBUILD:` — summed compile/link time for the deterministic, de-duplicated `--no-run` commands derived from selected `cargo nextest run` and `cargo test` package/target selectors
- `COLD WALL:` — `PREBUILD + warm`; it may exceed the 60s **declared** fast budget on a cold machine

The `cargo xtask …` frontend stays lean. Runner prebuild compiles only the Rust test targets implied by the selected bucket commands; buckets with no Rust test command report zero prebuild time. Use `cargo xtask-eval …` for product-linked harnesses.

## Test Tiers

| Tier | Command | When to use |
|------|---------|-------------|
| nano | `cargo xtask test nano` | Ultra-tight loop (`nano ⊆ fast`) |
| fast | `cargo xtask test fast` | Default local gate (declared sum ≤60s; warm wall) |
| smoke | `cargo xtask test smoke` | Quick sanity check |
| changed | `cargo xtask test changed` | Diff-scoped; **OverBudget** if mapped sum > fast budget (no bare `dev` fallback) |
| changed --scale | `cargo xtask test changed --scale` | OverBudget escalate: `unique(mapped ∪ dev)` |
| dev | `cargo xtask test dev` | After normal changes (batch gate) |
| system | `cargo xtask test system` | Startup/workspace/system changes |
| dogfood | `cargo xtask test dogfood` | Search/scoring/tokenization changes |
| full | `cargo xtask test full` | Pre-merge broad pass |

## Focused Buckets And Inventory

Leads may use `cargo xtask test bucket <name>` when a plan names a focused bucket and a full tier would waste time. This is still lead-owned verification. Workers run exact tests only and should not run bucket commands unless the plan explicitly assigns that diagnostic task.

Use `cargo xtask test inventory --bucket <name>` or `cargo xtask test inventory --tier dev` to audit selected tests with `cargo nextest list`. Inventory is diagnostic evidence, not a passing test gate. It can prove overlap, duplicate selection, or non-inventoryable commands, but it does not replace an exact test, `changed`, or `dev` run.

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
- scope label
- commit SHA
- result
- timestamp (UTC)
- evidence reused (`yes` or `no`)

Evidence may be reused only at the same HEAD commit SHA and the same scope label, and only from a row that already passed. If those conditions are not true, run the command again and record a new row. This is the default rule for expensive gates such as `cargo xtask test dogfood`.

## Search Matrix Harness

`cargo xtask-eval search-matrix` is an investigation harness (via the `xtask-eval` Cargo alias), not a replacement for `cargo xtask test dogfood`. Do not use deprecated `cargo xtask search-matrix|eval` — those point at the lean runner and error with a migration hint.

- `mine` reads the local daemon DB and writes a seed report under `artifacts/search-matrix/`.
- `baseline` runs the committed case and corpus manifests against pre-indexed daemon workspaces and writes JSON plus Markdown reports under `artifacts/search-matrix/`.
- Version 1 expects the target repos to already be indexed and registered in daemon mode. Missing or non-ready repos are reported as skipped, not auto-indexed on the fly.
- Use the matrix harness to mine failure shapes, compare repo families, and promote stable cases into dogfood coverage.

## Dogfooding Tests

The `search_quality` bucket loads a real 100MB SQLite fixture, backfills a Tantivy index, and runs real searches. It is a regression guard, not a fast unit-tier pass.

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

## Multi-Process Acceptance and Test Fixture Ownership

Julie verifies cross-process workspace lifecycle, single-writer fencing, follower edit coordination, and dynamic promotion using real OS subprocesses.

### Running Process Lifecycle Tests

Multi-process tests spawn actual `julie-server` subprocesses communicating over stdio JSON-RPC. To run them, point `JULIE_TEST_BIN` to the compiled binary:

```bash
# Build the test binary first
cargo build -p julie --bin julie-server

# Run the full process lifecycle test suite
JULIE_TEST_BIN=$(pwd)/target/debug/julie-server cargo nextest run -p julie --lib tests::workspace_process_lifecycle::

# Run continuation process tests
JULIE_TEST_BIN=$(pwd)/target/debug/julie-server cargo nextest run -p julie --lib tests::runtime_continuation::

# Run writer fencing and recovery tests
JULIE_TEST_BIN=$(pwd)/target/debug/julie-server cargo nextest run -p julie --lib tests::writer_fencing_contract::

# Run source-edit recovery contract tests
cargo nextest run -p julie --lib tests::edit_recovery_contract::
```

### Test Fixtures and Ownership

1. **`WorktreeProcessFixture` (`src/tests/workspace_process_lifecycle.rs`)**:
   - Manages a temporary git repository with two detached git worktrees (`git worktree add --detach`).
   - Spawns three concurrent `julie-server` subprocesses (Left Owner, Left Follower, Right Owner) sharing an isolated `JULIE_HOME`.
   - Verifies worktree isolation (distinct workspace IDs, databases, and Tantivy indexes), follower source edits, dynamic follower promotion upon owner exit, competing edit conflicts, and graceful shutdown.
   - Automatically kills all child processes and cleans up detached worktrees and temporary directories on `Drop`.

2. **`ContinuationProcessFixture` (`src/tests/runtime_continuation.rs`)**:
   - Manages multi-process pagination continuation token validation across multiple worktrees.
   - Verifies that continuation tokens are durable across process restarts, bounded by memory/TTL limits (15m TTL, 64 MiB total limit), and strictly rejected when passed to a different worktree (`CONTINUATION_INVALID`) or after source changes (`CONTINUATION_STALE`).

3. **`RecoveryProcessFixture` (`src/tests/writer_fencing_contract.rs`)**:
   - Injects faults by stopping child owner processes after canonical SQLite commit but before Tantivy publication.
   - Tests that newly promoted owners automatically detect the projection gap and reconcile Tantivy without requiring source file changes.
   - Enforces writer fencing: only the authentic holder of `WriterPermit` can publish revisions.

4. **`EditRecoveryFixture` (`src/tests/edit_recovery_contract.rs`)**:
   - Simulates interrupted multi-file source edits by writing durable old/new payloads to `<workspace_root>/.julie/edit-journals/`.
   - Tests `recover-edit` (`resume` and `rollback`) across processes under source hash guards, confirming idempotency and conflict preservation (`EDIT_RECOVERY_CONFLICT`).

### Detached Worktree Management

When testing multi-worktree scenarios:
- Tests use `git worktree add --detach` under a dedicated temporary directory.
- Detached HEAD worktrees prevent git branch locking collisions across test runners.
- Each worktree directory is resolved to a canonical path and assigned a unique workspace ID hash, ensuring complete index, SQLite, and lock isolation even when worktrees share the underlying git object database.
- Cleanup hooks remove worktrees and temporary trees only after all client processes have completely terminated and closed their file descriptors.

### Platform Results & Locking Semantics

- **Linux**:
  - Fully verified with real OS processes, Unix domain locks (`fcntl` / `flock`), and POSIX signals.
  - Multi-process failover, concurrent worktree isolation, slot admission scheduling, and graceful/abrupt termination scenarios pass cleanly.
- **Windows Advisory Locking Semantics**:
  - Windows file locking operates via `LockFileEx`. Unlike Unix advisory locks, Windows file locks can impose mandatory locking semantics on standard I/O reads/writes if handles are opened with conflicting sharing modes.
  - Julie configures file sharing flags (`FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`) and explicitly separates lock control files (`leader.lock`, `publication.lock`, `source-edit.lock`, `index-{n}.lock`) from payload data files (`symbols.db`, Tantivy segments, source files).
  - This preserves non-blocking read snapshots and advisory fencing across platforms without causing spurious access violation errors (OS error 5) on Windows.
