# Development Commands

**Last Updated:** 2026-07-22

Daily commands and workflows for Julie development.

Julie pins Rust 1.97.0 in `rust-toolchain.toml`. Use rustup's `cargo` and
`rustc` proxies so local builds, formatting, and release CI select the same
official toolchain. Verify the active selection with:

```bash
rustup show active-toolchain
```

## Daily Development

```bash
# Fast iteration (debug build)
cargo build

# Narrow test during iteration (default)
cargo nextest run --lib <exact_test_name>

# Diff-scoped coverage after a localized change
cargo xtask test changed

# Run specific tests during development (narrow filter, not full suite)
# Note: per-extractor tests now live in the external anortham/julie-extractors repo
cargo nextest run --lib test_stemming -- --nocapture

# Check for issues
cargo clippy
cargo fmt
```

See **CLAUDE.md** for the full test tier strategy (smoke/dev/system/dogfood/full).

## Release Preparation

```bash
# Optimized build
cargo build --release

# Cross-platform builds
cargo build --target x86_64-pc-windows-msvc --release
cargo build --target x86_64-unknown-linux-gnu --release

# Size optimization
cargo bloat --release
```

## Fast linker setup (macOS)

lld links significantly faster than the default macOS linker on large Rust projects.

**One-time setup:**

```bash
brew install lld
```

The `rustflags` block in `.cargo/config.toml` activates `ld64.lld`
automatically for macOS targets. Cargo also supplies
`MACOSX_DEPLOYMENT_TARGET=11.0` to native build scripts so bundled C objects
retain Julie's supported deployment target.

If both Homebrew Rust and Homebrew rustup are installed, put the rustup proxies
before `/opt/homebrew/bin` so the repository pin is honored:

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
```

Do not use Homebrew's host-targeted `rustc` for Julie release builds. Confirm
`command -v cargo`, `command -v rustc`, and `rustup show active-toolchain`
before diagnosing linker object-version warnings.

**Fallback:** if the linker causes issues, remove the `[target.'cfg(target_os = "macos")']` block from `.cargo/config.toml` to restore the default linker.

## Build cache (sccache)

sccache gives cross-branch build caching. Because incremental compilation conflicts with sccache, we disable incremental and gain cache hits across branch switches instead.

**One-time setup:**

```bash
cargo install sccache --locked
```

**Per-shell environment:**

```bash
export RUSTC_WRAPPER=sccache
export SCCACHE_DIR=$HOME/.cache/sccache
export CARGO_INCREMENTAL=0
```

Add these to your shell init (`~/.zshrc` or `~/.bashrc`).

**Verify cache hits:**

```bash
sccache --show-stats
```

**Reclaim stale artifacts:**

If `target/` has grown large (tens of GB) from cruft across branches, reclaim it with `cargo clean`. sccache will repopulate on next build from its external cache.

## Debugging

```bash
# Run with debug logging
RUST_LOG=debug cargo run

# Run specific test with logging
RUST_LOG=debug cargo test test_name -- --nocapture

# Profile memory usage
valgrind --tool=massif cargo run --release

# Profile CPU usage
perf record cargo run --release
perf report
```

## Shared Versus Standalone Storage

Julie supports two distinct storage topologies for development, testing, and production workflows:

### Shared Storage (Default for in-process MCP and standard CLI)
- **Index location**: `$JULIE_HOME/indexes/<workspace_id>/` (default: `~/.julie/indexes/<workspace_id>/`).
- **Registry**: Workspaces, codehealth snapshots, and metrics are recorded in `$JULIE_HOME/registry.db`.
- **Coordination**: Multiple in-process MCP sessions and CLI commands coordinate across the same shared indices via OS locks (`leader.lock`, `publication.lock`).
- **Host Scheduling**: Concurrent indexing runs under host admission slots at `$JULIE_HOME/scheduler/index-{0..7}.lock`.

### Standalone Storage (`--standalone` CLI flag)
- **Index location**: `<project>/.julie/indexes/<workspace_id>/` located directly within the project source root.
- **Registry**: Completely independent; does not register with or touch `$JULIE_HOME/registry.db`.
- **Isolation**: Ideal for local isolated tool debugging, scripting, or CI environments where `$JULIE_HOME` is shared but isolated indexing is required.
- **Source Coordination Parity**: Even in standalone mode, source modifications contend on the same canonical source locks (`<source_root>/.julie/locks/source-edit.lock`) and use the same journal format (`<source_root>/.julie/edit-journals/<edit_id>.json`). A standalone process and a shared MCP session cannot apply conflicting edits to the same source files.

```bash
# Run search using project-local standalone index
target/debug/julie-server fast-search "my_symbol" --standalone --json

# Edit file using standalone mode (still synchronizes on source-edit.lock)
target/debug/julie-server edit-file --params '{"file_path":"src/lib.rs","old_text":"foo","new_text":"bar"}' --standalone --json
```

## Truthful Freshness and Follower Semantics

Follower sessions serve read queries without acquiring index ownership or running background indexers:

- **No Canonical Mutations by Followers**: Followers NEVER write directly to `symbols.db` or Tantivy. They serve queries against SQLite WAL and Tantivy mmap read snapshots under shared `publication.lock`.
- **Honest Freshness Reporting**: When a follower applies a source edit or queries a workspace with pending edits:
  - Output explicitly reports `index_refresh_pending: true` in response envelopes or tool metadata.
  - If a reader encounters an uncommitted projection gap (e.g. after an owner crash or mid-publication), it returns `PROJECTION_LAG` instead of returning stale results silently or fabricating false exact matches.
- **Source Edits Without Index Ownership**: Followers can preview (`dry_run: true`, 0 writes, 0 locks) and apply source edits. When applying, the follower acquires `<source_root>/.julie/locks/source-edit.lock`, validates file hashes, performs atomic journaled writes, runs post-edit AST syntax validation, and signals pending index refresh.
- **Idempotent Recovery**: If an edit is interrupted, `recover-edit` can be invoked from any session (owner or follower) to `resume` or `rollback` the change deterministically:
  ```bash
  target/debug/julie-server recover-edit <edit_id> --action resume --workspace <path> --json
  target/debug/julie-server recover-edit <edit_id> --action rollback --workspace <path> --json
  ```
