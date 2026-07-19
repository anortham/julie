# TODO

## Enhancements

- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

- [x] **Extractor structural facts query** -- `patterns` queries the typed, upstream-maintained structural registry without accepting raw grammar-specific tree-sitter expressions.
- [x] **AST-based complexity metrics** -- Persist upstream `complexity_metrics` and show per-symbol counts in `deep_dive`; hotspot ranking remains a separate product feature.
- [ ] **Function body hashing for duplication detection** -- `body_hash` is already stored per symbol but currently used only for change detection. Repurpose for near-duplicate function detection across a codebase (normalize whitespace/identifiers before hashing for fuzzier matching). Low priority.
