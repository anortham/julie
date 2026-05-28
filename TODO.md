# TODO

## Enhancements

- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement.

## Future Ideas

- [ ] **Tree-sitter pattern query tool** -- Expose tree-sitter's structural query language as a Julie tool for finding code by AST shape, not just text. Use case: "find this bug pattern elsewhere in the codebase" -- e.g., htmx attribute on element without paired init call, or function calls missing required follow-up. Semantic search finds similar *intent*; text search finds *literal matches*; neither finds *structural patterns*. Tree-sitter's S-expression queries are the right primitive. Infrastructure already exists (we run tree-sitter for 34 languages during extraction); the gap is a tool that accepts a query string and returns matching nodes with file/line/snippet.
- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 34 extractors.
- [ ] **Function body hashing for duplication detection** -- `body_hash` is already stored per symbol but currently used only for change detection. Repurpose for near-duplicate function detection across a codebase (normalize whitespace/identifiers before hashing for fuzzier matching). Low priority.
