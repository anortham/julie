# Task 13: Delete the old storage

## WHAT
Deleted `symbols.db` and the julie-core `database` module. One storage path remains: `indexes/<id>/{facts.sqlite,tantivy/}`.

## WHY
Phase 3 gate: one store per checkout. The dual-write path (`db/` + `store/`) is gone.

## HOW
- TDD: `durable_roots` now requires `facts.sqlite` and `tantivy/` directly under `indexes/<id>/`. `db/` and `store/` are rejected.
- Failed open deletes `indexes/<id>/` and reopens. Never migrates.
- `JulieWorkspace` holds `store: Arc<CheckoutStore>` only. Watcher writes via `store.apply`.
- ToolContext DB accessors and `get_search_index_for_workspace` are gone. Snapshot only.
- `tool_calls` stays on `registry.db`. The `symbols.db` copy is gone.

## IMPACT
`cargo build` is green. `durable_roots` passes. Required crate tests: 1071 passed. xtask: 178 passed. `rg` for `SymbolDatabase|canonical_revision|projection_state|indexing_repair|index_engine_state` is empty.
