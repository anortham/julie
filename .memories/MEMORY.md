## Gotchas

- **Snowball English stemmer** does NOT reduce `-or` suffixes (`processor` stays `processor`), but `-tion`/`-ing` work (`estimation` -> `estim`, `running` -> `run`). Test expectations must use pairs that actually share stems.
- **Tantivy OR-mode bug**: `Occur::Should` clauses become optional when `Must` clauses exist in the same `BooleanQuery`. Fix: wrap OR clauses in a nested `BooleanQuery`.
- **Reference workspace "primary DB" pattern**: Three separate places were caught querying the primary workspace DB instead of the reference workspace DB (tokenizer open, FILE_CONTENT migration check, incremental refresh). When touching reference workspace code paths, always verify which DB handle is being used.
- **`SearchIndex::open` vs `open_with_language_configs`**: Reference workspace search must use `open_with_language_configs` to match how the index was created. Mismatched tokenizer configs produce different Tantivy term dictionaries, causing search to silently return zero results.
- **Parser-less files (text/json/markdown)** have `symbol_count = 0` permanently since `process_file_without_parser` never updates it. Any logic that uses `symbol_count == 0` as "needs reprocessing" creates an infinite re-indexing loop for these files.

## Decisions

- **`CENTRALITY_NOISE_NAMES` is separate from `NOISE_NEIGHBOR_NAMES`** in pipeline.rs. Different purposes (scoring boost skip vs. display filtering), different membership. Intentional.
- **Centrality boost formula**: `score *= 1.0 + ln(1 + ref_score) * 0.3`. Logarithmic scaling compresses 3597:1 raw range to ~3.5:1 boost range. Weight of 0.3 tested adequate, no adjustment needed.
- **Reference score weights**: Calls=3, Implements/Imports/Extends/Instantiates=2, Uses/References=1, Contains=0. Self-references excluded.
- **get_context scoring hierarchy**: source 1.0x > test 0.3x > non-code/docs 0.15x > structural kinds 0.2x (multiplicative). Import nodes filtered entirely.
- **`.memories` NOT blacklisted** from file walking. User clarified it is a project artifact meant to be committed.

## Open Questions

- **get_context pivot quality** is inconsistent on ambiguous natural-language queries. A code-first fallback guardrail was added (2026-02-26) but may need further tuning.
- **C# DI-heavy codebases** show ref_score 0 and 0 neighbors in reference workspaces. Fundamental limitation of static analysis; runtime-only relationships are invisible to tree-sitter.
