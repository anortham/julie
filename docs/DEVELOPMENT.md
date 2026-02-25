# Development Commands

**Last Updated:** 2026-02-25

Daily commands and workflows for Julie development.

## Daily Development

```bash
# Fast iteration (debug build)
cargo build && cargo run

# Fast test tier (~15s) — use after EVERY change
cargo test --lib -- --skip search_quality 2>&1 | tail -5

# Run specific tests during development
cargo test --lib test_stemming --nocapture

# Check for issues
cargo clippy
cargo fmt
```

See **CLAUDE.md** for the full test tier strategy (fast/dogfood/full).

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
