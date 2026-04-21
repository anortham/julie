# Dependency Upgrade Audit Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development (on Claude Code) or razorback:subagent-driven-development (elsewhere) to implement this plan. Fall back to razorback:executing-plans for single-task or tightly-sequential plans.

**Goal:** Audit Julie's core dependency stack, preserve the findings in-repo, and execute one measured pilot upgrade that has clear Julie payoff with manageable migration risk.

**Architecture:** Split the work into two tracks. First, capture the release audit and shortlist in a checked-in findings doc so future sessions do not have to rediscover the same release-note analysis. Second, execute the single pilot that cleared the bar, `rusqlite` to `0.39.0`, while explicitly deferring `tantivy` to its own plan and noting that `sqlite-vec`, `rmcp`, `tokio`, and `notify` do not warrant a second pilot in this pass.

**Tech Stack:** Rust, Cargo, rusqlite, sqlite-vec, Tantivy, rmcp, notify, tokio, Julie test harness (`cargo nextest`, `cargo xtask test changed`, `cargo xtask test dev`)

---

## Research snapshot that drives this plan

- `tantivy` is resolved at `0.22.1`; upstream `0.26.0` has meaningful query and index-lifecycle fixes, but the migration risk is high enough to deserve a dedicated plan because Julie's schema compatibility check is shallow and persisted index behavior matters.
- `sqlite-vec` is already resolved at stable `0.1.9`; there is no net-new pilot upgrade to do here.
- `rmcp` and `tokio` are already resolved at `1.5.0` and `1.52.1` through Cargo resolution, so there is no behavior change to test from a second pilot unless we choose manifest-alignment cleanup later.
- `notify` is already on latest stable `8.2.0`; the next line is `9.0.0-rc.x`, which is not a measured-pass target.
- `rusqlite` is the one contained upgrade with direct Julie value in this pass: newer bundled SQLite, bug fixes, and manageable code fallout.

### Task 1: Write the dependency audit findings doc

**Files:**
- Create: `docs/plans/2026-04-19-dependency-upgrade-audit-findings.md`

**What to build:** Write a concise findings document that records the current manifest and resolved versions, the last-6-month upstream changes that matter to Julie, and the upgrade recommendations for each candidate dependency. This doc is the durable output of the audit, not a scratchpad.

**Approach:**
- Record both the manifest requirements and the currently resolved versions from `cargo tree -p julie --depth 1`, because Cargo caret semantics already pull Julie onto newer stable `rmcp`, `tokio`, and `sqlite-vec` than the manifest text suggests.
- For each audited dependency, summarize only Julie-relevant release-note items, then classify as `pilot now`, `defer`, or `separate plan`.
- State plainly that `tantivy` is the next high-value candidate but should not be bundled into this measured pass.

**Acceptance criteria:**
- [ ] The findings doc covers `tantivy`, `sqlite-vec`, `rusqlite`, `rmcp`, `notify`, and `tokio`.
- [ ] Each dependency entry includes current manifest version, resolved version, latest realistic target, Julie-facing upstream changes, risk, and recommendation.
- [ ] The doc explains why only `rusqlite` clears the bar for this pilot.
- [ ] The doc explicitly defers `tantivy` to a separate plan.

### Task 2: Pilot the `rusqlite` upgrade to `0.39.0`

**Files:**
- Modify: `Cargo.toml:57-58`
- Modify: `Cargo.lock`
- Modify: `src/database/mod.rs:66-142`
- Modify: `src/database/files.rs:284-317`
- Modify: `src/database/symbols/search.rs:442-463`
- Modify if needed for API fallout: `src/analysis/change_risk.rs`
- Test: `src/tests/core/embedding_deps.rs:21-62`
- Test: `src/tests/core/vector_storage.rs:146-170`
- Test: `src/tests/core/vector_storage.rs:299-323`
- Test: `src/tests/core/memory_vectors.rs:39-52`
- Test: `src/tests/core/memory_vectors.rs:71-83`
- Test: `src/tests/analysis/change_risk_tests.rs:102-182`

**What to build:** Upgrade Julie from `rusqlite 0.37` to `0.39.0`, then make the smallest code changes needed to restore compatibility and keep sqlite-vec-backed vector storage healthy. The pilot should prove the upgrade is safe in Julie's actual database paths, not only that Cargo resolves it.

**Approach:**
- Expect the main source fallout to be integer parameter binding, because `rusqlite 0.38+` removed default `usize` and `u64` SQL conversions unless extra features are enabled.
- Fix the known raw `usize` binds first in `src/database/files.rs` and `src/database/symbols/search.rs` by converting limits to explicit SQLite integer types instead of turning on compatibility features.
- Keep the `sqlite-vec` integration path under `src/database/mod.rs` working without widening scope. If `rusqlite 0.39.0` exposes another compile or runtime issue on Julie's database paths, fix that path with the narrowest possible change.
- Do not change tree-sitter, Tantivy, or watcher code in this task.

**Acceptance criteria:**
- [ ] `Cargo.toml` and `Cargo.lock` resolve to `rusqlite 0.39.0`.
- [ ] Julie compiles without relying on deprecated integer-binding behavior.
- [ ] `cargo nextest run --lib test_sqlite_vec_registration_and_version`
- [ ] `cargo nextest run --lib test_sqlite_vec_vector_roundtrip`
- [ ] `cargo nextest run --lib test_delete_embeddings_for_file`
- [ ] `cargo nextest run --lib test_migration_010_is_idempotent`
- [ ] `cargo nextest run --lib test_migration_012_is_idempotent`
- [ ] `cargo nextest run --lib test_delete_memory_embedding`
- [ ] `cargo nextest run --lib test_compute_change_risk_scores`
- [ ] After the narrow loop, `cargo xtask test changed` passes.
- [ ] After the completed batch, `cargo xtask test dev` passes once.

## Execution notes

- Follow TDD for any behavior fix discovered during the upgrade. If the upgrade only triggers compile fallout, keep the change minimal and use the existing narrow regression tests above.
- Use an isolated worktree for execution unless the user opts out.
- Stop after the `rusqlite` pilot and findings doc unless a second candidate becomes compelling during execution. Do not force a second upgrade for symmetry.
- When the plan is complete, the summary should call out three buckets: `done now`, `deferred`, and `separate plan needed`.
