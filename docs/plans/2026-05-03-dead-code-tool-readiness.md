# Dead Code Tool Readiness

**Source baseline:** `docs/plans/2026-05-03-dead-code-audit-baseline.md`
**Workspace:** `dead-code-audit-cleanup_4f5b083c`
**Commit:** working tree based on `84b83c06`

## Decision

Do not add an MCP dead-code tool yet.

The audit workflow is useful, but the signal is still too noisy for an agent-facing MCP tool that looks authoritative. Keep this as a CLI/report workflow first. Productize only after the CLI can separate real cleanup candidates from graph gaps and dynamic entry points with much less manual review.

## Reviewed Counts

| Decision Label | Count | Notes |
| --- | ---: | --- |
| `delete` | 4 | `WorkspaceEntry.indexed`, `WorkspacePool::is_indexed`, `WorkspacePool::mark_indexed`, `WorkspacePool::sync_indexed_from_db` |
| `merge-into-caller` | 2 | `attach_workspace_once_and_sync_indexed`, unused dashboard cleanup `AppState` parameter |
| `keep` | 5 | Projection served revision, uncommitted projection apply path, workspace indexing snapshot, indexing file-count helpers |
| `graph-gap` | 3 | Unqualified same-file calls, watcher/product calls, constructor-style `new` methods |
| `needs-design-review` | 4 | Adapter forwarding and lifecycle restart helpers should wait for HTTP transport or a targeted lifecycle cleanup |
| `test-fossil` | 4 | WorkspacePool indexed-flag tests were removed with the fossil behavior |

## Useful Evidence

- The inventory script is good at finding test-only relationships.
- `fast_refs` plus content search is necessary because graph gaps are common.
- `julie-server signals` is useful for graph-quality leads, but its current top output is fixture-heavy for this repo.
- Candidate filtering by architecture stage kept the review sane. The raw inventory is too broad to act on directly.

## Noisy Evidence

- Extractor registry functions and language extractor methods show up as dead-ish because macro and registry usage is hard for the graph.
- Structs and enums often appear as zero-ref symbols even when imports or type usage prove they are product surface.
- Generic method names like `new`, `fmt`, `as_str`, and `empty` are mostly noise without file-path and caller context.
- Fixture entry-point signals are useful for code intelligence work, but not for Julie cleanup.

## Product Shape

Start with a CLI command, not MCP.

The first product version should:

- reuse the inventory script's sections, but make stage/path filtering first-class
- require a freshness check and print projection health at the top
- label every candidate as candidate evidence, not deletion advice
- include graph-gap reasons in the output when identifier hits conflict with relationship refs
- expose `--json` for plan ledgers
- stay out of the MCP handler until repeated cleanup sessions prove the output is stable

## Graph Gaps To Feed Back

- Same-file unqualified calls can make real helpers look unused, as with `projection_served_revision`.
- Public constructor-style methods are badly overreported.
- Trait, registry, macro, and dynamic dispatch roles need explicit keep signals.
- Watcher and session attachment ref-count methods need better relationship edges through service call paths.

## Result Of This Pass

The pass found and removed one real fossil cluster: the workspace pool's in-memory indexed flag. That flag had survived because tests referenced it, but product code no longer read it after the workspace service split. Removing it deleted stale synchronization calls in IPC, dashboard cleanup, handler session attachment, and three fossil tests.

The audit earned its keep as a cleanup workflow. It has not earned an MCP surface yet.
