# Julie Documentation

This directory contains **current** documentation for Julie. Code is the source of truth.

## Architecture Documentation

- **`SEARCH_FLOW.md`** - Tantivy search architecture: query processing, OR-fallback, graph centrality boost, English stemming
- **`INTELLIGENCE_LAYER.md`** - Intelligence layer: tree-sitter structure, naming variants, graph centrality, stemming
- **`ARCHITECTURE.md`** - Token optimization strategies (TokenEstimator, ProgressiveReducer, get_context budgeting)
- **`WORKSPACE_ARCHITECTURE.md`** - Multi-workspace isolation, routing, storage

## Reference Documentation

- **`TESTING_GUIDE.md`** - SOURCE/CONTROL methodology, test coverage
- **`ADDING_NEW_LANGUAGES.md`** - Guide for adding tree-sitter language extractors
- **`DEPENDENCIES.md`** - Tree-sitter versions, dependency management
- **`SQLITE_USAGE_GUIDELINES.md`** - SQLite patterns and best practices
- **`DEVELOPMENT.md`** - Daily commands and development workflows
- **`FILEWATCHER.md`** - File watcher architecture and incremental re-indexing
- **`RELATIVE_PATHS_CONTRACT.md`** - Relative Unix-style path storage contract
- **`TYPES_TABLE_DESIGN.md`** - Types table design for LSP-quality type intelligence
- **`operations/embedding-sidecar.md`** - Embedding sidecar env vars, configuration, and troubleshooting

## Quality Reports

- **`LANGUAGE_VERIFICATION_RESULTS.md`** - Quality matrix for all 33 languages with known open issues
- **`LANGUAGE_VERIFICATION_CHECKLIST.md`** - Reusable verification methodology for language support

## Primary Documentation

For development guidelines, TDD methodology, and current project status, see:
- **`/CLAUDE.md`** - Project development guidelines
- **`/TODO.md`** - Current observations and ideas
- **Code** - The ultimate source of truth

---

**Last Updated**: 2026-03-25
**Philosophy**: Code is truth. Documentation describes how things work NOW and where we're going NEXT.
