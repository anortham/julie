# Development Commands

**Last Updated:** 2026-03-25

Daily commands and workflows for Julie development.

## Daily Development

```bash
# Fast iteration (debug build)
cargo build

# Default test tier -- run after EVERY change
cargo xtask test dev

# Run specific tests during development (narrow filter, not full suite)
cargo test -p julie-extractors typescript_extractor -- --nocapture
cargo test --lib test_stemming -- --nocapture

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
