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
