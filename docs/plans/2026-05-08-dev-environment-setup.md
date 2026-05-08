# Julie Development Environment Setup Plan

**Date:** 2026-05-08
**Goal:** Configure Claude Code AND Codex on the dev machine so Julie's plugin (skills, hooks) is globally active while the MCP server runs the locally-built dev binary. Maintain dogfood discipline: every change is exercised through the same plugin layer that ships to users.

**Revision history:**
- v1 (2026-05-08): initial draft
- v2 (2026-05-08): rewrite after codex-cli adversarial review caught 4 high-severity errors. Detailed corrections at bottom of this file.

---

## Current state (verified)

| Component | State | Verified by |
|---|---|---|
| Julie MCP (Claude Code) | User-scope, points at `~/source/julie/target/release/julie-server` | `claude mcp list` |
| Julie MCP (Codex) | **IS registered.** Project-scope per-project via `<project>/.codex/config.toml` with `env.JULIE_WORKSPACE` set. Same pattern for goldfish. | `codex mcp get julie`, `cat ~/source/julie/.codex/config.toml` |
| julie-plugin install | NOT installed in marketplaces or cache | `ls ~/.claude/plugins/` |
| Plugin version drift | Plugin manifests at v7.5.5, `Cargo.toml` at v7.8.1 — plugin is 3 minor versions stale | `cat .claude-plugin/plugin.json`, `cat Cargo.toml` |
| Plugin skill coverage | Plugin ships `editing`, `explore-area`, `impact-analysis`, `web-research` (and a few more). Missing `dead-code-audit` and `search-debug` that exist in `~/source/julie/.claude/skills/` | `ls ~/source/julie-plugin/skills/` vs `ls ~/source/julie/.claude/skills/` |
| Julie hooks (Claude Code) | Project-local at `~/source/julie/.claude/hooks/`. Fire only when cwd is in Julie repo. | `cat .claude/hooks/hooks.json` |
| Julie skills/hooks/AGENTS.md (Codex) | Not deployed. `~/.codex/skills/` has `.system` and `codex-primary-runtime` only. `~/.codex/AGENTS.md` has context-mode rules but no Julie section. `~/.codex/hooks.json` has only context-mode entries. | `ls ~/.codex/skills/`, `cat ~/.codex/AGENTS.md`, `cat ~/.codex/hooks.json` |
| context-mode coverage | Fully wired in both harnesses, all hook types | already verified |

### Why Codex MCP uses project-scope, not user-scope

Codex Desktop does **not** honor cwd correctly when invoking MCP servers. Even after Julie added MCP `roots` support (the standard mechanism for telling MCP servers about the active workspace), Codex Desktop still doesn't pass the right path. The workaround is per-project `.codex/config.toml` with explicit `env.JULIE_WORKSPACE = "/path/to/this/project"`. Codex CLI works fine with cwd alone, but the project-scoped config is harmless there and necessary for desktop, so the same shape is used for both.

This means: when adding Julie to a new project for Codex use, **the project gets its own `.codex/config.toml`**. There is no global Codex-side MCP setup for Julie; each project does its own registration.

---

## Constraints

1. **Dev binary is sacred.** MCP for `julie` must point at `~/source/julie/target/release/julie-server` (or debug binary during iteration). Plugin's bundled binary path must lose.
2. **Plugin contents must be live-editable.** Iterating on skills/hooks should not require `npm publish` or marketplace push. Use `claude --plugin-dir` for the dev loop (verified flag in `claude --help`).
3. **Existing context-mode hooks must keep working.** All edits to `~/.codex/hooks.json` are merges, not overwrites.
4. **Codex Desktop compatibility.** Anything we write to `~/.codex/` must work when Codex is invoked via desktop, not just CLI.
5. **PreToolUse hook output format is structured, not freeform.** Raw `console.log("hint")` does not reach the model on PreToolUse. Hooks must emit `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "..."}}` for context injection, or `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "..."}}` for hard blocks. **Verify against current Claude Code by writing a probe hook before relying on this** — schema field names may have evolved.
6. **PreToolUse on Codex cannot inject context, only deny.** Per context-mode README, Codex's PreToolUse `additionalContext` is unsupported pending `openai/codex#18491`. Context injection on Codex must use SessionStart and PostToolUse.

---

## Phase 0 — Pre-install plugin work

The current `~/source/julie-plugin` is stale and missing pieces. These need to be filled before we install, or the install yields incomplete coverage.

### 0.0 Sync plugin to current source (do this FIRST)

Codex review caught: don't add new hooks on top of a stale base.

1. Bump `~/source/julie-plugin/package.json`, `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json` to match `~/source/julie/Cargo.toml` (currently 7.8.1 — verify before bumping).
2. Copy missing skills from source to plugin: `dead-code-audit`, `search-debug` (and any others missing). Source: `ls ~/source/julie/.claude/skills/` vs `ls ~/source/julie-plugin/skills/`.
3. The skill list in CLAUDE.md's plugin distribution section names the canonical set — reconcile.
4. Verify: `diff -r ~/source/julie/.claude/skills/ ~/source/julie-plugin/skills/` is clean (modulo the one-way exclusion of in-progress experiments).

### 0.1 Hook scripts not yet in the plugin

| Script | Currently at | Needs to ship in plugin |
|---|---|---|
| `pretool-broad-tests.cjs` (Bash test-blocker, hard deny via `permissionDecision`) | `~/source/julie/.claude/hooks/` | Copy to `~/source/julie-plugin/hooks/` |
| `pretool-grep-redirect.cjs` (NEW: Bash grep/find/rg → fast_search nudge if path indexed) | Doesn't exist | Write new |
| `pretool-read-large.cjs` (NEW: Read of large code file → get_symbols nudge) | Doesn't exist | Write new |
| `codex-pretooluse.cjs` (Codex Bash deny rules — PreToolUse on Codex can deny but not nudge) | Doesn't exist | Write new |
| `codex-sessionstart.cjs` (Codex precedence rule injection — main enforcement on Codex) | Doesn't exist | Write new |
| `codex-posttooluse.cjs` (Codex after-the-fact coaching log) | Doesn't exist | Write new |

**Existing hooks need conversion too.** `pretool-edit.cjs` and `pretool-agent.cjs` currently emit raw `console.log()` and exit 0, which on PreToolUse does NOT reach the model (codex review pass 2 flagged this). Convert both to use the `format.cjs` helpers from Phase 0.2. Without this, the Edit and Agent nudges that the plugin already advertises don't actually work.

### 0.2 Hook output contract (Claude Code)

Before writing any new hook, write a probe hook that logs its stdin and various output shapes to `/tmp/julie-hook-probe.log`, register it for `PreToolUse:Bash`, run a Bash call, and confirm what Claude actually surfaces back to the model. Don't assume. Specifically test:

- Plain `console.log("hint")` exit 0 → does the model see it? (codex review claim: no)
- JSON `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "hint"}}` exit 0 → does it appear as system reminder?
- JSON with `permissionDecision: "deny"` and `permissionDecisionReason: "..."` → does it block?
- Exit 2 with message on stderr → does the model see it?

Once probed, document the working shape in `~/source/julie-plugin/hooks/lib/format.cjs` and have all hooks call helpers like `emitNudge(text)` and `emitDeny(reason)`.

### 0.3 Indexed-workspace detection (corrected)

Codex review caught: `.julie/` ancestry alone is wrong because daemon mode stores indexes globally at `~/.julie/indexes/`, and the project itself may have no `.julie/` directory.

Correct detector — accept either signal as positive:

1. **Daemon mode**: query `~/.julie/daemon.db` for known workspace roots. SQLite read-only open, single SELECT. Match if any registered `root_path` is an ancestor of the target path.
2. **Stdio mode fallback**: ancestry walk looking for `.julie/` directory.

```js
// hooks/lib/workspace-detect.cjs
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { DatabaseSync } = require("node:sqlite"); // Node 22.5+ stable

const DAEMON_DB = path.join(os.homedir(), ".julie", "daemon.db");

function findIndexedWorkspace(startPath) {
  const abs = path.resolve(startPath);

  // Try daemon registry first (covers the common case in daemon mode).
  // Verified schema: workspaces.path TEXT NOT NULL UNIQUE.
  try {
    if (fs.existsSync(DAEMON_DB)) {
      const db = new DatabaseSync(DAEMON_DB, { readOnly: true });
      const rows = db.prepare("SELECT path FROM workspaces WHERE path IS NOT NULL").all();
      db.close();
      for (const { path: root } of rows) {
        if (root && (abs === root || abs.startsWith(root + path.sep))) {
          return root;
        }
      }
    }
  } catch { /* daemon db missing, schema mismatch, or Node too old — fall through */ }

  // Stdio-mode fallback: walk for .julie/ directory.
  let cur = abs;
  while (true) {
    if (fs.existsSync(path.join(cur, ".julie"))) return cur;
    const parent = path.dirname(cur);
    if (parent === cur) return null;
    cur = parent;
  }
}

module.exports = { findIndexedWorkspace };
```

**Verified facts (codex review pass 2):**
- Schema column is `path`, not `root_path`. Confirmed via `sqlite3 ~/.julie/daemon.db '.schema workspaces'` and live SELECT.
- Plugin has zero dependencies (`~/source/julie-plugin/package.json`, no `node_modules`). Avoids `better-sqlite3` to skip native-build complexity. `node:sqlite` is built-in since Node 22.5 stable.
- Add `"engines": { "node": ">=22.5.0" }` to plugin's `package.json` so the requirement is explicit.

Hooks early-return (no-op) when this returns null. **Overhead is low (one SQLite open + select per matching tool call), not zero.** Measure before claiming numbers.

### 0.4 Codex install script

`~/source/julie-plugin/bin/install-codex.cjs` — idempotent script that:
1. Symlinks `~/source/julie-plugin/skills/<each>` to `~/.codex/skills/julie-<each>/`. Codex skill discovery layout to be confirmed (SKILL.md inside subdirectory? flat? — verify against an existing `~/.codex/skills/` entry).
2. Merges Julie hook entries into `~/.codex/hooks.json` between sentinel comments (`<!-- julie hooks start --> ... <!-- end -->` style if JSON allows; else use a state file tracking which entries Julie added so reruns can replace them safely).
3. Appends Julie precedence section to `~/.codex/AGENTS.md` between sentinel HTML comments so reruns are idempotent.

**Does NOT** modify `[mcp_servers.julie]` in any config.toml. MCP registration is per-project and already handled by the user's existing project configs.

Idempotency is non-negotiable: this script will run on every plugin upgrade.

### 0.5 Plugin-declared MCP server

Plugin's `.claude-plugin/plugin.json` declares `mcpServers.julie`. For users with their own user-scope `julie` MCP registration, Claude Code's documented scope precedence (local > project > user > plugin > connectors — source: https://code.claude.com/docs/en/mcp) means user wins. **No action needed**; this is no longer an open question.

For Codex: the plugin's MCP entry is irrelevant because Codex doesn't read Claude plugin manifests. Codex MCP setup remains the existing per-project `.codex/config.toml` pattern.

---

## Phase 1 — Claude Code setup

Pre-req: Phase 0 complete.

Two distinct workflows. Don't conflate them.

### 1A — Dev iteration loop (the one you'll use daily)

Use `claude --plugin-dir` per session. This loads the plugin from a directory without going through marketplace install. Live edits propagate by re-running `/reload-plugins` inside the session.

```bash
claude --plugin-dir ~/source/julie-plugin
# Inside session, after editing skills or hooks in the plugin repo:
/reload-plugins
```

This is the documented Claude Code plugin development primitive. Per `claude --help`: "Load a plugin from a directory or .zip for this session only (repeatable: --plugin-dir A --plugin-dir B.zip)".

Pros: live iteration, no install needed, no cache concerns.
Cons: only active for that session; you have to remember the flag. Could alias it: `alias claudej='claude --plugin-dir ~/source/julie-plugin'`.

### 1B — Installed dogfood (verifying the install itself works)

Run this when you want to verify the marketplace install path itself is healthy, not for daily iteration.

```bash
claude  # Inside Claude Code:
  /plugin marketplace add ~/source/julie-plugin
  /plugin install julie@julie-plugin
```

Note the syntax: `<plugin-name>@<marketplace-name>`, not the inverse. Verified against `~/source/julie-plugin/.claude-plugin/marketplace.json`:
```json
{ "name": "julie-plugin",
  "plugins": [{ "name": "julie", "source": "./", "version": "7.5.5", ... }] }
```

So the marketplace is `julie-plugin` and the plugin is `julie`. `/plugin install julie@julie-plugin` installs it.

After install, verify:
- `claude mcp list` still shows `julie: /Users/murphy/source/julie/target/release/julie-server` (single entry, dev path). User-scope wins over plugin-declared by precedence; if you see a duplicate, the precedence isn't behaving and Phase 0.5 needs revisiting.
- `claude /plugin list` shows `julie@julie-plugin` installed.
- Skills appear with namespace: `julie:editing`, `julie:explore-area`, etc. **Not** bare `editing`. Per Claude Code plugin docs, plugin skills are namespaced under the plugin name.

### 1.3 Verify hook coverage outside the Julie repo

In any non-Julie project (e.g., `cd ~/source/some-other-project`):
- Edit any file → Edit hook should nudge toward `edit_file` (assuming JSON output format from Phase 0.2 is correct).
- Run `grep "foo" .` via Bash on a path with no daemon-indexed workspace and no `.julie/` ancestor → grep-redirect hook should no-op.
- Run `grep "foo" .` via Bash inside `~/source/julie` (which is daemon-indexed) → should suggest `fast_search`.
- Read of a >300-line code file in indexed workspace → should suggest `get_symbols` first.
- Run broad `cargo test` → blocker fires (`permissionDecision: deny` style).

---

## Phase 2 — Codex setup

Pre-req: Phase 0.4 (Codex install script) complete.

### 2.1 MCP server: nothing to do

Already registered project-scope per project via `<project>/.codex/config.toml` with `env.JULIE_WORKSPACE`. This is the right shape for both Codex CLI and Codex Desktop. **Skip.** Verify with `codex mcp get julie`, not config.toml grep.

When adding Julie to a new project: the user adds a `[mcp_servers.julie]` block to that project's `.codex/config.toml` themselves. The plugin install script does not do this — it can't know which projects the user wants Julie in.

### 2.2 Hook merge into `~/.codex/hooks.json`

Run `node ~/source/julie-plugin/bin/install-codex.cjs`. Resulting file should look like:

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "...context-mode regex...", "hooks": [{ "command": "context-mode hook codex pretooluse" }] },
      { "matcher": "local_shell|shell|exec_command|Bash", "hooks": [{ "type": "command", "command": "node /Users/murphy/source/julie-plugin/hooks/codex-pretooluse.cjs" }] }
    ],
    "SessionStart": [
      { "hooks": [{ "command": "context-mode hook codex sessionstart" }] },
      { "hooks": [{ "type": "command", "command": "node /Users/murphy/source/julie-plugin/hooks/codex-sessionstart.cjs" }] }
    ],
    "PostToolUse": [
      { "hooks": [{ "command": "context-mode hook codex posttooluse" }] },
      { "hooks": [{ "type": "command", "command": "node /Users/murphy/source/julie-plugin/hooks/codex-posttooluse.cjs" }] }
    ],
    "UserPromptSubmit": [...keep context-mode only...],
    "Stop": [...keep context-mode only...]
  }
}
```

Julie skips UserPromptSubmit and Stop — no current use case.

### 2.3 Codex hook strategy (different from Claude Code)

Codex PreToolUse can deny but cannot inject `additionalContext`. So the enforcement story differs:

| What we want | Mechanism on Codex |
|---|---|
| Hard block (e.g., `cargo nextest run --lib` no filter) | PreToolUse deny |
| "Use fast_search" precedence | SessionStart injects rules at session start; PostToolUse logs after-the-fact (model sees it next turn) |
| Edit routing | PostToolUse logs "Edit on indexed code → next time use edit_file"; SessionStart precedence reinforces |

**Verify Codex hook stdin shapes before writing scripts.** Codex passes input as JSON over stdin, and the field names for `tool_name`, `tool_input`, `cwd`, `session_id`, etc. may differ from Claude Code. Write a probe hook that logs everything to `/tmp/julie-codex-probe.log` and run it against:
- A `Bash` / `exec_command` call
- An `apply_patch` call
- An MCP tool call (e.g., `mcp__julie__fast_search`)
- A subagent invocation (if Codex spawns subagents)

Don't infer payload shapes from "context-mode is wired" — context-mode handles a different set of concerns and may not exercise the same fields.

### 2.4 Skill deployment

`install-codex.cjs` symlinks `~/source/julie-plugin/skills/<name>/` to `~/.codex/skills/julie-<name>/` (or whatever Codex's expected layout is — verify by inspecting `~/.codex/skills/.system/` and `~/.codex/skills/codex-primary-runtime/` to see what shape Codex expects).

### 2.5 AGENTS.md precedence section

`install-codex.cjs` appends, between sentinel HTML comments:

```markdown
<!-- julie-precedence start -->
## Julie tool precedence

When working in a Julie-indexed workspace (registered in daemon, or with `.julie/` directory):
- Symbol/ref/def queries → `mcp__julie__fast_search`, `mcp__julie__deep_dive`, `mcp__julie__fast_refs`
- File structure → `mcp__julie__get_symbols` before any Read of code files
- Edits to indexed code → `mcp__julie__edit_file` / `rewrite_symbol` / `rename_symbol`
- Refactor scope → `mcp__julie__blast_radius`

context-mode handles non-indexed text/web/data. Wrapping `grep src/` in `ctx_batch_execute` on an indexed repo is the wrong tool.
<!-- julie-precedence end -->
```

This section matters more on Codex than Claude Code: Codex PreToolUse cannot inject context, so AGENTS.md and SessionStart are the primary enforcement surfaces.

### 2.6 Verify

- `codex mcp get julie` confirms registered.
- Start Codex session in `~/source/julie`, list MCP tools → julie tools listed.
- Run a grep on indexed code via Bash → next-turn context should reflect PostToolUse coaching.
- `~/.codex/AGENTS.md` contains the Julie precedence block.

---

## Phase 3 — Development workflow (dogfooding)

### 3.1 Iteration loop: Julie binary changes (most common)
1. Edit Rust source in `~/source/julie/src/`.
2. `cargo build --release` — produces new binary at `target/release/julie-server`.
   - **Windows note:** if a Claude Code or Codex session is active, the running `julie-server.exe` holds a file lock; rebuild fails with "Access is denied". Exit the MCP client first.
3. Restart Claude Code OR Codex (whichever you're using) — MCP server reconnects, picks up new binary.
4. Hooks and skills are unchanged (they live in plugin, not in binary).

### 3.2 Iteration loop: skill or hook changes (less common)
1. Edit `~/source/julie/.claude/skills/<name>/SKILL.md` or `~/source/julie/.claude/hooks/<name>.cjs` (project-local source, the canonical copy).
2. Sync to `~/source/julie-plugin/`.
3. **Claude Code dev case** (Phase 1A): inside session, run `/reload-plugins`. Live.
4. **Claude Code installed case** (Phase 1B): re-run `/plugin install julie@julie-plugin`. Or restart session.
5. **Codex case:** rerun `node ~/source/julie-plugin/bin/install-codex.cjs`. Symlinks already point at source so changes propagate without re-running for skill edits, but hook script edits to `.cjs` files apply on next hook firing automatically.
6. Restart session as needed.

### 3.3 Iteration loop: plugin manifest changes
1. Edit `~/source/julie-plugin/.claude-plugin/plugin.json` or `hooks/hooks.json`.
2. **Dev mode (1A):** `/reload-plugins`.
3. **Installed mode (1B):** `/plugin install julie@julie-plugin`.
4. Restart session.

### 3.4 Sync rules between source repo and plugin repo (IMPLEMENTED)

`cargo xtask sync-plugin` (added in `xtask/src/sync_plugin.rs`):
- **Skills:** full mirror source → plugin. Updates changed files, adds new files, removes plugin-only skill files. Idempotent. `--dry-run` previews. `--plugin-root <path>` overrides default sibling lookup.
- **Hooks:** report-only. Plugin's `hooks.json` uses `${CLAUDE_PLUGIN_ROOT}` and a plugin-only `lib/` (require paths) that don't apply in source dev mode, so auto-syncing plugin → source would break the source dev environment. The xtask reports divergence (`≠`, source-only, plugin-only) so the user can manually reconcile when intended.

Sync direction for skills is julie repo → julie-plugin repo. `julie-plugin` stays a pure distribution artifact for that surface. Hooks remain "intentionally separate" per the CLAUDE.md plugin distribution section.

### 3.5 Why this is dogfooding
- Every Julie change goes through `cargo build --release` → MCP reconnect → tools we're using right now run the new binary.
- Every plugin change goes through `--plugin-dir` reload → hooks/skills we're using right now use the new content.
- Bugs surface in our own session before user reports.

---

## Phase 4 — Verification battery

After all phases applied, run these. Document pass/fail per `docs/plans/verification-ledger-template.md`.

| # | Check | Pass criterion |
|---|---|---|
| 1 | `claude mcp list` (after plugin install in 1B) | `julie: /Users/murphy/source/julie/target/release/julie-server  - ✓ Connected` (single entry, dev path) |
| 2 | `claude plugin list` (CLI subcommand) OR `/plugin` interactive UI in session | `julie@julie-plugin` shown installed, scope: user |
| 3 | Hook output probe (Phase 0.2) | Document which output shape reaches the model on PreToolUse |
| 4 | In a non-Julie project, ask Claude to find a symbol → uses `julie:fast_search` | Tool call appears in transcript |
| 5 | In `~/source/julie`, run `grep "foo" src/` via Bash | Hook nudges to `fast_search` (or denies and forces use, depending on enforcement choice) |
| 6 | Run `cargo nextest run --lib` (no filter) | PreToolUse blocks |
| 7 | Edit a code file in indexed workspace | Hook nudges to `edit_file` |
| 8 | `codex mcp get julie` | Returns enabled, dev binary path, JULIE_WORKSPACE env |
| 9 | Codex: start session in `~/source/julie`, list tools | `julie` MCP tools listed |
| 10 | Codex: run a grep on indexed code | PostToolUse coaching event in `~/.codex/sessions/...` |
| 11 | Edit Rust source, `cargo build --release`, restart Claude Code | New binary serves; verify by calling a tool with a behavior you just changed |
| 12 | Edit `~/source/julie-plugin/skills/editing/SKILL.md`, run `/reload-plugins` | New skill content visible without full restart |
| 13 | Workspace detection helper unit test | Returns daemon root for a daemon-indexed path; falls back to `.julie/` ancestry; returns null outside any workspace |

---

## Open questions / risks

Reduced from v1 after codex review.

1. **Hook output format for PreToolUse on Claude Code.** Documented expectation per codex review: `hookSpecificOutput.additionalContext` for nudges, `permissionDecision: "deny"` for blocks. Verify with probe hook (Phase 0.2) before shipping new hooks.
2. **Codex hook stdin payload shape.** Field names for tool name, input, cwd may differ from Claude Code. Probe before writing.
3. **Daemon DB schema.** Workspace root column name needs verification against `src/daemon/database.rs`.
4. **`pretool-grep-redirect.cjs` false positives.** If user runs `grep` for a non-code file (e.g., a CSV) in an indexed workspace, the nudge is wrong. Heuristic: only nudge if the grep target ext matches indexed code extensions for that workspace's languages. Or only nudge once per session per target.
5. **Codex skill discovery layout.** Verify directory structure expected by Codex (e.g., `~/.codex/skills/<name>/SKILL.md`?) by inspecting existing entries before shipping the install script.
6. **AGENTS.md merge sentinel collision.** If multiple plugins use the same sentinel-comment trick, last-writer-wins. Use a Julie-specific sentinel (`<!-- julie-precedence start -->`).
7. **Hook overhead under load.** Node spawn + SQLite open per matching PreToolUse. Cumulative overhead in a busy session worth measuring before declaring it acceptable. Cache the workspace lookup per-session if hot.
8. **Windows path handling.** All hook scripts use `path.resolve` and forward-slash output; daemon DB path uses `os.homedir()`. Verify on Windows before claiming parity.

---

## Recommended sequence

1. **Phase 0.0** — Sync plugin repo to source. Closes version drift before adding new work.
2. **Phase 0.2** — Probe hook to determine PreToolUse output format. Without this, all subsequent hook work is guessing.
3. **Phase 0.3** — Workspace detect helper. Verify daemon DB schema. Smoke test against current setup.
4. **Phase 0.1** — Write the new hook scripts using the verified output format.
5. **Phase 0.4** — Codex install script.
6. **Phase 1A** — Validate the dev loop (`--plugin-dir` + `/reload-plugins`).
7. **Phase 1B** — One-shot install-and-verify, then return to 1A as primary workflow.
8. **Phase 2** — Codex side: hook merge, skill deploy, AGENTS.md augment. MCP step skipped (already done).
9. **Phase 4** — Verification battery, record in ledger.
10. **Phase 3.4** — Add `cargo xtask sync-plugin` so future iteration is one command.

Total estimated effort: 1.5-2 days. Largest line items are the new hook scripts (especially after probing what actually works) and the Codex install automation.

---

## Corrections applied from v2 (codex-cli review pass 2)

Pass 2 confirmed the v1→v2 corrections held. Four new findings, all addressed:
1. **High — Daemon SQL column wrong.** `workspaces.path`, not `root_path`. Verified live: `sqlite3 ~/.julie/daemon.db '.schema workspaces'`. Code sample fixed.
2. **High — `better-sqlite3` is not a plugin dep.** Plugin has zero deps. Switched code sample to `node:sqlite` (Node 22.5+ stable, built-in, no native build) and added `engines.node` requirement.
3. **Medium — Existing `pretool-edit.cjs` and `pretool-agent.cjs` still use raw `console.log`.** Phase 0.1 now explicitly says "convert existing hooks to format helpers" alongside writing the new ones.
4. **Low — `claude /plugin list` is suspect.** Use `claude plugin list` (CLI subcommand) or `/plugin` interactive UI in-session.

## Corrections applied from v1 (codex-cli adversarial review)

1. **High — Codex MCP claim was wrong.** v1 said Julie wasn't registered in Codex; actually registered project-scope per project with `env.JULIE_WORKSPACE`. Reframed as "no global Codex setup, per-project setup is correct as-is, plugin install does NOT touch MCP". Added section explaining the desktop-vs-CLI rationale.
2. **High — PreToolUse "nudge" semantics.** v1 assumed `console.log` from a hook would reach the model. Codex correctly pointed out raw stdout on PreToolUse exit 0 is not seen by Claude. v2 makes hook output format an explicit constraint (#5), adds Phase 0.2 to probe the actual format, and references `hookSpecificOutput.additionalContext` and `permissionDecision: deny`. **Marked as needing verification because exact field names may have evolved.**
3. **High — Plugin install command syntax.** v1 used invented `julie-plugin@local-julie`. v2 uses verified `julie@julie-plugin` (plugin@marketplace).
4. **High — `.julie/` ancestry detection broken in daemon mode.** v2 queries daemon DB first (`~/.julie/daemon.db`), falls back to ancestry. Schema needs verification.
5. **Medium — MCP scope conflict no longer "open".** Documented precedence (local > project > user > plugin > connectors). Removed from open questions.
6. **Medium — Local marketplace vs `--plugin-dir`.** v1 conflated dev iteration with installed dogfood. v2 splits into Phase 1A (`--plugin-dir`, the actual dev primitive) and Phase 1B (marketplace install for verifying the install path).
7. **Medium — Plugin repo stale.** v2 starts with Phase 0.0: bring plugin to current source before adding new hooks.
8. **Medium — Codex hook payload shapes.** v2 calls for probe hooks against multiple Codex tool types before relying on inferred payload structure.
9. **Low — "Zero overhead".** Reworded to "low overhead, measure before claiming".
10. **Low — Skill namespacing.** Verification expects `julie:editing`, not bare `editing`.
