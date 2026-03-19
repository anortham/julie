# Fix manage_workspace for Reference Workspaces

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `manage_workspace index` silently returning primary workspace stats for reference workspaces, and document `force` parameter for `refresh` operation.

**Architecture:** Three targeted fixes — one code bug fix in the `is_indexed` guard, one tool schema update, one agent instructions expansion.

**Tech Stack:** Rust, MCP tool schema (serde/JsonSchema), Markdown docs

---

## Background

### Bug: `index` on reference workspaces returns primary's stats
When `manage_workspace(operation="index", path="/path/to/ref")` is called and the primary workspace is already indexed, the `is_indexed` guard at `index.rs:147` checks the primary workspace's database, finds symbols, and returns "Workspace already indexed: {primary_symbol_count} symbols". The reference workspace is never touched. This is a silent failure — the agent sees a plausible success message with the wrong workspace's data.

### UX issue: `refresh` doesn't advertise `force` parameter
The tool schema says `force` is "used by: index" — agents don't know they can pass `force=true` to `refresh`. The refresh example in the schema omits `force`. When an agent needs to re-index a reference workspace after extractor code changes (source files unchanged), they get "Already up-to-date" and have no obvious path forward.

### Doc gap: Agent instructions don't cover reference workspace operations
The `manage_workspace` section in `JULIE_AGENT_INSTRUCTIONS.md` is 2 lines — no mention of `add`, `refresh`, reference workspaces, or when to use `force`.

---

## Task 1: Fix `is_indexed` guard for reference workspaces

**Files:**
- Modify: `src/tools/workspace/commands/index.rs:147` — add `&& !is_reference_workspace` to guard condition

- [ ] **Step 1: Fix the guard**

In `index.rs`, change line 147 from:
```rust
if !force_reindex {
```
to:
```rust
if !force_reindex && !is_reference_workspace {
```

This skips the primary's `is_indexed` check when the target is a reference workspace, allowing `index_workspace_files` to run with incremental filtering (same as `refresh`).

- [ ] **Step 2: Run targeted test**

```bash
cargo test --lib tests::tools::workspace::isolation 2>&1 | tail -20
```

- [ ] **Step 3: Run dev tier**

```bash
cargo xtask test dev 2>&1 | tail -20
```

---

## Task 2: Update tool schema description

**Files:**
- Modify: `src/tools/workspace/commands/mod.rs:61-92` — update descriptions and examples

- [ ] **Step 1: Update `force` field description**

Change line 78 from:
```rust
/// Force complete re-indexing (used by: index)
```
to:
```rust
/// Force complete re-indexing (used by: index, refresh)
```

- [ ] **Step 2: Update refresh example**

Change line 69 from:
```rust
/// Refresh workspace:    {"operation": "refresh", "workspace_id": "workspace-id"}
```
to:
```rust
/// Refresh workspace:    {"operation": "refresh", "workspace_id": "workspace-id", "force": true}
```

- [ ] **Step 3: Verify build**

```bash
cargo build 2>&1 | tail -5
```

---

## Task 3: Expand agent instructions for manage_workspace

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md:123-125` — expand manage_workspace section

- [ ] **Step 1: Replace manage_workspace section**

Replace the 2-line section with comprehensive reference workspace documentation covering:
- `index` for primary workspaces
- `add` + `refresh` for reference workspaces
- When to use `force=true` (extractor/indexing code changed, source files unchanged)
- `health` for diagnostics

- [ ] **Step 2: Commit all changes**

```bash
git add src/tools/workspace/commands/index.rs src/tools/workspace/commands/mod.rs JULIE_AGENT_INSTRUCTIONS.md
git commit -m "fix(workspace): index on reference workspaces no longer returns primary stats"
```
