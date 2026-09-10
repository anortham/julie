# Finding: Machine Service Phase 2 Gate (One Writer, Disposable Indexes)

**Date**: 2026-09-10
**Branch**: `machine-service`, commits `a3ef3e5a..46bb53da` (27 commits after the phase 1 tree)
**Plan**: `docs/plans/2026-09-09-machine-service-phase2-plan.md`
**Ledger**: `docs/plans/2026-09-09-machine-service-phase2-ledger.md`

---

## Verdict: GATE PASSED

Design section 14 sets two conditions for phase 2: the section 5.4 deletion list
is empty, and the phase is net negative in lines. Both hold, with two items
deferred by the plan's recorded sequencing decision (native broker client and
embedding generations move to phase 4).

## Line counts

`tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust`

| Tree | Files | Lines | Code |
|---|---|---|---|
| main `8b07074b` | 652 | 158,860 | 129,055 |
| phase 2 base `a3ef3e5a` (phase 1 landed) | 658 | 159,488 | 129,739 |
| phase 2 head `46bb53da` | 618 | 145,367 | 118,117 |

Phase 2 removed 14,121 lines (11,622 code lines) against its base and 13,493
against main. The whole branch diff since the base is 403 files, +4,371 / -68,697
(tests, fixtures, and docs included).

## Deletion-list ledger (design section 5.4)

| Item | Status | Evidence |
|---|---|---|
| `leadership.rs` | deleted | Task 1 (7caa96e7) and Task 2 (386f49d6); `src/tests/service/durable_roots.rs` fails on any `leader.lock` |
| Daemon lock guards (`DaemonLockGuard`, `OwnerEpoch`, `WriterPermit`, host slots) | deleted | Task 2 (386f49d6); `.symbols.db.init.lock` and `fs2` locking deleted in Task 9 (7944312f); `fs2` remains only for `available_space` |
| Publication locks | deleted | Task 2 (386f49d6) |
| Per-session workspace bootstrap (primary swap, session attachment, deferred auto-index, MCP roots) | deleted | Task 3 (20160dfe, ebdc4050); `RuntimeFactory` binds one handler per `(root, index_root)` |
| Continuation store and `spillover_get` | deleted | Task 4 (7209380a); 12 tools remain; over-limit output ends with a truncation line |
| Python embedding host | deleted | Task 6 (b2a4dfb2, 967394eb); -14,565 lines; tests default to `JULIE_EMBEDDING_PROVIDER=none` |
| Sidecar broker client | deferred to phase 4 because the native sidecar is the only remaining semantics provider and its client is the seam phase 4 replaces with embedding generations | plan section "Sequencing decision"; `sidecar_protocol.rs` kept for the client's types |
| Migrations for derived state | deleted for `symbols.db` (Task 5, 6f069f49): a schema or engine version mismatch deletes `indexes/<id>/` and reindexes; `registry.db` keeps its small migrations because it is per-machine durable state, not a derived index | `schema_version_matches`; `SEMANTIC_INDEX_ENGINE_VERSION` ends with `+schema=32` |
| Embedding generations | deferred to phase 4 with the broker client | plan section "Sequencing decision" |

No cross-process lock remains in product code, so ADR-0004's cross-process form
is gone with the rest.

## Report-only numbers

| Measure | Value |
|---|---|
| `symbols.db` for the Julie worktree (`~/.julie-dogfood`) | 299 MB |
| `tantivy/` for the Julie worktree | 96 MB |
| Sibling seed of a Julie worktree (`manage_workspace open`) | 14.0 s seeded vs 35.9 s from scratch (copy-bound on a 370 MB Tantivy directory at the time of the Task 7 run) |
| `cargo xtask test fast` | 25.3 s warm at 608750cf |
| `cargo xtask test dev` | 57.9 s warm at 46bb53da (about 195 s before Task 6) |
| `cargo xtask test full` | 471 s warm at 46bb53da, 51 buckets |
| Leaked test processes after the tiers | 0 from this worktree (one 24-hour-old `julie-embedding-host` belongs to the main checkout's debug binary) |
| Complexity words in added product code (`scripts/complexity-words.sh main`) | 15 hits, all ruled accepted in Task 9: 4 moved lines and 11 pre-existing names |
| `cargo audit` | 4 vulnerabilities and 8 warnings, identical to main's `Cargo.lock`; none introduced here |

## Docs check

`rg -n 'leader|follower|writer fencing|Python sidecar|leader\.lock' CLAUDE.md AGENTS.md docs/WORKSPACE_ARCHITECTURE.md`
returns three lines, each stating that the mechanism no longer exists
(`CLAUDE.md:404`, `AGENTS.md:404`, `docs/WORKSPACE_ARCHITECTURE.md:113`).
`diff CLAUDE.md AGENTS.md` is empty.

## Branch-gate fixes made by the lead during Task 10

Each of these was a test that encoded deleted behavior or a pre-existing failure
on main that the `full` tier had not been run against:

- `test_non_force_root_swap_tears_down_old_search_index` deleted (primary swap is gone).
- `in_process_boundary` tripwires now assert the no-args arm runs the stdio shim and that `src/registry/discovery.rs` is gone.
- The startup repair plan no longer schedules a missing-embeddings catch-up when no provider can exist (`JULIE_EMBEDDING_PROVIDER=none` and nothing injected); an injected provider still counts.
- Ten workspace tests inject their embedding provider on the handler; the workspace object they used to write to is rebuilt by a force index.
- Two search enrichment tests use a string literal marker: julie-extractors v2.41 now extracts local `let` bindings as symbols, which routed the old fixtures to the definition-first path.
- Eight rename tests call the handler's `execute_rename_symbol` (the tools-crate entry only prepares); they failed on main `8b07074b` too. A dry run with zero matches now reports the no-references message.
- The non-force refresh routing test no longer expects the loaded workspace to swap.
- The concurrent `SymbolDatabase::new` race test deleted: one handler per checkout initializes `symbols.db` once.
- The search-quality snapshot (`fixtures/databases/julie-snapshot/`, schema 9, committed in February) can no longer be opened because migrations are gone. A rebuilt snapshot is about 300 MB, above GitHub's 100 MB file limit, so the snapshot is now git-ignored and rebuilt on demand by `ensure_julie_fixture` (about 40 s once per checkout; rebuilt when the schema or engine version changes). `fixtures/search-quality/` is excluded from indexing so the OR-fallback sentinel is not indexed. Three dogfood tests were adjusted to the current tree. This is the one decision in the phase that changes what the repository tracks; it is reversible by committing a snapshot under a size limit or by adding Git LFS.

## Deferred findings (carried in the SDD ledger)

- `BindingResolver::is_unknown_selector` routes an unknown workspace id to the process workspace so the tool layer can reject it (Important; phase 3 engine binding must reject in the engine).
- Watcher `acquire_gate_or_mark_rescan` returns an `Option` that callers treat as "skip"; `metadata.rs:101` still says "migrate"; `startup_repair_plan.rs:72` checks the engine string only (which carries `+schema=32`).
- `ToolContext::session_id` is unused in production; `cargo xtask sync-plugin` is due before release.
- `last_file_event_at` in `manage_workspace status` is best-effort (2 s dedup map only).
- Dogfood: `fast-refs` lists a `pub use` re-export line twice; investigate in phase 3.
- julie-extractors emits call identifiers whose names are not plain identifiers (for example `(case.verify)`); the fast_refs dogfood probe skips them.
- A full-format `fast_search` whose query exactly matches a symbol shows the definition with a truncated signature instead of the source line; decide in phase 3 whether the definition-first path should carry the line text.
