## Gotchas

- **Hatchling package discovery**: `pyproject.toml` `name` must match the actual package directory, or add explicit `[tool.hatch.build.targets.wheel] packages = ["sidecar"]`. Caused silent sidecar bootstrap failure.
- **tqdm/safetensors stdout corruption**: `SentenceTransformer()` loading writes tqdm progress to fd 1 via native C code. Python-level `sys.stdout = sys.stderr` is insufficient; must use `os.dup2(2, 1)` before model init.
- **Sidecar cold start timeout**: PyTorch import + model loading takes 5-15s; first-time model download can take 30s+. Init timeout must be separate from request timeout (120s vs 5s).
- **DirectML + CUDA mutual exclusivity**: `torch-directml` pins exact `torch==2.4.1`, installing alongside CUDA torch forces CPU-only downgrade. Must install CUDA torch first, probe availability, only then consider DirectML.
- **HuggingFace tokenizer batch rejection**: Some symbol text causes `TypeError` in batch encode. Sidecar needs `_sanitize_texts()` (null bytes, empty strings) and `_encode_individually()` fallback.
- **Tantivy OR-mode bug**: `Occur::Should` clauses become optional when `Must` clauses exist in the same `BooleanQuery`. Fix: wrap OR clauses in a nested `BooleanQuery`.
- **sqlite-vec vec0 virtual tables** don't support `INSERT OR REPLACE`; must DELETE then INSERT.
- **Atomic saves vs file watcher**: Editors do write-temp, delete-original, rename-temp. DELETE handler must check `path.exists()` before purging symbols. Also, `should_index_file()` calls `path.is_file()` which is false for deleted files; need separate `should_process_deletion()`.
- **Cross-file inheritance was silently dropped** in C#, Java, TS, JS, Kotlin, Swift: extractors only resolved base types against same-file symbols. Fixed with `PendingRelationship` fallback for all 6 OOP languages.

## Decisions

- **Candle backend removed**: Redundant with sidecar (primary) and ORT (fallback). Removed 2,164 lines and 6 heavy deps.
- **Sidecar is default embedding path** with auto fallback to ORT. Managed local venv via `uv`, not user-managed.
- **Deferred embedding init**: Sidecar bootstraps lazily after indexing completes, not during `initialize_all_components()`. Keyword search available ~48s sooner.
- **Centrality propagation**: 70% of interface/base class centrality flows to implementations via SQL UPDATE in `compute_reference_scores`. Enables DI-heavy codebases (C#) to surface implementations in `get_context`.
- **Non-code embeddings purged**: Markdown, JSON, TOML, YAML, CSS, HTML, regex, SQL excluded from vector space. Container symbols enriched with child method + property names.
- **get_context default format**: Changed from readable to compact, since primary consumers are AI agents.

## Open Questions

- **NL query recall gap**: BM25 term mismatch means NL queries like "Lucene search implementation" miss `LuceneIndexService` (query stems: `lucen,search,implement` vs symbol stems: `lucen,index,servic`). Needs synonym expansion or semantic fallback improvement.
- **C# property name extraction bug**: `extract_property` in `members.rs` uses `.find(|c| c.kind() == "identifier")` which grabs the type identifier instead of the property name when the type is a plain identifier. Known, not yet fixed.
