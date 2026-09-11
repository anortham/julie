# Autonomous Execution Report - Deployment story, v8.0.0 (owner sequence item 4)

**Status:** Awaiting publication approval
**Plan:** docs/plans/2026-09-11-deployment-story-plan.md
**Branch:** deployment-story (worktree `.worktrees/deployment-story`, 9 commits over main `91c7968c`); plugin branch `v8-deployment` in `~/source/julie-plugin` (5 commits over the local `d741d4c`)
**PR:** not created (the owner merges, tags, and pushes; plan Global Constraints: "Never push, tag, or publish")
**Publication authority:** local commit=authorized (approved plan, 2026-09-11); push=prohibited (plan: "Never push, tag, or publish. The owner does that with the runbook in the finding."); PR=prohibited (same)
**Duration:** about 1 h 45 min across two sessions (Batch A dispatched 20:52Z; final docs commit 22:34Z)
**Phases:** 1/1 complete
**Tasks:** 8/8 complete, plus two defects fixed on the way (service instructions, shim `JULIE_WORKSPACE`) and one in the plugin (`findArchive`)
**External-model policy:** no external model received the diff (reviewer: none; the docs review used same-model subagents)

## What shipped
- Release archives carry `julie-semantic-sidecar` 0.1.0 beside `julie-server` for all four targets: `.github/scripts/pack-release.sh` with pinned sha256 values, called by `release.yml`.
- Version 8.0.0 in `Cargo.toml`, `Cargo.lock`, `docs/site/index.html`; `docs/release-notes/v8.0.0.md`.
- One routing text: `JULIE_AGENT_INSTRUCTIONS.md` (1,960 bytes) served on MCP `initialize` by the machine service (fixed: the service dropped it) and printed by the julie dev hook and the plugin session hook; `cargo xtask sync-plugin` mirrors it. Routing block, pretool hooks, and `.agents/` deleted.
- README install section: six harnesses plus Manual, no HTTP-URL registration, Node.js 22.5 prerequisite, one workspace-resolution paragraph.
- Plugin repo: root `plugin.json`, `.claude-plugin/plugin.json` with `mcpServers` and `hooks`, `.codex-plugin/plugin.json`, `.agents/plugins/marketplace.json`, `hooks/hooks.json` plus `hooks/session-start.cjs`, `JULIE_AGENT_INSTRUCTIONS.md`; `bin/install-codex.cjs` deleted; `run.cjs` picks the highest archive version.
- Six harness checks pass (Claude Code, Codex, Antigravity, OpenCode, Hermes, Cursor), each answering `:52 find_default_sidecar_binary function` through the plugin launcher.
- The stdio shim honors `JULIE_WORKSPACE` (`shim_workspace_root`), as the README promised.
- Finding with the owner runbook: `docs/findings/2026-09-11-deployment-story.md`. Ledger: `docs/plans/2026-09-11-deployment-story-ledger.md`.

## Judgment calls (non-blocking decisions made)
- `julie-plugin/mcp.json`, `mcp_config.json` (deleted) - Codex and Antigravity start a plugin's MCP servers inside the plugin directory with no MCP roots and no project path (probe plugin evidence in the finding), so the plugin-declared server indexed the plugin directory. The plugin now declares no server for them; both READMEs show the config entry that starts Julie in the project directory. The design doc assumed the harness starts servers in the project; the finding records the correction.
- `src/service/shim.rs:159` - `shim_workspace_root()` reuses `resolve_workspace_startup_hint(None)` instead of a new resolver, because the CLI already tested the `JULIE_WORKSPACE`-then-cwd order.
- `julie-plugin/hooks/run.cjs:56` - `findArchive` sorts matches with `localeCompare(numeric: true)` and takes the last, instead of matching the manifest version, because the manifests stay at 7.18.0 until the workflow bumps them.
- `julie-plugin/bin/install-opencode.cjs:128` - The printed `opencode.json` block no longer carries `environment.JULIE_WORKSPACE`; pasted into the global config it pinned every session to one project.
- `docs/findings/2026-09-11-deployment-story.md` runbook - Plugin `main` on GitHub is the workflow's orphan commit and `release.yml` dispatches the plugin workflow on it, so the runbook force-pushes `v8-deployment` to plugin `main` before the julie tag instead of merging after it.
- Codex check ran with `-c mcp_servers.julie.*` overrides from the worktree instead of editing the owner's `~/.codex/config.toml`; the override is the same entry the README shows.
- Antigravity's model turn needs a login that this machine had intermittently ("You are not logged into Antigravity" in the CLI log at 21:56Z); the later run completed in 33.6 s and passed.

External review: none (not requested for this run). A same-model adversarial review of the finding, READMEs, and runbook ran instead: 3 reviewers, 29 findings, 25 confirmed and fixed, 4 refuted.

## Review campaign
- **State:** clean
- **Evidence:** lead-only
- **Round:** 1/1
- **External invocations:** 0/0
- **Open critical/high:** 0
- **Open medium/low:** 0
- **Open at/above floor:** 0

## Tests
- `cargo xtask test full` at `18403740`: 5/5 commands in 41.7 s (dogfood 69 passed); `cargo clippy --workspace --all-targets`: 0 errors, pre-existing warnings only.
- `cargo xtask test dev` on the docs working tree (committed as `318fe4c7`): 2210 passed, 13 CLI tests passed, 15.5 s. The first run failed on a stray `/tmp/.julie` marker from a mistaken shim run; removed, rerun clean.
- Plugin `node --test hooks/*.test.cjs` at `1dec280`: 21 pass, 0 fail.
- Live: six harness checks; `JULIE_WORKSPACE` shim check from `/tmp`; Task 6 end-to-end through the packed archive.

## Blockers hit
- None. Push, tag, and release are the owner's, per the plan; the runbook is in the finding.

## Files changed
- julie: `git diff --stat 91c7968c..HEAD`: 46 files, 1,452 insertions, 1,493 deletions (net negative). Main groups: `.github/scripts/pack-release.sh` (new), `.github/workflows/release.yml`, `Cargo.toml`, `Cargo.lock`, `docs/site/index.html`, `docs/release-notes/v8.0.0.md` (new), `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/hooks/` (three files deleted, one changed), `.agents/` (deleted), `CLAUDE.md`, `AGENTS.md`, `README.md`, `src/service/mcp.rs`, `src/service/shim.rs`, `src/handler/mcp_adapter.rs`, `src/handler.rs`, `src/tests/service/mcp_http.rs`, `src/tests/service/shim.rs`, `src/tests/cli_tools_tests.rs`, `xtask/src/sync_plugin.rs`, `xtask/tests/docs_contract_tests.rs`, `docs/plans/` (design, plan, ledger), `docs/findings/2026-09-11-deployment-story.md`, `.memories/`.
- plugin: `git diff --stat d741d4c..HEAD`: 26 files, 625 insertions, 435 deletions.

## Source control
- **Outstanding:** None on the branches. julie worktree `.worktrees/deployment-story` at `318fe4c7`, clean (untracked `fixtures/databases/` symlink is the dogfood fixture link, not a change). Plugin `~/source/julie-plugin` at `1dec280`, clean except the untracked local archive `bin/archives/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz` (scratch for the checks; do not commit).
- **The user's:** main checkout `/home/murphy/source/julie` (`main` at `91c7968c`, 176 ahead of origin) has the owner's uncommitted `.codex/config.toml` and `.memories/2026-09-11/133457_b2d2.md`; not touched. Plugin local `main` at `d741d4c` is behind `origin/main` (`09a2c23`, the workflow's orphan commit); not touched.
- **Worktrees left in place:** `.worktrees/deployment-story` (the branch is not merged; disposition is the owner's).
- **Environment:** Codex, Hermes, Antigravity, Cursor configs restored byte for byte; OpenCode `opencode.json` deleted and skills unlinked; Codex and Antigravity plugin installs removed; junk workspaces from the plugin-directory runs removed from the service. The live service runs the branch's 8.0.0 release binary (pid 611965). The stray `tmp_e9671acd` checkout that item 2 deferred was refreshed by a mistaken run and still exists.

## Next steps
- Owner: run the runbook in `docs/findings/2026-09-11-deployment-story.md` (plugin `main` first, then merge and tag julie, then check the release and the plugin run, then a fresh install).
- Item 5 (code cleanup) is next in the brief.
- Deferred: a one-step Codex or Antigravity path (needs a harness signal that names the project); an Antigravity hook (root `hooks.json`); the `tmp_e9671acd` stray checkout.
