# Autonomous Execution Report - Dashboard Early Warnings

**Status:** Complete
**Plan:** docs/plans/2026-04-23-dashboard-early-warnings.md
**Branch:** codex/dashboard-early-warnings
**PR:** https://github.com/anortham/julie/pull/15
**Duration:** same-session plan execution
**Phases:** 5/5 complete
**Tasks:** 5/5 complete

## What shipped
- Removed the stale security risk analysis path, its tests, and the obsolete metrics trend surface.
- Added annotation class and early warning config to embedded language configs.
- Added `EarlyWarningReport` generation over persisted normalized annotations.
- Added `early_warning_reports` cache storage with migration 021 and fresh-schema creation.
- Added dashboard Signals route, templates, refresh behavior, and project detail navigation.
- Added analysis, cache, dashboard, language config, and metrics regression tests.

## Judgment calls (non-blocking decisions made)
- `src/analysis/early_warnings.rs` - Chose annotation-derived signals over revived risk scoring because the removed security module was speculative.
- `src/analysis/early_warnings.rs` - Chose to treat auth bypass markers as auth decisions while still surfacing them as review markers because `[AllowAnonymous]` and `permitall` are intentional auth metadata, not missing-auth evidence.
- `src/analysis/early_warnings.rs` - Chose full-row cache writes before applying dashboard section limits because otherwise a small view poisons later full reads.
- `src/dashboard/routes/signals.rs` - Chose the active workspace pool DB path when available, with a disk-open fallback for inactive registered workspaces.
- `languages/typescript.toml` and `languages/javascript.toml` - Dropped low-confidence `all`, `options`, and `head` markers to reduce false positives.

## External review (claude, adversarial)
- **Findings:** 9 from the retry output
- **Verified real, fixed:** 6
  - Cache could grow without pruning. Fixed by keeping one current row per workspace and file pattern.
  - Section limits were not part of cache keys and could poison cached rows. Fixed by caching full reports and applying limits on read.
  - Auth candidates could double count symbols with multiple entry markers. Fixed by de-duping per symbol.
  - Auth bypass markers could show as both bypass evidence and missing-auth candidates. Fixed by counting bypass as an auth decision.
  - Dashboard copy and count assertions were too weak. Fixed copy and strengthened card-scoped assertions.
  - Active workspace report generation opened a separate DB path. Fixed by using the pool-owned DB when available and running report generation in `spawn_blocking`.
- **Dismissed:** 3
  - All-language config schema version should invalidate every report. Dismissed because reports key off languages present in the workspace, avoiding invalidation for irrelevant languages.
  - HashSet string lifetime nit. Dismissed as non-material.
  - GET summary writes cache rows. Kept as an intentional report cache side effect, with active-workspace DB routing fixed.
- **Flagged for your review:** 0

## Tests
- `cargo fmt --check`: passed
- `cargo build`: passed
- `cargo xtask test changed`: 10 buckets passed in 315.2s
- `cargo nextest run --lib test_system_health_reports_projection_revision_lag`: 1 passed after an integration-bucket SQLite lock failure
- `cargo xtask test system`: rerun passed 2 buckets in 41.1s
- `./target/debug/julie-server search "@app.route" --workspace . --standalone --json`: returned `isError: false`

## Blockers hit
- Claude CLI did not obey the requested strict schema on either run. The stream output still contained usable review findings.
- The first system-tier run hit a SQLite lock in `test_system_health_reports_projection_revision_lag`. The exact test passed in isolation, then the full system tier passed on rerun.

## Files changed
- 44 files changed
- 2669 insertions
- 1260 deletions
- Major areas: `.memories/`, `dashboard/templates/`, `docs/plans/`, `languages/`, `src/analysis/`, `src/dashboard/`, `src/database/`, `src/search/`, `src/tests/`, `src/tools/metrics/`

## Next steps
- Review PR: https://github.com/anortham/julie/pull/15
- Exercise the live dashboard after rebuilding the release binary and reconnecting the MCP client.
