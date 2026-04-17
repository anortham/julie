# Pre-Release Hardening Design

**Date:** 2026-04-16
**Scope:** verify and fix the confirmed pre-release regressions called out in `docs/PRE-RELEASE-FINDINGS.md`

## Goal

Ship v6.10.0 without the upgrade-path Tantivy wipe, the restart-time rebuild loop, or the dead dashboard poller, then clean up the low-risk runtime issues that are blocking release confidence.

## Primary Changes

1. **Projection upgrade-path hardening**
   - Seed or reconstruct missing canonical revision metadata for upgraded workspaces that already contain SQLite and Tantivy data.
   - Stop `SearchProjection::ensure_current_from_database` from wiping a healthy Tantivy index when canonical metadata is absent.
   - Use a cumulative SQLite doc count when deciding whether the Tantivy projection is already current.

2. **Dashboard poller repair**
   - Fix the runtime-state formatter typo in `dashboard/templates/status.html`.
   - Keep the existing dashboard tests green and add coverage if the current suite misses the broken path.

3. **Low-risk runtime cleanup**
   - Remove the `retry_dirty_tantivy` early-return operation leak.
   - Drop the workspace-pool write lock before the async session-count update.

## Planned Files

- `docs/PRE-RELEASE-FINDINGS.md`
- `src/search/projection.rs`
- `src/database/migrations.rs`
- `src/database/revisions.rs`
- `src/tools/workspace/indexing/index.rs` if projection sync entry behavior needs a gate
- `src/tests/integration/projection_repair.rs`
- `dashboard/templates/status.html`
- `src/tests/dashboard/integration.rs` or `src/tests/dashboard/state.rs` if extra coverage is needed
- `src/watcher/runtime.rs`
- `src/daemon/workspace_pool.rs`

## Constraints

- Follow TDD for each bug fix.
- Use the narrowest failing test during red and green.
- Run `cargo xtask test dev` after the code changes land.
- Avoid broad refactors until the blockers are dead.

## Acceptance Criteria

- [ ] Upgraded workspaces with pre-existing SQLite and Tantivy data do not lose Tantivy docs on first edit.
- [ ] Incremental writes do not trigger a full Tantivy rebuild on every daemon restart.
- [ ] Dashboard status auto-refresh runs without the runtime-state `ReferenceError`.
- [ ] `retry_dirty_tantivy` does not leave indexing runtime stuck in `WatcherRepair`.
- [ ] `WorkspacePool::get_or_init` does not hold the write lock across `update_session_count().await`.
- [ ] Targeted regression tests cover the fixed bugs.
- [ ] `cargo xtask test dev` passes after the fixes.
