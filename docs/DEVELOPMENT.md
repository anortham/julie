# Development Commands

**Last Updated:** 2025-11-07

Daily commands and workflows for Julie development.

## Daily Development

```bash
# Fast iteration (debug build)
cargo build && cargo run

# Run specific tests during development
cargo test typescript_extractor --no-capture

# Watch for changes
cargo watch -x "build" -x "test"

# Check for issues
cargo clippy
cargo fmt
```

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
