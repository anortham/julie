# ADR-0003: Prepared-once edit/rewrite invariant

## Context

Edit tools (`edit_file`, `rewrite_symbol`) report success metrics — applied/dry-run flag, diff, changed byte count, failure category — alongside the actual edit operation. Pre-cleanup, the metrics path and the apply path each independently resolved the target path, read the file, applied the edit, and computed the diff. The pattern was:

```rust
let metadata = self.success_metrics_metadata(handler).await?;  // reads file, applies, diffs
let result = self.call_tool(handler).await?;                   // reads file again, applies again, diffs again
```

This produced two real problems:

1. **Double work on the hot path.** Each tool call did two file reads, two parses (for `rewrite_symbol`), and two diff computations.
2. **A race window.** If the file changed on disk between the metrics computation and the apply, the reported metrics described one state and the persisted edit described another. The metrics could report `applied=true` for an edit that actually failed, because they were computed against a stale read.

The plan Candidate 9 deletion test was: "a request should have exactly one prepared edit/rewrite object per handler invocation."

## Decision

Both `edit_file` and `rewrite_symbol` follow a strict three-step contract:

1. **Prepare once.** `prepare_edit` / `prepare_rewrite` resolves the path, reads the file, applies the edit logic, computes the diff and the changed-byte count, and packages everything into a `PreparedEdit` / `PreparedRewrite` struct.
2. **Metrics from prepared.** `success_metrics_metadata_from_prepared(&prepared)` reads fields from the prepared struct. It does no file I/O, no parsing, no diff computation.
3. **Apply from prepared.** `call_prepared(prepared)` consumes the prepared struct and commits the edit via `EditingTransaction` (see ADR-0004). It does no file I/O until commit time.

The prepared struct is the single source of truth for `applied`, `diff`, `changed_bytes`, and the final commit content. Metrics and apply are guaranteed to agree because they read the same object.

Failure shaping: if `prepare_*` fails, the handler returns the error to `classify_tool_failure` (see ADR-0002) and no prepared object is constructed. If `prepare_*` succeeds but the commit fails, the metrics-merge layer forces `applied = false` over the prepared metadata.

Concretely:

- `src/tools/editing/edit_file.rs:58` — `PreparedEdit { resolved_path, original_content, modified_content, diff, changed_bytes, ... }`
- `src/tools/editing/edit_file.rs:488` — `prepare_edit`
- `src/tools/editing/edit_file.rs:529` — `success_metrics_metadata_from_prepared`
- `src/tools/editing/edit_file.rs:553` — `call_prepared` (with no-op short-circuit when `modified == original`)
- `src/tools/editing/rewrite_symbol.rs:141` — `PreparedRewrite { application, diff, changed_bytes, ... }`
- `src/tools/editing/rewrite_symbol.rs:596` — `prepare_rewrite`
- `src/tools/editing/rewrite_symbol.rs:842` — `success_metrics_metadata_from_prepared`
- `src/tools/editing/rewrite_symbol.rs:874` — `call_prepared`

## Consequences

**Easier**

- The metrics report and the persisted edit are by construction the same edit.
- File reads happen once per request. Parses (for `rewrite_symbol`) happen once.
- No-op edits (`modified == original`, e.g. `old_text == new_text`) short-circuit before touching the file system or the watcher.
- Race testing has a clear shape: a test changes the file between `prepare` and `commit`, and the commit detects the mismatch (see ADR-0004).

**Harder**

- Adding a new metric to the success metadata requires extending the prepared struct, not just adding a side computation. This is intentional — side computations are how the double-read drift started.
- The prepared struct carries the full `modified_content` until commit. For very large files this is more memory than the pre-cleanup "compute on demand" shape, but matches the cost of doing the work correctly.

## Applies To

- `src/tools/editing/edit_file.rs::PreparedEdit`
- `src/tools/editing/rewrite_symbol.rs::PreparedRewrite`
- `src/handler/tools/edit_file.rs` and `src/handler/tools/rewrite_symbol.rs` (handler call sites must call `prepare_*` once)
- Any future edit-shaped tool

## Future Agents

- When implementing a new edit-shaped tool, follow the prepare → metrics-from-prepared → apply-from-prepared shape. Do not interleave file I/O with metadata construction.
- Do not call `prepare_*` twice in the same handler invocation. If you find yourself wanting to "re-prepare," the right move is usually to extend the prepared struct with the field you need, not to re-derive it.
- Cache anything diff-shaped (e.g. `diff: String`, `changed_bytes: usize`) on the prepared struct itself. `PreparedRewrite` learned this lesson late; do not reintroduce the asymmetry where one consumer reads from the struct and another recomputes from `original_content + modified_content`.
- The `applied=false` invariant on prepare or apply failure is enforced by the handler's metadata merge. If you bypass the handler (e.g. in a direct `call_tool` path), preserve the same merge — the contract is "applied reflects what the user's filesystem will look like after this call."
- The no-op short-circuit in `call_prepared` (when `modified == original`) is part of the contract, not an optimization. It prevents the watcher from doing a pointless re-extraction. Preserve it.
