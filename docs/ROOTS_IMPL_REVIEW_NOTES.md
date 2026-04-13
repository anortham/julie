# Roots Implementation Review Notes

**Scope:** Deep review of unpushed commits `93cbdf00`..`4b2e1fbc`, centered on `e004927e feat(workspace): resolve primary bindings from client roots` (14,916 insertions / 1,762 deletions across 72 files).

**Goal:** Confirm the current code is proper and working. Not verifying that output matches any original plan — the refactor included opportunistic fixes along the way.

**Status:** In progress. This document is the consolidated findings artifact.

---

## Review Structure

- **Pass 1** — Architectural spine (reviewed in-session by lead)
- **Pass 2** — Tool-level rebind changes (dispatched to 4 Opus teammates in parallel)
- **Pass 3** — Test coverage audit (part of Pass 2 teammate D)
- **Pass 4** — Synthesis + severity-ranked punch list

---

## Severity Legend

- 🔴 **Must-fix before push** — correctness, security, or user-facing break
- 🟡 **Should-fix** — real issue, can ship without but deserves a follow-up commit on this branch
- 🟢 **Nice-to-have** — style, minor clarity, non-load-bearing test gap
- ⚪ **Observation** — worth knowing, no action

---

## Findings

### Finding #1 ✅ FIXED — Version gate now rejects mismatched session when sessions > 0

Implemented in Commit 1. Refactored the gate into a pure `evaluate_version_gate(adapter_version, daemon_version, active_sessions) -> VersionGateOutcome` function in `src/daemon/ipc_session.rs`, with regression tests covering matching versions, missing version header (legacy adapter), no-active-sessions shutdown, active-sessions reject, and bidirectional mismatch. The accept loop in `src/daemon/mod.rs` now drops the stream and `continue`s on the reject path instead of falling through.

**Pass 1 was wrong (was 🔴, partially revised, now ✅):**

**CORRECTION (post-implementation-review):** Pass 1 was wrong that the gate didn't exist. `src/daemon/mod.rs:769-796` already compares `adapter_version != daemon_version` and triggers `restart_pending`. The sessions=0 path correctly shuts down immediately. The **bug** is in the sessions>0 branch: it sets `restart_pending` but falls through and still serves the mismatched session, which may fail mid-flight on a protocol difference. The reproducer we hit during setup was actually same-version-string-different-code (development-time only, since Cargo.toml hadn't been bumped for the roots work yet), which the gate can't catch without content hashing — but in real plugin upgrades the version bumps and the gate does fire.

**Original description (kept for context):**

**Where:** `src/daemon/ipc_session.rs`, `src/adapter/mod.rs`, `src/daemon/mod.rs` (handshake path).

**What:** When a user upgrades the plugin binary to a new version that includes roots support while an *older* daemon is still running on `~/.julie/daemon.sock`, every new MCP session fails silently with:

```
WARN julie::daemon: MCP serve failed: connection closed: initialize request
```

Reproduced live during setup of this review: the plugin 6.7.0 binary (pre-roots) was running as daemon, and the local roots-aware `target/release/julie-server` adapter kept hitting initialize failures until we overwrote the plugin binary to match.

**Why this is load-bearing:**
- Plugin version upgrades install to a new path (e.g. `.../julie-plugin/julie/6.7.1/...`). The old daemon's captured binary mtime stays valid, so the stale-binary auto-handoff doesn't trigger.
- User-visible symptom is a broken MCP connection with no actionable error — "julie failed" in `/mcp`.

**Options to fix:**
1. Version-gate the IPC handshake: adapter sends `VERSION:x.y.z`; daemon refuses or triggers self-restart if `adapter_version > daemon_version`. Header already exists, just needs enforcement.
2. Daemon sends its own version in the handshake reply; adapter detects mismatch and disconnects cleanly with an actionable error.
3. Short-term: document in release notes that upgrades require killing `~/.julie/daemon.*` files.

Recommendation: option 1 with a clear log message on the daemon side ("adapter expects roots protocol, daemon is pre-roots; restart daemon"). Cheap to add and prevents a silent-hang foot-gun.

**Discovered during:** initial setup, before formal review began.

---

*(Findings from Pass 1 architectural review will be appended below.)*

## Pass 1 Findings (Architectural Spine)

Files read in full: `src/workspace/startup_hint.rs`, `src/handler/session_workspace.rs`, `src/adapter/mod.rs`, `src/daemon/ipc_session.rs`, `src/startup.rs`, `src/cli.rs`. Spot-read in `src/handler.rs` (focused on constructor set, reconcile/ensure flow, MCP protocol hooks, swap/rollback), `src/daemon/mod.rs` (stale-binary detection, accept loop).

### Things that work well (observations, no action)

- ⚪ **Deferred-workspace handler** (`new_deferred_daemon_startup_hint`) is the right call. When startup hint is `Cwd` (weakest signal), handler boots without attaching to any workspace; first primary-scoped request resolves via client roots. Avoids speculative indexing for short-lived sessions.
- ⚪ **Stale-binary detection at session disconnect** (`src/daemon/mod.rs:841-862`): caught a rebuild we intentionally triggered during review setup and auto-restarted cleanly. Well-commented, including the race-closure rationale.
- ⚪ **`PrefixedIpcStream`** (`src/daemon/ipc_session.rs:60-117`): elegant solution for the "we accidentally consumed one byte past the header block" problem. Stashes already-read bytes and replays them before polling the underlying stream.
- ⚪ **Lazy reconcile on root changes** (`on_roots_list_changed` only marks dirty; `ensure_primary_workspace_for_request` reconciles): avoids thundering-herd reconciliation during rapid root changes. Slight staleness window is acceptable.
- ⚪ **Atomic `is_indexed` claim at `on_initialized`** (`src/handler.rs:2763-2771`): write lock held only for the check+set, dropped before `.await`. Inline "Fix E" comment shows the author knew the trap. Good.
- ⚪ **`PrimarySwapRollback`** (`src/handler.rs:68-135`): captures full handler state (workspace, loaded_id, loaded_root, session_workspace) and can restore on swap failure. Right pattern for a multi-step operation that can fail mid-way.
- ⚪ **CLI resolution order is explicit**: `--workspace` > `JULIE_WORKSPACE` > cwd, with source tracking so the daemon knows how much to trust the hint.

### Findings

#### Finding #2 ⚪ (WITHDRAWN — Pass 1 error) Version header IS consumed

Pass 1 claim was wrong. `headers.version` is consumed in `src/daemon/mod.rs:769-796`. Left here as a note on lead fallibility; the real issue is the refined Finding #1 (sessions>0 branch falls through). Also: the existing gate has no regression test — folded into Commit 1 scope.

---

#### Finding #3 ⚪ Removing a client root preserves its secondary workspace — intentional, test-backed

**Revised after Pass 2 (reviewer D).** Originally flagged as 🟡. D found that `test_roots_list_changed_startup_hint_fallback_preserves_active_secondary` in `src/tests/daemon/roots.rs:1378` **explicitly asserts** that secondaries persist after a root-list shrink. The "preserved set" name is also in the test identifier. So this is deliberate design: when the client drops a root, the secondary workspace stays active for the session (useful for cross-workspace queries that were in flight).

**Remaining concern for author:** the intent isn't commented in the test or the code. A future reader sees the behavior and wonders if it's a bug (I did). Add a sentence to `reconcile_primary_workspace_roots` explaining why previously-attached secondaries are preserved across root-list shrinks. Low priority but cheap.

(Original mechanics still correct as a description of the code path — it's just that the conclusion "bug or intentional" now has an answer.)

---

#### Finding #4 🟡 `unwrap_or_else(|p| p.into_inner())` on `SessionWorkspaceState` is pervasive

**Where:** `src/handler.rs` — 30+ sites on `self.session_workspace.read/write().unwrap_or_else(|p| p.into_inner())`.

`PoisonError::into_inner()` recovers the wrapped value, which for `SessionWorkspaceState` means invariants between `primary_binding`, `secondary_workspace_ids`, `attached_workspace_ids`, and `primary_swap_in_progress` could be partially updated if a panic occurred mid-mutation. Silently continuing with partially-updated state could e.g. leave `primary_swap_in_progress = true` forever (nothing ever calls `complete_primary_swap`), which would make `current_workspace_id()` return `None` for the rest of the session.

The right fix is either:
1. Convert the fields that have a multi-step invariant into an `enum` representing atomic states, so there is no partial update.
2. On poison, log loudly *and* reset the state to a known-safe default (e.g. clear swap flag, drop secondaries).

Shipping as-is is tolerable because `SessionWorkspaceState` mutations are bounded and unlikely to panic, but this is a fragility. At minimum a `warn!` on poison recovery would surface it in the logs rather than being silent.

---

#### Finding #5 🟡 `handler.rs` is 2,796 lines — 5.6× the project's own 500-line limit

**Where:** `src/handler.rs`.

Project `CLAUDE.md` mandates ≤500 lines for implementation files. `handler.rs` was already over; this refactor grew it by +1,814. The new methods naturally group into: (a) root URI parsing (~40 lines), (b) roots capability/snapshot state (~40 lines), (c) primary workspace binding/resolution (~200 lines), (d) swap/rollback (~130 lines), (e) deferred indexing (~30 lines), (f) the existing constructor/tool set.

Recommend opportunistic extraction into `handler/roots.rs`, `handler/primary_swap.rs` while the author remembers all the edges. Not a blocker for push.

---

#### Finding #6 🟢 `tool_request_targets_primary` hardcodes tool names

**Where:** `src/handler.rs:740-757`.

New tools must remember to add themselves here or `ensure_primary_workspace_for_request` won't be called. A missing entry manifests as "No workspace initialized" errors on tools that should have auto-resolved. Consider a per-tool attribute or a trait method on the tool struct so this stays colocated with the tool definition. Cosmetic.

---

#### Finding #7 ⚪ CLI passes through nonexistent `--workspace` path — intentional, test-backed

**Revised after Pass 2 (reviewer D).** Originally flagged as 🟢. D found that `cli_tests.rs` renamed the prior test `_with_nonexistent_path_falls_through` to `_preserves_explicit_path` in this commit — the contract was deliberately changed: explicit CLI arg passes through even if the path doesn't exist (supports "boot against a not-yet-created directory" use case). No action.

---

#### Finding #8 🟢 `reconcile_primary_workspace_to_startup_hint` stores empty `Vec` for `last_roots_snapshot`

**Where:** `src/handler.rs:636` passes `Vec::new()` to `apply_root_snapshot`.

This overwrites any prior non-empty `last_roots_snapshot` with an empty vec. Then `ensure_primary_workspace_for_request:683` has `self.last_roots_snapshot().filter(|roots| !roots.is_empty())` to fall back to cached roots when `list_roots` fails — but this cache is wiped every time the fallback path runs. If `list_roots` fails twice in a row, the second fallback no longer has the cached roots to use.

Fix is to not touch `last_roots_snapshot` in the startup-hint reconciliation path (pass `None` instead, or don't update it). Minor — fallback chain is already robust enough.

---


## Pass 2 Findings (Tool-level Rebind)

### Reviewer A — Navigation tools

Scope: `deep_dive/mod.rs`, `navigation/fast_refs.rs`, `navigation/resolution.rs`, `get_context/pipeline.rs`, `refactoring/{mod,rename}.rs`. Full writeup at `/tmp/roots-review-A-navigation.md`.

**Verdict:** No 🔴. Rebind plumbing is coherent — every primary-path DB acquisition now routes through `primary_database()` / `primary_database_and_search_index()` / `require_primary_workspace_root()`, all of which go through `require_primary_binding()` and honor `primary_swap_in_progress`. This closes the stale-workspace hole the refactor was designed to close. Previously, these tools called `handler.get_workspace().await?` and wouldn't have observed the swap flag at all — real correctness upgrade.

#### Finding #9 🟡 Stdio-mode `resolve_workspace_filter` silently accepts unknown secondary IDs

**Where:** `src/tools/navigation/resolution.rs:118-119`.

When `daemon_db` is `None` (stdio mode), the resolver skips validation entirely and returns `Ok(Reference(workspace_id))` for *any* string, including typos. The added active-session gate (lines 86-105) made the daemon-mode path stricter; the stdio path stayed permissive, producing a downstream DB-open failure with no suggestion. Pre-existing, but the new daemon gate throws the asymmetry into relief.

**Fix:** in stdio mode, validate against `handler.loaded_workspace_id()` / attached set; return a parallel "not attached" error.

#### Finding #10 🟢 `rename_symbol` captures DB and root in separate `await`s — TOCTOU window

**Where:** `src/tools/refactoring/rename.rs:75` (snapshots `primary_db`) vs. downstream `resolve_workspace_root` → `handler.require_primary_workspace_root()` in `refactoring/mod.rs:115`.

A primary-swap between those two calls causes the rename to write file changes against a tree whose indexed references it didn't compute. Rename is the only tool in this slice that mutates the filesystem, so the consequence is writes against a workspace that was never queried. Rare in practice.

**Fix:** derive both DB and root from a single `primary_workspace_snapshot()` / `require_primary_workspace_binding()` call. The request-level `ensure_primary_workspace_for_request` already pins the binding; tool just needs to read both from the same binding.

#### Finding #11 🟢 `FastRefsTool` threads `Option<Arc<Mutex<DB>>>` when invariant is "Some ⟺ Primary"

**Where:** `src/tools/navigation/fast_refs.rs:67, 213, 259`.

`primary_db: Option<Arc<Mutex<SymbolDatabase>>>` passes through three signatures. The `None` branches are defensive but unreachable given current callers. Turn the invariant into a type:

```rust
enum FastRefsTarget {
    Primary(Arc<Mutex<SymbolDatabase>>),
    Reference(String),
}
```

Non-load-bearing cleanup for the next time the author is in the file.

#### Other observations (no action)

- ⚪ Swap-safety is correctly centralized — every primary-DB acquirer in this slice honors `primary_swap_in_progress`.
- ⚪ Per-call DB snapshot is internally consistent within `fast_refs` — exact + variant + relationship + identifier + semantic-fallback all share one `Arc`.
- ⚪ `get_context` fetches embedding provider from handler (not the loaded workspace) — intentional in daemon mode where the provider is shared via `EmbeddingService`.
- ⚪ No `TODO`/`FIXME`/`unimplemented!()` in changed code.
- ⚪ Error messages are actionable; one edge case at `fast_refs.rs:291` has internal-sounding wording but the branch is unreachable via current callers.

**Handoff to D:** check whether the rename TOCTOU window (Finding #10) has any regression test, and whether `fast_refs` primary-rebind tests actually assert the post-rebind DB is used (not just that the tool runs).

### Reviewer B — Search + symbols

Scope: `search/{mod,text_search,line_mode}.rs`, `symbols/{primary,reference}.rs`. Full writeup at `/tmp/roots-review-B-search-symbols.md`.

**Verdict:** Zero 🔴, three 🟡, two 🟢, two ⚪. Core rebind plumbing is sound — `primary_workspace_snapshot()` (`handler.rs:1730-1758`) *re-reads* loaded workspace ID and compares roots before reusing handles, so if a swap completed between `require_primary_workspace_binding()` and the snapshot, the helper correctly falls through to the pool/disk path. Search and symbols tools pair primary-binding with matching DB/Tantivy handles atomically. That's the exact swap-stale-handle hazard the refactor exists to close, and it's closed in this slice.

#### Finding #12 🟢 Definition-search NotReady branch has duplicate index-presence check (dead for the original bug)

**Where:** `src/tools/search/mod.rs:142-203`.

Added guards emit specific messages when the Tantivy index is missing for primary or a reference target, but the final fall-through path is unchanged:

```rust
let message = "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first.";
```

If `HealthChecker::check_system_readiness` reports `NotReady` for a definition search where primary DB *and* Tantivy index both exist (the exact scenario `tests/tools/search/primary_workspace_bug.rs` captures: "7,917 symbols indexed but fast_search returns 'Workspace not indexed yet'"), this path still short-circuits with the wrong message. The new guards only improve the error when DB exists but Tantivy doesn't; the "false NotReady" case is not patched here.

**Resolved after lead verification:** the fix for `primary_workspace_bug.rs` lives in `src/health.rs:89-131` — `HealthChecker::check_system_readiness` was rewritten to distinguish `FullyReady` / `SqliteOnly` / `NotReady`. An indexed primary with no Tantivy now returns `SqliteOnly`, not `NotReady`. So the `else` branch in `search/mod.rs:142-203` that returns "Workspace not indexed yet" is only reached now when DB symbols = 0 or health is ColdStart — correct messages for those cases.

B-1's real residual concern is **duplicate index-presence checks**: search/mod.rs re-checks DB + Tantivy presence after HealthChecker already reported readiness. Not a correctness issue, but cleanup opportunity (collapse into HealthChecker's output). Downgraded from 🟡 → 🟢.

#### Finding #13 🟡 `text_search_impl` silently returns empty when reference Tantivy index missing

**Where:** `src/tools/search/text_search.rs:93-99`.

`si_arc == None` → `return Ok((Vec::new(), false, 0))`. Every other rebind callsite in this commit returns an actionable error for the same scenario (`mod.rs:230-237`, `line_mode.rs:152-158`). The silent branch is dead code under current callers, but `text_search_impl` is `pub`; any future programmatic caller (or a secondary tool) gets a silent empty result with zero diagnostic.

**Fix:** replace silent `Ok(empty)` with `Err("Definition search requires a Tantivy index for workspace '{}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{}\") first.")`.

#### Finding #14 🟡 `symbols/reference.rs` absolute-path branch swallows `get_workspace_root_for_target` errors

**Where:** `src/tools/symbols/reference.rs:38-50`.

```rust
let query_path = match handler.get_workspace_root_for_target(&ref_workspace_id).await {
    Ok(root) => to_relative_unix_style(&canonical, &root).unwrap_or_else(|_| file_path.to_string()),
    Err(_) => file_path.to_string(),  // <-- swallows registry error
};
```

If `get_workspace_root_for_target` errors (registry can't resolve the workspace ID — exactly the case the user needs to know about), `query_path` becomes the absolute path string and never matches the relative paths in the DB. The fallback at `:123-135` retries with path-separator normalization (still absolute), misses again, and the user gets `"No symbols found in: {file_path}"` with no hint that the real issue is "workspace not registered."

**Fix:** propagate the error via `?` (the relative-path branch at `:53-57` already does this correctly), or at minimum `warn!` and include a helpful suffix in the final error.

#### Finding #15 🟢 Index-presence gating duplicated 4× in `search/mod.rs`

**Where:** `mod.rs:142-198`, `:213-236`, `:251-271`, and corresponding defenses in `line_mode.rs:69-73`.

Same check with slightly different message formatting. One helper (`validate_index_presence`) collapses them. Not a defect, just maintenance debt that landed in the same commit.

#### Finding #16 🟢 `text_search.rs` destructuring swaps tuple positions

**Where:** `src/tools/search/text_search.rs:138-141`.

Inner tuple from `primary_database_and_search_index()` is `(db, search_index)`; outer re-binds as `(search_index_clone, db_clone, ...)`. Correct today, but a future reader changing one side has a 50% chance of swapping types. Bind directly: `let (db_clone, search_index_clone) = ...;`.

#### Other observations

- ⚪ **`line_mode.rs` test:impl ratio is 33:1** (+31 impl, +1033 test). Either line-mode was fragile across many edge cases (worth a project-memory note on *why*) or tests are over-parameterized. Reviewer D's territory.
- ⚪ `primary_workspace_snapshot` correctly defends against swap-stale handles. Positive datapoint recorded.
- ⚪ `fast_search` / `get_symbols` are in `tool_request_targets_primary`, so primary binding is reconciled before tool body executes.
- ⚪ No cached handler-construction-time references to `search_index` / `db` survive; every primary-target site re-resolves per call.
- ⚪ Relative-path computation uses dynamic root via `binding.workspace_root` per call (not a captured-at-construction root).

### Reviewer C — Workspace + editing

Scope: `workspace/commands/{index,registry/*}`, `workspace/indexing/*`, `editing/{edit_file,edit_symbol}`, `metrics/mod.rs`. Full writeup at `/tmp/roots-review-C-workspace-editing.md`.

**Verdict:** Zero 🔴. Five 🟡 (one user-visible UX regression, two future-race hazards), three 🟢, two ⚪ positives. Metrics attribution during swap is confirmed correct (C-9).

#### Finding #27 ✅ FIXED — `list` / `add` / `remove` no longer mislead in deferred sessions

Implemented in Commit 2. `list` and `remove` switched from `require_primary_workspace_identity()` to `current_workspace_id()` (Option), with sensible fallbacks — `list` shows registered workspaces with no `CURRENT`/`PAIRED` labels, `remove` skips the pairing-cleanup step when no primary is bound. `add` keeps the hard fail but with a new error pointing at `manage_workspace(operation="open", path=...)` or client roots instead of the irrelevant `index` operation. Three regression tests in `global_targeting.rs` cover the three surfaces.

**Original description:**

**Where:** `registry/list_clean.rs:17`, `registry/add_remove.rs:21, 164`.

Switched from `handler.workspace_id.as_deref().unwrap_or("primary")` to `handler.require_primary_workspace_identity()?`. In a deferred daemon session where roots haven't arrived, this returns `"No workspace initialized. Run manage_workspace(operation=\"index\") first."` But:

- `list` should show all registered workspaces regardless of session state.
- `remove <id>` only needs primary for pairing-metadata cleanup — can gracefully skip.
- `add` legitimately needs primary, but "run index" misdirects — the user has nothing to index yet.

Before: the `"primary"` sentinel made `list_references("primary")` return an empty set harmlessly. Now the whole call short-circuits.

**Fix:** for `list`/`remove`, make primary id optional (err → empty `paired_ids`, skip pairing cleanup). For `add`, keep hard fail but return `"Cannot add a reference workspace before a primary is bound. Open a primary first via client roots or manage_workspace(operation=\"open\", path=...)."`

#### Finding #28 🟡 `refresh` re-initializes primary outside `PrimarySwapRollback` machinery

**Where:** `registry/refresh_stats.rs:152-172`.

After a successful `refresh_single_workspace`, when `current_workspace_id == workspace_id` but `loaded_workspace_id != workspace_id` (primary was rebound but handler's loaded workspace still points at the old root), the code calls `handler.initialize_workspace_with_force(Some(success.workspace_path), false)` — which **mutates `handler.workspace` and `session_workspace.primary_binding` outside the swap machinery**.

Two risks: (1) concurrent `ensure_primary_workspace_for_request` mid-swap races and can partially clobber swap state; (2) `force=false` means a no-op reinit if primary hasn't been indexed at the new root — caller sees "Refresh success" but loaded workspace stays stale.

**Fix:** route through swap machinery, or guard with `is_primary_workspace_swap_in_progress()`. At minimum, verify refresh actually populated the target index.

**Synthesis note:** #28 and #29 together represent a class of hazard — secondary code paths mutating the same state the swap machinery guards. Even if nothing fails today, this is the bug shape the machinery exists to prevent. Worth closing on this branch.

#### Finding #29 🟡 `open` on an unindexed primary bypasses the swap path

**Where:** `registry/open.rs:110-122`.

When `target.is_primary == true` and `target.status != "ready"`, we call `handle_index_command(..., force, false)`. `explicit_path_requested=true` picks `current_workspace_root()` (not `require_primary_workspace_root()`), and if `loaded_workspace_matches_target=false`, directly calls `initialize_workspace_with_force` — same pattern as #28, same risk.

**Fix:** guard `handle_open_command` top with `is_primary_workspace_swap_in_progress()`, or funnel all "become-primary" transitions through the swap machinery.

#### Finding #30 ✅ FIXED — Editing tools reject unknown fields loudly

Implemented in Commit 2. Added `#[serde(deny_unknown_fields)]` to both `EditFileTool` and `EditSymbolTool`. `edit_file(workspace="x")` now returns a serde error naming the offending field instead of silently ignoring it and editing primary. Two regression tests cover both tools (plus sanity tests that known fields still parse).

**Original description:**

**Where:** `editing/edit_file.rs:39-59`, `edit_symbol.rs:24-44`.

Neither tool has a `workspace` field. Both resolve against primary unconditionally. Because serde accepts unknown fields by default, `edit_file(file_path="src/foo.rs", workspace="secondary-id")` silently ignores `workspace` and edits against primary. If a name collision exists, it edits the wrong file.

Not a refactor regression (old code same shape), but since cross-workspace targeting is a product feature elsewhere, silent-ignore is a footgun.

**Fix:** `#[serde(deny_unknown_fields)]` on the editing tool structs. Cheap; turns silent-ignore into loud error. True cross-workspace editing is a feature for a later branch.

#### Finding #31 🟡 `final_current_primary_id` naming overloaded between primary/reference

**Where:** `workspace/commands/index.rs:234-243, 289-302`.

Variable holds primary id in one branch, reference id in another. At line 241-243, `update_workspace_status(&final_current_primary_id, "ready")` fires inside `is_indexed && symbol_count > 0` regardless of `is_reference_workspace` — so a reference-path refresh can mark the reference as ready via the "primary" slot. Not a bug today (refresh writes reference stats separately later), but the overloaded name makes this fragile.

**Fix:** rename to `stats_target_id`; split branches so the "primary already-indexed status refresh" doesn't fire with a reference id.

#### Finding #32 🟢 `handle_refresh_command` passes `self.force.unwrap_or(false)` where `true` is provable

**Where:** `registry/refresh_stats.rs:152-157`. Inside `if self.force.unwrap_or(false) && ...`, the guarded expression is provably `true`. Pass the literal `true`.

#### Finding #33 🟢 Stdio reference-detection widened by `starts_with` — diverges from daemon's DB check

**Where:** `workspace/commands/index.rs:99-104`. New `!request_canonical.starts_with(&primary_canonical)` treats any subdirectory of primary as still-primary. If a user registers `primary/vendor/foo` as a secondary reference, stdio disagrees with daemon-mode's DB-backed detection.

**Fix:** accept asymmetry (stdio mode's reference support is thin anyway) or document the invariant.

#### Finding #34 🟢 `loaded_workspace_matches_target` canonicalize on hot path

**Where:** `workspace/commands/index.rs:156-163`. Syscalls every `index` call. Non-issue today; cache the canonicalized root on `JulieWorkspace` at load time for future-proofing against auto-index-from-roots-change hammering.

#### Finding #35 ⚪ Metrics writer correctly captures binding at queue time

**Where:** `handler.rs:1545-1602`, tool sites `2257-2655`.

`record_tool_call` takes a `workspace_snapshot: Option<&PrimaryWorkspaceBinding>`. Each tool site captures `require_primary_workspace_binding().ok()` **before** running the tool body; `MetricsTask` carries the queue-time snapshot; `run_metrics_writer` uses snapshot's `current_workspace_root` + `workspace_id`, not live handler state. Metrics attribute to binding at call time — correct under swap.

Minor wart: `MetricsTask.workspace: Arc<RwLock<Option<JulieWorkspace>>>` is still live-shared, but only used to recover `index_root_override` (stable in daemon mode). Safe but subtle; a one-line comment near the field would help future readers.

#### Finding #36 ⚪ Health check now accurately distinguishes "no DB" vs "no Tantivy"

**Where:** `registry/health.rs:150-167, 236-260`.

Old code said "Tantivy READY" whenever a DB was present. New requires both `db_ready && search_index_ready`. Honest improvement.


## Pass 3 Findings (Test Coverage — Reviewer D)

Full writeup at `/tmp/roots-review-D-tests.md` (per-file capsules included).

**Overall verdict: 8/10 — real TDD work, not performative.** New tests exercise actual MCP flows via `serve_directly` over duplex streams answering real `ListRootsRequest` messages, persist via `bulk_store_fresh_atomic`, and use polled `session_count` on the daemon DB for race-free reconciliation checks. Error branches (swap-gap, cold-start, neutral-gap, roots/list failure, secondary detach) are tested alongside happy paths. No tautologies, no `todo!()`, no banned patterns. Test-only state-forcing hooks are scoped to pushing state into specific phases rather than mocking tested logic — acceptable.

### Finding #17 🟡 `test_fresh_index_no_reindex_needed` is `#[ignore]`d ✅ FIXED (commit 3)

**Where:** `src/tests/integration/stale_index_detection.rs:22`.

Reason given: "Flaky due to filesystem timestamp resolution - needs investigation." This is the load-bearing **happy-path** claim of the freshness-check system: "fresh index does NOT trigger re-index." Every other test in the file verifies the TRUE branch of the check. Shipping the FALSE branch untested means a regression making `check_if_indexing_needed` always return `true` would pass CI with no complaint.

**Fix applied:**
1. Removed `#[ignore]` and replaced the 10ms sleep with `File::set_modified()` (stable since Rust 1.75), backdating the source file's mtime by 10s after indexing. This eliminates dependence on sub-second FS clock resolution.
2. **Real root cause found during diagnosis:** the test was not actually failing due to mtime resolution — it was failing because `.julieignore` (auto-generated by discovery) was being stored in the symbols DB as a tracked file, while `scan_workspace_files` excluded it (extension filter rejects extensionless files). The resulting "indexed files ⊃ scanned files" asymmetry triggered a phantom "deleted file" signal every time. Fixed by adding `.julieignore` to `BLACKLISTED_FILENAMES` in `src/tools/shared.rs`, since Julie's own config file produces no symbols and should never be indexed. Both sides now agree.

This was a pre-existing latent bug affecting every freshly indexed workspace in daemon mode: `check_if_indexing_needed` would return `true` on reconnection, triggering unnecessary re-indexing work even on an untouched workspace.

### Finding #18 🟡 Two `workspace_init.rs` tests don't exercise their SUT

**Where:** `src/tests/core/workspace_init.rs:25-97` (`test_workspace_detection_priority`), `:243-307` (`test_env_var_concept`).

Comments in-test admit `get_workspace_root()` is private so the tests fall back to asserting env vars they just set, or paths they just created, still exist. Not tests of priority logic — setup-verification tests. The actual priority logic IS tested by new `cli_tests.rs` around `resolve_workspace_startup_hint`, so the logic isn't unguarded, but these two tests are dead weight.

**Fix:** delete them, or make `get_workspace_root` `pub(crate)` for tests and exercise it.

### Finding #19 🟡 `std::mem::forget(temp_dir)` intentional leak in three rebind test helpers

**Where:** `deep_dive_primary_rebind_tests.rs:120`, `fast_refs_primary_rebind_tests.rs:~180`, `get_context_primary_rebind_tests.rs:135`.

Deliberate to sidestep async-teardown ordering issues, but each test leaks a tempdir for the CI process lifetime. With parallel test runs and repeated cargo invocations, this builds up in `$TMPDIR` across a CI session.

**Fix:** keep the `TempDir` in the returned tuple so it drops when the test finishes. Costs a field, buys proper cleanup.

### Finding #20 🟡 Weak `format!("{:?}", result).contains(...)` across rebind tests

**Where:** widespread across `deep_dive`, `fast_refs`, `get_context`, `rename_symbol`, `primary_rebind_metrics` rebind test files.

Brittle to `Debug` impl drift; false-positive risk (the substring could appear in error-diagnostic metadata not the rendered tool output). The codebase already has `extract_text_from_result` helpers — these tests should use them. Not incorrect, but softer than needed.

**Fix:** standardize on `extract_text_from_result` across rebind tests.

### Finding #21 🟡 Or-gated assertion in `primary_workspace_bug.rs`

**Where:** `src/tests/tools/search/primary_workspace_bug.rs:99-104`.

`contains("TestStruct") || contains("No results") || contains("🔍")` — three acceptable outcomes is two too many. The sharp assertion is the adjacent `!contains("Workspace not indexed yet!")`; the loose assertion adds no coverage. Tighten to require `"TestStruct"`.

### Finding #22 🟡 No test clarifies intentional secondary-preservation on roots shrink

Revised Finding #3 above: the behavior is intentional and test-backed by `test_roots_list_changed_startup_hint_fallback_preserves_active_secondary` (`daemon/roots.rs:1378`). But the test has no comment explaining *why* this is the intent. A future reader (me, just now) sees the behavior and suspects a bug. Add a doc-comment in the test *and* in `reconcile_primary_workspace_roots` stating the design decision.

### Finding #23 🟢 `daemon/roots.rs` is 2510 lines

Over the 1000-line test-file soft target. Natural splits: `roots_initialize.rs` (initialized-probe suite), `roots_list_changed.rs` (the 8 `_list_changed_` tests), `roots_secondary_and_filters.rs` (secondary-scoped-request + reference-first-request + explicit/env startup). Would halve read-time for future reviewers.

### Finding #24 🟢 `search/line_mode.rs` test file is mis-named

~80% of its 1033 lines are `fast_search` primary-rebind/swap-gap/filter tests — nothing to do with line-mode output. This explains the 33:1 test:impl ratio that reviewer B flagged. Rename to `fast_search_primary_and_filters.rs`, or split the line-mode tests out.

### Finding #25 🟢 Sleep-based waits in `line_mode.rs` tests

`sleep(500ms)` and `sleep(2s)` appear after indexing in several tests. Time bombs on slow CI. The codebase already uses polled `wait_for_session_count`; extend with `wait_for_index_ready` and retire the sleeps.

### Finding #26 🟢 Hardcoded Symbol fixtures duplicated across rebind tests

Fields like `code_context: Some("pub fn rebound_primary_symbol() {}".to_string())` are hand-rolled in each rebind test file. A `Symbol::test_default(name)` builder would cut ~40% of the setup boilerplate and reduce rot as the struct evolves.

### Observations (no action)

- ⚪ **No concurrency/race tests in the roots suite.** Every test is linear: one session, sequential JSON-RPC exchange. Given the inline "Fix E" atomic-claim logic in `handler.rs:2763-2771`, a test firing two near-simultaneous `on_initialized` + `tools/call` sequences would be worth it eventually. Acknowledged that rmcp's `serve_directly` is single-threaded-per-session by design, so this needs a harness extension.
- ⚪ **No tests for Findings #1/#2** (version header enforcement). Expected — the feature doesn't exist yet. Flagging so the punch list doesn't forget test coverage if the fix lands.
- ⚪ **MCP harness helpers duplicated** across 5+ test files (`send_json_line`, `read_server_message`, `extract_text_from_result`, `answer_roots_request`). A `src/tests/fixtures/mcp_harness.rs` would deduplicate ~300 lines.
- ⚪ **`handler.rs` `metrics_db_path_helper_uses_current_workspace_root_for_local_storage` uses `/tmp/...` literal paths.** Fine on macOS/Linux; will fail on Windows if the helper is ever cross-compiled. The adjacent UNC-URI test is cfg-gated; this one should match.
- ⚪ **Dogfood fixture now depends on `set_current_primary_binding`** (`search_quality/helpers.rs` +8). Not a test issue — noting that the dogfood suite is now load-bearing on the new primary-binding API.

## Findings discovered during fix-up (post-Pass 4)

### Finding #37 ✅ FIXED — daemon bucket added to xtask `dev` and `full`

Implemented in Commit 1.5. Added `[buckets.daemon]` to `xtask/test_tiers.toml` (~12s expected, 60s timeout) and updated the manifest contract test so future drift is caught. Now runs ~180 tests in `cargo xtask test dev`.

**Original description:**

**Where:** `xtask` tier definitions (verified via `cargo xtask test list`).

The `dev`, `full`, `system`, `dogfood`, and `smoke` tiers reference these buckets: cli, core-database, core-embeddings, tools-get-context, tools-search, tools-workspace, tools-misc, core-fast, workspace-init, integration, search-quality. **None invoke `tests::daemon`**, which contains:

- `tests::daemon::handler` (~352 lines)
- `tests::daemon::ipc_headers` (~305 lines)
- `tests::daemon::ipc_session` (~970 lines including new version_gate tests)
- `tests::daemon::roots` (2,510 lines — the crown-jewel roots flow tests)
- `tests::daemon::session_workspace` (~150 lines)

Total: ~4,200 lines of high-quality tests (per reviewer D's 8/10 audit) silently absent from CI. CLAUDE.md asserts "All tiers are currently green" — but only because the daemon tests aren't in any tier.

**Fix in Commit 1.5:** add a `daemon` bucket to `xtask/src/buckets.rs`, include it in `dev` and `full`.

### Finding #38 ✅ FIXED — daemon-mode helpers fail loudly when primary missing from pool

Implemented in Commit 1.5. Split the contract: `workspace_storage_anchor` and `workspace_db_file_path_for` stay lenient (path computation only — needed by `manage_workspace(add)` and refresh-routing tests that rebind before the pool catches up). Added `ensure_primary_pool_membership_for` guard to `get_database_for_workspace` and `get_search_index_for_workspace` — these actually open connections, so they enforce the daemon-mode invariant.

This is also a defense-in-depth backstop against the bug shape Findings #28/#29 flag: any rebind path that bypasses `attach_daemon_primary_binding_if_needed` now fails loudly the moment it tries to open the DB.

**Original description:**

**Where:** `src/tests/daemon/ipc_session.rs:679-683, 694-698`. Verified pre-existing by stashing my changes — fails identically on `4b2e1fbc`.

The test calls `handler.get_database_for_workspace(&workspace_b_id)` for a rebound primary that isn't in the workspace pool, and expects `db_err.to_string()` to contain `"not attached in the daemon workspace pool"`. The error returned doesn't contain that substring. Same for `get_search_index_for_workspace`.

This is the test reviewer D praised for verifying "helper-call failure when rebound workspace missing from pool" — an important regression guard. Failing silently because Finding #37 hides it.

**Fix in Commit 1.5:** trace `get_database_for_workspace` for non-pool-resident workspace IDs, restore the expected error contract.

---

## Pass 4 — Consolidated Punch List

### Overall verdict

The refactor is **sound and shippable** with a small set of fixes. Four independent Opus reviewers plus the lead's architectural pass found **zero correctness bugs** in the core rebind plumbing — every primary-path DB acquisition now correctly routes through `require_primary_binding()` and honors `primary_swap_in_progress`, which was the design goal. Tests are real TDD work (8/10 per reviewer D), not performative. Baseline `cargo xtask test dev` green, clippy clean.

The issues worth addressing before push cluster into three categories:
1. **Protocol-skew foot-gun** (Findings #1, #2): the adapter/daemon wire protocol change has no compatibility gate, so a plugin-binary upgrade while an older daemon owns the socket produces a silent `initialize request` failure loop. We hit this live during review setup.
2. **UX regression** (Finding #27): `list` / `remove` now hard-fail with a misleading "run index" error in deferred sessions where no primary is bound yet.
3. **Cheap hygiene wins** (Findings #17, #30): un-ignore the freshness-check happy-path test; add `#[serde(deny_unknown_fields)]` on editing tools.

Everything else is follow-up material.

### Punch list — release gate

#### 🔴 Must-fix before push

- **[#1 + #2]** **Add version gate to the IPC handshake.** Adapter already sends `VERSION:`; daemon parses it into `IpcHeaders.version` but never uses it. On session accept, if `adapter_version > daemon_version`, either (a) trigger stale-binary restart path so the fresh binary comes up, or (b) refuse with a clear log + error header that surfaces in the client as "daemon out of date, please restart". One focused change in `src/daemon/ipc_session.rs` + `src/daemon/mod.rs` accept loop. Reproducer documented under Finding #1.

#### 🟡 Strongly recommended before push (cheap, high-value)

- **[#17]** **Un-`#[ignore]` `test_fresh_index_no_reindex_needed`** (`src/tests/integration/stale_index_detection.rs:22`). Load-bearing happy-path of the freshness check; fix the mtime-resolution flake (seed mtimes explicitly) rather than shipping the FALSE branch untested.
- **[#27]** **Relax `list` / `remove` primary requirement.** Follow C's fix: make primary_id optional in `list_clean.rs` and `add_remove.rs` (remove branch); for `add`, keep hard fail but return actionable error text pointing at `manage_workspace(operation="open")` or client roots. Avoids misleading "run index" error in a deferred session.
- **[#30]** **Add `#[serde(deny_unknown_fields)]` to `EditFileTool` and `EditSymbolTool`.** Pre-existing footgun: `edit_file(workspace="secondary")` is silently ignored. One-line change; turns silent-ignore into loud error. True cross-workspace editing is a feature for later.

#### 🟡 Can-ship, track as follow-ups

- **[#4]** Log loudly on `RwLock` poison recovery in `session_workspace` and consider enum-ing the multi-step invariant.
- **[#9]** Stdio-mode `resolve_workspace_filter` should match daemon-mode's helpful "known but not active" error.
- **[#13]** `text_search_impl` missing-index branch: return `Err(...)` instead of silent `Ok(empty)`.
- **[#14]** `symbols/reference.rs` absolute-path branch propagates registry errors instead of swallowing.
- **[#18]** Delete or enable the two `workspace_init.rs` tests that don't exercise their SUT.
- **[#19]** Drop `std::mem::forget(temp_dir)` leaks in three rebind test helpers.
- **[#20]** Replace `format!("{:?}", result).contains(...)` with `extract_text_from_result` helper across rebind tests.
- **[#21]** Tighten `primary_workspace_bug.rs:99` or-gated assertion to require `"TestStruct"`.
- **[#22]** Add a comment on the intentional secondary-preserve-on-roots-shrink behavior (test name + `reconcile_primary_workspace_roots`).
- **[#28 + #29]** Either route `refresh`/`open` through the swap machinery or guard with `is_primary_workspace_swap_in_progress()`. Not hitting a race today, but it's exactly the bug shape the machinery exists to prevent.
- **[#31]** Rename `final_current_primary_id` to `stats_target_id`; split primary/reference branches in `index.rs`.
- **[#5]** Opportunistically split `handler.rs` (2,796 lines) as you next edit it — `handler/roots.rs`, `handler/primary_swap.rs`.

#### 🟢 Nice-to-have (no rush)

Findings #6, #7 (now ⚪), #8, #10, #11, #12, #15, #16, #23, #24, #25, #26, #32, #33, #34. Cleanup opportunities; none urgent.

### Suggested branch-close workflow

1. Land #1/#2 (protocol gate) as its own commit so CI covers it independently.
2. Land #27, #30, #17 in a "post-review hygiene" commit.
3. Decide whether to tackle #28/#29 on this branch or file a follow-up — they're my one residual "I'd sleep better if these were fixed now" items, but they're bounded race hazards not active bugs.
4. Run `cargo xtask test full` (not just `dev`) to cover the system + dogfood buckets that dev skips.
5. Open PR, request review on the review doc as part of the PR description.

### Review metadata

- Lead: architectural spine (Pass 1) + synthesis (Pass 4). Opus.
- Reviewer A (navigation tools): Opus. Writeup `/tmp/roots-review-A-navigation.md`.
- Reviewer B (search + symbols): Opus. Writeup `/tmp/roots-review-B-search-symbols.md`.
- Reviewer C (workspace + editing): Opus. Writeup `/tmp/roots-review-C-workspace-editing.md`.
- Reviewer D (test coverage audit): Opus. Writeup `/tmp/roots-review-D-tests.md`.

Total wall-clock: ~40 min including baseline test run. Scope: 14,916 insertions / 1,762 deletions across 72 files (commit `e004927e`). Baseline: `cargo xtask test dev` green, clippy clean on HEAD before and after review.

### Positive findings captured along the way

The refactor delivers a real correctness upgrade. Worth recording so future readers know what landed:

1. Every primary-workspace DB acquisition now routes through helpers that honor `primary_swap_in_progress` (Reviewer A). Previously, tools used `handler.get_workspace().await?` and never observed swap state.
2. `primary_workspace_snapshot` re-reads loaded workspace ID and compares roots before reusing handles (Reviewer B). Closes the swap-stale-handle hazard.
3. Metrics attribution correctly captures binding at tool-call entry (Reviewer C), so `record_tool_call` writes against the workspace the user had at call time, not at drain time.
4. `HealthChecker` rewritten to distinguish `FullyReady` / `SqliteOnly` / `NotReady`; the `primary_workspace_bug` false-positive is genuinely fixed.
5. Test suite is real TDD work — new rmcp duplex-stream harness, real `ListRootsRequest` responses, polled `session_count` reconciliation, error-branch coverage alongside happy paths (Reviewer D).
6. Health check now honestly distinguishes "no DB" vs "no Tantivy" (Reviewer C, Finding #36).
7. Stale-binary detection at session disconnect works as designed — verified live during review setup.
