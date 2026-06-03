# Testing Guide

**Last Updated:** 2026-03-25

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

# Search matrix investigation harness
cargo xtask search-matrix mine --days 7 --out artifacts/search-matrix/seeds-YYYY-MM-DD.json
cargo xtask search-matrix baseline --profile smoke
cargo xtask search-matrix baseline --profile breadth --out artifacts/search-matrix/breadth-YYYY-MM-DD.json

# Narrow filter for a specific test
cargo nextest run --lib test_stemming
# Note: per-extractor tests now live in the external anortham/julie-extractors repo
```

## Test Tiers

| Tier | Command | When to use |
|------|---------|-------------|
| smoke | `cargo xtask test smoke` | Quick sanity check |
| dev | `cargo xtask test dev` | After normal changes (default) |
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

`cargo xtask search-matrix` is an investigation harness, not a replacement for `cargo xtask test dogfood`.

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
