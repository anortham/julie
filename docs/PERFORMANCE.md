# Performance Targets

**Last Updated:** 2025-11-07

Julie must meet these performance targets:

## Benchmarks

- **Search Latency**: <5ms (Tantivy full-text search)
- **Parsing Speed**: 5-10x optimized performance
- **Memory Usage**: <100MB typical (target: ~500MB)
- **Startup Time**: <2s (SQLite + Tantivy indexing)
- **Indexing Speed**: Process 1000 files in <2s (SQLite + Tantivy)

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
