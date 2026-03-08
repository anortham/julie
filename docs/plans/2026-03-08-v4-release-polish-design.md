# Design: v4.0 Release Polish

**Date:** 2026-03-08
**Status:** Approved
**Path:** Lightweight (6 small fixes, same-session)

## Problem

Core v4.0 features are all working (daemon, connect, OpenAPI, dashboard, plugin, worktree validation) but user-facing polish is missing: version still says 3.9.1, docs are stale, port numbers are inconsistent, and root URL returns 404.

## Fixes

### 1. Version bump: 3.9.1 → 4.0.0

- **File:** `Cargo.toml` line 3 — change `version = "3.9.1"` to `version = "4.0.0"`

### 2. Port consistency: standardize on 7890

The CLI default is 7890 (in `src/cli.rs`). But `julie-plugin/` and `mcp.json.example` reference 3141.
Standardize everything to 7890.

- **Files to fix (3141 → 7890):**
  - `julie-plugin/.claude-plugin/plugin.json` (line 22)
  - `julie-plugin/README.md` (lines 65, 101, 126, 143, 206)
  - `mcp.json.example` (line 21)
  - `docs/plans/2026-03-08-v4-release-design.md` (line 44, 201) — design doc reference
  - `docs/plans/2026-03-08-v4-release-impl.md` (line 23) — impl doc reference

### 3. Root redirect: `/` → `/ui/`

- **File:** `src/server.rs` — add a redirect route for `/` that sends 302 to `/ui/`
- One-liner: `.route("/", axum::routing::get(|| async { axum::response::Redirect::temporary("/ui/") }))`
- Add after the existing `/ui/` routes

### 4. Update CLAUDE.md

- **File:** `CLAUDE.md` — update version reference from 3.9.0 to 4.0.0 (last line, "Last Updated" section)
- Brief mention of daemon mode, connect command, and dashboard in the "Key Project Facts" or "Architecture" section
- Keep changes minimal — CLAUDE.md is already comprehensive

### 5. Clean up TODO.md

- **File:** `TODO.md` — mark completed post-platform items as done, update status of deferred items
- Items to mark done: 1 (skills), 5 (auto-registration), 8 (filewatcher), 9 (API docs), 10 (worktree validation), 13 (token optimization)
- Items already marked done at top of file are fine
- Move remaining deferred items under a "4.1 Backlog" heading

### 6. Complete stale Julie plan

- Use `mcp__julie__plan` tool to complete the "Sidecar Binary Distribution" plan (it's marked completed but still active)

## Acceptance Criteria

- [ ] `Cargo.toml` version is `4.0.0`
- [ ] All references to port 3141 changed to 7890 in plugin, mcp.json.example, and design docs
- [ ] `GET /` returns 302 redirect to `/ui/`
- [ ] CLAUDE.md references v4.0.0 and mentions daemon/connect/dashboard
- [ ] TODO.md post-platform items have correct done/deferred status
- [ ] Active Julie plan completed (no stale plan in recall)
- [ ] `cargo test --lib -- --skip search_quality` passes (fast tier)
- [ ] All changes committed
