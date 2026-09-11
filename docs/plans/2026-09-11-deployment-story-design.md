# Deployment story design (v8.0.0)

Owner sequence item 4. Brief: `.memories/briefs/machine-service-one-rust-service-per-machine-repla.md`.

## Goal

A user on Claude Code, Codex, Antigravity, OpenCode, Hermes, or Cursor installs Julie in one step and gets the machine service with semantic search working, from one set of release artifacts. The owner then tags v8.0.0 and pushes; everything before that is prepared on the branch.

## What exists (verified 2026-09-11)

- Julie's last GitHub release is v7.18.1 (2026-08-20): four archives, each holding only `julie-server`. Main carries the machine service unreleased (176 commits, 1,044 files) with `Cargo.toml` still at 7.18.1 and `docs/site/index.html` showing v7.18.1.
- `julie-plugin` (Claude Code plugin, version 7.18.0): `hooks/run.cjs` extracts the archive from `bin/archives/` on first use and execs `julie-server` with no arguments, which is the stdio shim that auto-starts the machine service. Skills are copied from `anortham/julie` at the release tag by `.github/workflows/update-binaries.yml`. `hooks/hooks.json` is empty and the plugin's CLAUDE.md records a decision: no behavior hooks. Its README still tells users to install `uv` for a Python embedding host that no longer exists. `bin/install-codex.cjs` and `bin/install-opencode.cjs` symlink skills and print an MCP command.
- Semantic search needs `julie-semantic-sidecar`. Julie looks beside `julie-server`, then `~/.cache/julie-semantic/bin/`, then `PATH` (`crates/julie-pipeline/src/embeddings/native/launch.rs:50-77`). Nothing installs it. The sidecar has its own GitHub release v0.1.0 with four portable archives (2 to 34 MB).
- MCP server instructions: `JULIE_AGENT_INSTRUCTIONS.md` (26 lines, about 1.7 KB) is embedded with `include_str!` and served to every harness. Claude Code shares roughly a 4 KB budget across all configured servers' instructions and silently truncates the tail (anthropics/claude-code issue 43474), so a user with two or three other servers can lose Julie's block. The julie repo's dev hook `.claude/hooks/session-start.cjs` prints `.claude/hooks/julie-routing-block.md`, which duplicates the instructions and adds julie-dev-only lines.
- `.agents/skills/` in the julie repo is a stale copy of `.claude/skills/` (the editing skill still names deleted tools). `cargo xtask sync-plugin` mirrors `.claude/skills/` to the plugin and only reports hook divergence.
- The service mints a new bearer token on every start (`src/service/discovery.rs:15`), so the README's HTTP registration URL (`http://127.0.0.1:<port>/mcp?token=<token>`) breaks at each restart.
- Harness mechanics, verified against current docs:
  - Codex plugins follow the Agent Plugins standard (agent-plugins.org; steering committee Amazon, Cursor, Microsoft, OpenAI, Vercel): root `plugin.json` (`$schema`, `name`, `version`, `description`), `mcp.json` with `mcpServers.<name>` of `type: stdio`, `command` (one executable token; `./` resolves inside the plugin root), `args`; `skills/` auto-discovered; hooks under `hooks/hooks.json`. `.codex-plugin/plugin.json` is the compatibility fallback. Repo marketplaces live at `.agents/plugins/marketplace.json`; `codex plugin marketplace add <owner>/<repo>` then `codex plugin add <name>@<marketplace>`.
  - Antigravity CLI (`agy`): plugin bundle with root `plugin.json` (`name`, `description`), optional `mcp_config.json` (`mcpServers.<name>` with `command`, `args`, `env`, `cwd`), `skills/`, `hooks.json`; `agy plugin install <path>`; ponytail's README also documents `agy plugin install <git URL>`. Global MCP config `~/.gemini/config/mcp_config.json`, workspace `.agents/mcp_config.json`.
  - OpenCode: `opencode.json` `mcp.<name>` with `type: local`, `command` array, `environment`, `enabled`. No plugin path for MCP servers.
  - Hermes Agent: `~/.hermes/config.yaml` `mcp_servers.<name>` with `command` and `args`. Plugins are Python packages and cannot register MCP servers.
  - Cursor: `~/.cursor/mcp.json` or `.cursor/mcp.json`, `mcpServers.<name>` with `command`, `args`, `env`; HTTP with `url`, `headers`. Cursor has its own marketplace, not the Agent Plugins standard.
  - Claude Code: `.claude-plugin/plugin.json` with `mcpServers`, `hooks`; marketplace via `/plugin marketplace add <owner>/<repo>`.
- `~/source/ponytail` (owner's pointer) is a working example of one repo serving Claude Code, Codex, Antigravity, OpenCode, Hermes, and Cursor with one manifest per harness and shared `skills/` and `hooks/`.

## Design

One launcher, one archive per platform, one instructions file, one plugin repo with a manifest per harness.

### 1. Release archives carry the sidecar

`release.yml` downloads the pinned sidecar asset for the matching target from `anortham/julie-semantic-sidecar` (version pinned in the workflow, sha256 checked against the published `.sha256`), unpacks it, and places `julie-semantic-sidecar` beside `julie-server` in the archive. Julie finds it with no configuration. The mapping: aarch64-apple-darwin and x86_64-apple-darwin use the `metal-portable` assets; x86_64-unknown-linux-gnu and x86_64-pc-windows-msvc use the `vulkan-portable` assets. The release notes list both versions.

Rejected: Julie downloading the sidecar on first use (network, checksum, and cache code in the product); leaving it manual.

### 2. Version 8.0.0

`Cargo.toml` and `Cargo.lock` to 8.0.0; `docs/site/index.html` to v8.0.0; `docs/release-notes/v8.0.0.md` written by hand (the workflow's boilerplate is not enough per CLAUDE.md): machine service, stdio shim, service commands, semantic search by default, bundled sidecar, removed tools and the daemon, index hygiene, test tiers, upgrade notes (old `~/.julie` daemon state, first start reindexes), known caveats. The plugin workflow bumps `plugin.json` to 8.0.0 when the owner runs it.

### 3. Plugin repo: one manifest per harness

Layout after the change (ponytail's shape):

```
julie-plugin/
  plugin.json                  root manifest: Agent Plugins schema (Codex) and Antigravity read it
  .claude-plugin/plugin.json   Claude Code manifest (mcpServers, hooks)
  .codex-plugin/plugin.json    Codex compatibility manifest (skills, hooks)
  mcp.json                     Agent Plugins MCP declaration (stdio, node ./hooks/run.cjs)
  mcp_config.json              Antigravity MCP declaration (same command)
  .agents/plugins/marketplace.json   Codex repo marketplace
  .claude-plugin/marketplace.json    Claude Code marketplace (exists today as the marketplace source)
  hooks/hooks.json             SessionStart and SubagentStart -> hooks/session-start.cjs (Claude Code and Codex)
  hooks/session-start.cjs      prints JULIE_AGENT_INSTRUCTIONS.md as additional context
  hooks/run.cjs                unchanged launcher
  skills/                      copied from julie at the tag
  JULIE_AGENT_INSTRUCTIONS.md  copied from julie at the tag
  bin/archives/                julie-v8.0.0-<target>.tar.gz|zip, now with the sidecar inside
  bin/install-opencode.cjs     kept; prints the verified opencode.json block
  bin/install-codex.cjs        deleted; the Codex marketplace replaces it
```

Every MCP declaration runs the same command: `node <plugin-root>/hooks/run.cjs`. Each manifest's exact path form (`${CLAUDE_PLUGIN_ROOT}`, `./hooks/run.cjs`, or an absolute path printed by the installer) is verified against the harness docs in the plan task that writes it.

Hooks: the plugin ships the SessionStart and SubagentStart hook for Claude Code and Codex. This reverses the plugin's recorded "no behavior hooks" decision for one reason, written into the plugin's CLAUDE.md: Claude Code's shared instruction budget truncates server instructions when other servers are configured, so the hook is the only reliable channel for the routing text there. The hook prints the same file the MCP instructions embed, nothing else. `JULIE_SESSION_HOOKS=0` disables it, as in the julie dev hook.

### 4. One instructions file

`JULIE_AGENT_INSTRUCTIONS.md` is the only per-session routing text: rules, tool list, paging, workspace, and the four workflow lines from the old routing block. It stays under 2 KB. `.claude/hooks/julie-routing-block.md` is deleted; `.claude/hooks/session-start.cjs` prints `JULIE_AGENT_INSTRUCTIONS.md` instead. The rest of the old routing block goes where it belongs: the extractor re-pin gate and the standalone-CLI evidence rule to CLAUDE.md (julie-dev only); the CLI wrapper examples and the `extract` section to the README's CLI and External Extract sections, which already cover them; the "Subagents: paste this" block is deleted because the SubagentStart hook now delivers the same text to subagents in Claude Code and Codex. The test that asserted routing-block strings (`test_agent_instructions_recommend_standalone_for_quick_dogfood_checks`, `src/tests/cli_tools_tests.rs:659`) is repointed at CLAUDE.md. The unreferenced `.claude/hooks/pretool-edit.cjs` and `pretool-agent.cjs` are deleted. `.agents/skills/` is deleted from the julie repo; `cargo xtask sync-plugin` also mirrors `JULIE_AGENT_INSTRUCTIONS.md` to the plugin.

### 5. Install paths in the docs

README "Installation" lists exactly six harnesses, each with the verified command or config block, and one "Manual" section (download the archive, run the binary as a stdio MCP server). The HTTP URL registration section is removed: the stdio shim works on every harness and the token is not stable. The `JULIE_WORKSPACE` table stays.

| Harness | Install | Mechanism |
|---|---|---|
| Claude Code | `/plugin marketplace add anortham/julie-plugin`, `/plugin install julie@julie-plugin` | plugin manifest with `mcpServers` and hooks |
| Codex | `codex plugin marketplace add anortham/julie-plugin`, `codex plugin add julie@julie-plugin` | Agent Plugins `mcp.json`, skills, hooks |
| Antigravity | `agy plugin install https://github.com/anortham/julie-plugin` | root `plugin.json`, `mcp_config.json`, skills |
| OpenCode | clone, `node bin/install-opencode.cjs`, paste the printed `opencode.json` block | `mcp.julie` local command |
| Hermes | clone, add the printed `mcp_servers.julie` block to `~/.hermes/config.yaml` | stdio command |
| Cursor | clone, add the `mcpServers.julie` block to `~/.cursor/mcp.json` | stdio command |

The plugin README carries the same table; the `uv` and Python text goes.

### 6. Owner runbook (not executed by this item)

Written into the finding as a checklist: push main; `git tag v8.0.0`; push the tag; wait for `release.yml`; replace the release page body with `docs/release-notes/v8.0.0.md` if the workflow did not pick it up; run the plugin's `update-binaries.yml` with `version=8.0.0 tag=v8.0.0`; review and push the plugin commit; install the plugin fresh on one machine and confirm `julie-server service status` shows the embedding child ready after one search.

### End-to-end check on this machine

Before the owner tags: build the Linux archive the way the workflow does (release binary plus the downloaded sidecar), drop it into a local plugin checkout's `bin/archives/`, run `node hooks/run.cjs` as an MCP client would, send one `initialize` and one `fast_search`, and confirm `service status` reports the embedding child ready and `julie-semantic-sidecar` extracted beside `julie-server`. All six harness CLIs are installed on this machine (`codex`, `agy`, `opencode`, `hermes`, `cursor`, and Claude Code), so each install path is exercised for real from the local plugin checkout: the plugin or config is added, the harness starts, and Julie answers one `fast_search`. The finding records each result.

## Acceptance criteria

- [x] `release.yml` packs `julie-semantic-sidecar` beside `julie-server` for all four targets, sha256 verified, sidecar version pinned in one place.
- [x] Version 8.0.0 in `Cargo.toml`, `Cargo.lock`, `docs/site/index.html`; `docs/release-notes/v8.0.0.md` exists and covers the machine service, removed features, upgrade notes, and caveats.
- [x] Plugin repo has the manifests in section 3, the hook, `JULIE_AGENT_INSTRUCTIONS.md`, no `uv` text, no `install-codex.cjs`; `node --test hooks/*.test.cjs` passes; the plugin's CLAUDE.md records the hook reason.
- [x] `JULIE_AGENT_INSTRUCTIONS.md` under 2 KB; `julie-routing-block.md`, the pretool hooks, and `.agents/skills/` are gone from julie; the dev hook prints the instructions file; `cargo xtask sync-plugin --dry-run` reports the instructions file.
- [x] README and plugin README list the six harnesses with commands verified against current docs (each task cites the doc URL it checked), and no HTTP-URL registration.
- [x] Local end-to-end: packed Linux archive through `run.cjs`, one search, embedding child ready, sidecar found beside the binary.
- [x] `cargo xtask test dev` green (docs contract tests included); plugin tests green.
- [x] Finding at `docs/findings/2026-09-11-deployment-story.md` with the owner runbook and the verification evidence.

## Rejected

- Support every harness ponytail supports (Gemini, Copilot CLI, pi, Qoder, Devin, Grok, and more). The owner set six. The Agent Plugins layout makes later additions a manifest each.
- Stable token plus HTTP registration. Adds durable state to the service for a route the stdio shim already covers everywhere.
- A Hermes Python plugin for skills. Hermes gets the MCP config block; skills come later if asked.
- Keep the plugin free of hooks. The instruction-budget truncation is measured and real; the hook is one file printing one shared text.

## Architecture impact

No product module changes. `release.yml` gains one download-and-verify step. `xtask sync-plugin` mirrors one more file. The plugin repo changes layout; `run.cjs` is untouched.
