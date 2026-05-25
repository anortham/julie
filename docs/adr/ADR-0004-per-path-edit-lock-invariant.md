# ADR-0004: Per-path edit lock invariant

## Context

Multiple in-process writers can race on the same file path:

- `edit_file` and `rewrite_symbol` both call `EditingTransaction::commit_*`.
- `MultiFileTransaction` (used by `rename_symbol` and any future multi-file refactor) commits N files in a single logical operation.
- Refactoring callers historically held the original content and wrote via blind last-writer-wins.

Pre-cleanup, `EditingTransaction` was synchronized only by its own internal state, and `MultiFileTransaction` did not coordinate with it at all. The result: two in-process writers targeting the same file could interleave their temp-write + atomic-rename steps, with the second writer overwriting the first's commit while the first still held the metadata describing the "applied" state. The metrics for the first edit would report success against a file that no longer reflected its output.

`commit_if_unchanged` (the original-content guard) helped against external writers but did nothing about same-process races, because both writers in the race had read the same original.

## Scope

The lock invariant covers **user-source-file writes inside a workspace** — files an editor or tool action might modify, and that the watcher might index. It does **not** cover:

- daemon state (`src/daemon/pid.rs`, `src/daemon/lifecycle.rs`, `src/daemon/transport.rs`, `src/daemon/app/helpers.rs`)
- workspace config files (`src/workspace/mod.rs`, `src/tools/workspace/discovery.rs::.julieignore`)
- search/embedding markers (`src/search/index.rs::write_compat_marker`, `src/embeddings/sidecar_*.rs`)
- xtask reports and tool outputs

These have their own consistency models (single-writer, atomic-rename, idempotent retry) and are not edited by users or watched by the indexer.

## Decision

There is a single process-global registry, `EDIT_LOCKS`, at `src/tools/editing/mod.rs:75`:

```rust
static EDIT_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
```

Every in-process writer that commits to a **user source file** **must** acquire `edit_lock_for(&path)` before the existence/permission check and hold it across the temp-write + atomic-rename. Path keys go through `normalized_edit_lock_path(file_path)` so logically-equivalent paths (case, separators, symlink targets) hash to the same lock.

The two writers that satisfy this:

- `EditingTransaction::commit_inner` (`src/tools/editing/mod.rs:143`) — acquires `edit_lock_for(&self.file_path)` once.
- `MultiFileTransaction::commit_all` (`src/tools/editing/mod.rs:327`) — collects normalized lock paths for all targets, **sorts and dedups them**, then holds all guards across Phase 1 (temp write) and Phase 2 (atomic rename).

Sorting before lock acquisition is load-bearing: it prevents deadlock between two `MultiFileTransaction` instances that share a subset of paths in different orders.

`commit_if_unchanged` is now a true guard against both same-process and external writers, because every same-process writer holds the lock before re-reading.

## Consequences

**Easier**

- Two edit-shaped operations on the same file serialize cleanly. The second one re-reads inside the lock and sees the first's committed content.
- A multi-file refactor can read all original contents, do its edits in memory, and commit atomically without fearing that an `edit_file` slipped in between.
- The metrics-vs-commit contract in ADR-0003 holds even under concurrent writers, because the `applied=false` re-check at commit happens while the lock is held.

**Harder**

- Any new path-based writer must register with `EDIT_LOCKS`. A writer that bypasses the registry is invisible to existing serialization. The compile-time signal that you forgot is weak — there is no type-level "lock held" proof.
- Holding many locks across a multi-file commit slightly increases the worst-case latency for an unrelated single-file edit on one of those paths. This is a small cost for correctness.
- The `EDIT_LOCKS` map grows monotonically (one entry per ever-touched path). For Julie's session lifetime this is unbounded but very small.

## Applies To

- `src/tools/editing/mod.rs::{EDIT_LOCKS, edit_lock_for, normalized_edit_lock_path}`
- `src/tools/editing/mod.rs::EditingTransaction`
- `src/tools/editing/mod.rs::MultiFileTransaction`
- `src/tools/refactoring/rename.rs` (and any future multi-file refactor)
- Any new in-process writer that commits to a file path

## Future Agents

- Before writing a file in a new tool, ask: "is this a user source file inside a workspace? Could another tool in the same process also write this path?" If yes to both, acquire `edit_lock_for(&path)` via the existing `EditingTransaction` or `MultiFileTransaction` API. Do not write directly with `std::fs::write` or `tempfile::rename_*` from a tool implementation that targets user source.
- For non-source-file writes (daemon state, config, markers, reports), use direct `std::fs::write` with the appropriate consistency model (typically atomic-rename via `tempfile::persist`). These are out of scope for the lock registry.
- Never acquire multiple `EDIT_LOCKS` entries in an unsorted order. Always collect, normalize via `normalized_edit_lock_path`, sort, dedup, then acquire. `MultiFileTransaction::commit_all` is the reference pattern.
- Do not weaken `commit_if_unchanged` for performance. The original-content re-check inside the lock is what closes the race against external writers (editors, watchers, sidecar processes). External-writer races still exist; this guard is what makes them detectable.
- If you add a fundamentally new file-write path (e.g. an LSP-style code action that writes from outside the editing module), wire it through `EditingTransaction` rather than building a parallel locking scheme. One registry is the contract.
