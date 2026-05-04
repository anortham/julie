# Dead Code Tool Readiness

**Source baseline:** `docs/plans/2026-05-03-dead-code-audit-baseline.md`
**Workspace:** `http-daemon-transport_a9c9c040`
**Commit:** `49028afa`

## Decision

Do not add an MCP dead-code tool yet.

The audit workflow is useful, but the signal is still too noisy for an agent-facing MCP tool that looks authoritative. Keep this as a CLI/report workflow first. Productize only after the CLI can separate real cleanup candidates from graph gaps and dynamic entry points with much less manual review.

## Reviewed Counts

| Decision Label | Count | Notes |
| --- | ---: | --- |
| `delete` | 8 | `WorkspaceEntry.indexed`, `WorkspacePool::is_indexed`, `WorkspacePool::mark_indexed`, `WorkspacePool::sync_indexed_from_db`, `flag_restart_pending_for_restart`, `WorkspacePool::active_count`, `WorkspacePool::new` migration args, `WatcherPool::increment_ref` |
| `merge-into-caller` | 4 | `attach_workspace_once_and_sync_indexed`, unused dashboard cleanup `AppState` parameter, lifecycle `store_phase`, `WatcherPool::decrement_ref` |
| `make-private` | 2 | Daemon state-file write helpers stayed available to daemon tests but left the public lifecycle API |
| `keep` | 13 | Projection served revision, uncommitted projection apply path, workspace indexing snapshot, indexing file-count helpers, and legacy IPC adapter helpers that were kept only during the HTTP compatibility window |
| `graph-gap` | 6 | Unqualified same-file calls, constructor-style `new` methods, enum/type usage, same-named method conflation, watcher/session lifecycle edges, and extractor-heavy blast-radius output |
| `needs-design-review` | 2 | Projection helper cleanup waits for projection invariant ownership; legacy IPC removal was resolved by `docs/plans/2026-05-04-remove-legacy-ipc.md` |
| `test-fossil` | 8 | Workspace indexed-flag tests, restart-pending state helper tests, WorkspacePool map-count tests, and WatcherPool raw ref-count tests |

## Useful Evidence

- The inventory script is good at finding test-only relationships.
- `fast_refs` plus content search is necessary because graph gaps are common.
- `julie-server signals` is useful for graph-quality leads, but its current top output is fixture-heavy for this repo.
- Candidate filtering by architecture stage kept the review sane. The raw inventory is too broad to act on directly.
- Rewriting fossil tests to assert product behavior was more valuable than simply deleting tests. The WorkspacePool and WatcherPool passes are the clearest examples.

## Noisy Evidence

- Extractor registry functions and language extractor methods show up as dead-ish because macro and registry usage is hard for the graph.
- Structs and enums often appear as zero-ref symbols even when imports or type usage prove they are product surface.
- Generic method names like `new`, `fmt`, `as_str`, and `empty` are mostly noise without file-path and caller context.
- Fixture entry-point signals are useful for code intelligence work, but not for Julie cleanup.
- `fast_refs` can conflate same-named methods when asked for a qualified symbol that no longer exists or is stale in the index.
- `blast_radius` can produce unrelated high-centrality extractor output when the seed resolution is noisy. That output is useful as a graph-quality warning, not deletion evidence.

## Product Shape

Start with a CLI command, not MCP.

The first product version should:

- reuse the inventory script's sections, but make stage/path filtering first-class
- require a freshness check and print projection health at the top
- label every candidate as candidate evidence, not deletion advice
- include graph-gap reasons in the output when identifier hits conflict with relationship refs
- expose `--json` for plan ledgers
- include a "tests preserve this symbol" section that names the tests and suggests product-behavior replacements
- stay out of the MCP handler until repeated cleanup sessions prove the output is stable

## Graph Gaps To Feed Back

- Same-file unqualified calls can make real helpers look unused, as with `projection_served_revision`.
- Public constructor-style methods are badly overreported.
- Trait, registry, macro, and dynamic dispatch roles need explicit keep signals.
- Same-named methods need stronger disambiguation, especially after a symbol has just been deleted.
- File-seeded blast-radius needs better seed resolution diagnostics when returned callers are obviously unrelated to the changed file.
- Watcher and session attachment lifecycle edges need better relationship edges through service call paths.

## Result Of This Pass

This pass found and removed four real fossil clusters:

- the workspace pool's in-memory indexed flag and sync helpers
- a lifecycle restart-pending helper preserved only by old state tests
- WorkspacePool constructor migration args and a map-count accessor preserved only by tests
- WatcherPool raw ref-count hooks preserved only by tests after session attachment became the product lifecycle

The best cleanup wins came from replacing fossil tests with product-behavior tests, not just deleting code. The worst evidence came from graph outputs that looked authoritative until raw `rg` proved they were wrong or stale.

The audit earned its keep as a cleanup workflow. It has not earned an MCP surface yet. Build the CLI/report version first, then let repeated cleanup passes prove whether the labels are stable enough for an MCP tool.
