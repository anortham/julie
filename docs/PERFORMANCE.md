# Performance Targets

**Last Updated:** 2025-11-07

Julie must meet these performance targets:

## Benchmarks

- **Search Latency**: <5ms SQLite FTS5, <50ms Semantic (target: 50ms)
- **Parsing Speed**: 5-10x optimized performance
- **Memory Usage**: <100MB typical (target: ~500MB)
- **Startup Time**: <2s (CASCADE SQLite only), 30-60x faster than old blocking approach
- **Background Indexing**: HNSW Semantic 20-30s (non-blocking)
- **Indexing Speed**: Process 1000 files in <2s (SQLite with FTS5)

## Performance Testing

```bash
# Benchmark suite
cargo bench

# Profile memory usage
valgrind --tool=massif cargo run --release

# Profile CPU usage
perf record cargo run --release
perf report
```
