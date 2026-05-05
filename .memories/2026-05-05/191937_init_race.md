---
id: checkpoint_init_race_191937
timestamp: 2026-05-05T19:19:37Z
tags:
  - standalone
  - database
  - race
  - sqlite
  - verification
git:
  branch: codex/standalone-init-race
  commit: "37dc7cd0"
summary: Hardened concurrent standalone database initialization
briefId: julie-world-class-systems-program
type: checkpoint
context: User asked to investigate the standalone initialization race observed
  after the tree-sitter release-binary dogfood restart.
impact: Concurrent standalone processes now serialize SQLite schema setup through
  a per-database advisory init lock. This prevents both observed failure modes:
  `database is locked` during WAL setup and `table symbol_vectors already exists`
  during migration 010.
evidence:
  - Two concurrent standalone searches against a fresh disposable workspace
    reproduced the race before the fix with `Failed to enable WAL mode:
    database is locked`.
  - `cargo nextest run --lib symbol_database_new_serializes_concurrent_initialization`
    failed before the fix with WAL lock and `symbol_vectors` migration errors.
  - The same exact regression test passed after adding the init lock.
  - A rebuilt debug CLI survived five two-process standalone search races
    against tiny git workspaces with both searches finding their target symbols.
  - `cargo fmt --check` passed.
  - `git diff --check` passed.
  - `cargo nextest run --lib test_run_cli_tool_standalone_definition_search_uses_bootstrapped_index`
    passed.
  - `cargo xtask test changed` fell back to dev and passed 22 buckets in 332.4s.
symbols:
  - SymbolDatabase::new
  - symbol_database_new_serializes_concurrent_initialization
  - bootstrap_standalone_handler
next: Merge the branch to main, build release, then rerun the two-process
  standalone race and live release dogfood on the merged binary.
confidence: 5
unknowns:
  - A markerless temp directory exposed a separate CLI workspace-root quirk, but
    tiny git workspace reproductions cover the original standalone race.
---

## Hardened Concurrent Standalone Database Initialization

### WHAT
Added a per-database advisory init lock around `SymbolDatabase::new()` and a regression test that opens the same fresh SQLite database from 12 concurrent threads.

### WHY
The observed standalone failure was not just a CLI orchestration problem. Concurrent openers raced while enabling WAL and running migrations. The same root cause produced both `database is locked` and `table symbol_vectors already exists`.

### HOW
Used `fs2::FileExt::lock_exclusive()` on a lock file next to the target SQLite database. The lock is held only during connection setup, migrations, and schema initialization, then released when `SymbolDatabase::new()` returns.

### IMPACT
Standalone CLI processes no longer collide while initializing the same workspace database. The fix lives at the DB boundary, so daemon, standalone, and future callers share the same protection.
