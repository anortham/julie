## Gotchas

- **Candle Metal**: `no metal implementation for layer-norm` for BGE-small model. Dead end per candle upstream. CoreML (Apple Neural Engine) is the working acceleration path on macOS.
- **CoreML constraints**: Fixed input shape `(1 x 128)`, batch size 1 only. Must use `PaddingStrategy::Fixed(128)` and sequential single-item inference. Tensor inputs must be cast to I64 (not U32).
- **CoreML `.mlpackage` download**: HF Hub treats `.mlpackage` as a directory bundle. Must download constituent files (`Manifest.json`, `model.mlmodel`, `weight.bin`) individually and reconstruct the directory locally.
- **Embedding backend fallback was asymmetric**: Originally only handled Candle-to-ORT fallback. Fixed to be symmetric (ORT-to-Candle also works). Both backends must use `BAAI/bge-small-en-v1.5` to stay in the same vector space.
- **Tantivy OR-mode bug**: `Occur::Should` clauses become optional when `Must` clauses exist in the same `BooleanQuery`. Fix: wrap OR clauses in a nested `BooleanQuery`.
- **sqlite-vec vec0 virtual tables** don't support `INSERT OR REPLACE`; must DELETE then INSERT.

## Decisions

- **Embedding backend auto-resolver**: Unified `auto|ort|candle` preference with platform-aware defaults. Apple Silicon defaults to Candle (CoreML), Windows uses ORT with DirectML-first policy. Explicit provider requests are strict (fail if unavailable); `auto` degrades gracefully.
- **`embeddings-candle` is a default feature**: Added to default features so plain `cargo build --release` includes Candle on macOS. CI for Linux/Windows uses `--no-default-features --features embeddings-ort`.
- **CoreML default model**: `michaeljelly/bge-small-en-coreml-v1.5` for Apple Silicon, with tokenizer/config from `BAAI/bge-small-en-v1.5` (CoreML repos typically lack these).
- **Strict acceleration mode**: `JULIE_EMBEDDING_STRICT_ACCEL=1` disables embeddings when runtime is unresolved, degraded, or not accelerated. Opt-in for CI/validation.
- **Runtime status via provider diagnostics**: Use `EmbeddingProvider::accelerated()` and `degraded_reason()` instead of inferring state from device label strings.

## Open Questions

- **Hybrid search on vague NL queries** remains inconsistent. Technical queries work well; vague phrases still miss.
