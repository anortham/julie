# Finding: deployment story, v8.0.0 (owner sequence item 4)

Branch `deployment-story` over main `91c7968c`. Plugin branch `v8-deployment` over the local `d741d4c` (7.18.0); plugin `main` on GitHub is the workflow's orphan commit `09a2c23` (7.18.1), see the runbook. Design: `docs/plans/2026-09-11-deployment-story-design.md`. Plan: `docs/plans/2026-09-11-deployment-story-plan.md`. Ledger: `docs/plans/2026-09-11-deployment-story-ledger.md`.

## Verdict

Ready for the owner to tag. Every design acceptance criterion has evidence below. One design assumption was wrong and is corrected on the branch: Codex and Antigravity do not start a plugin's MCP server in the project directory, so both harnesses register Julie from their own config, and the plugin carries only skills and hooks for them. One product defect found by the checks is fixed: the stdio shim ignored `JULIE_WORKSPACE`.

## What shipped

Per design section:

1. **Release archives carry the sidecar.** `.github/scripts/pack-release.sh <target> <version> <release-dir> <out-dir>` downloads the pinned `julie-semantic-sidecar` 0.1.0 asset for the target, checks its sha256 against the value in the script, and packs it beside `julie-server` at the archive root. `release.yml` calls it for all four targets. `--dry-run` prints the asset and sha256 without network. Julie finds the sidecar beside its own binary first, so semantic search works from the archive alone.
2. **Version 8.0.0.** `Cargo.toml`, `Cargo.lock`, `docs/site/index.html`, and `docs/release-notes/v8.0.0.md` (machine service, semantic search by default, removed features, upgrade notes, caveats).
3. **Plugin repo, one manifest per harness.** Root `plugin.json` (Agent Plugins schema), `.claude-plugin/plugin.json` with `mcpServers` and `hooks`, `.codex-plugin/plugin.json` with skills and hooks, `.agents/plugins/marketplace.json`, `hooks/hooks.json` with SessionStart and SubagentStart, `hooks/session-start.cjs`, `JULIE_AGENT_INSTRUCTIONS.md`. `bin/install-codex.cjs` deleted. The plugin CLAUDE.md records why the hook exists. Changed after the harness checks: `mcp.json` and `mcp_config.json` deleted, see "What the harness checks changed".
4. **One instructions file.** `JULIE_AGENT_INSTRUCTIONS.md` (1,960 bytes) is the only routing text. The machine service sends it on `initialize` (fixed in `9e38fdba`; the service built its adapter without instructions). The julie dev hook and the plugin hook print it. `cargo xtask sync-plugin` mirrors it. `.claude/hooks/julie-routing-block.md`, `pretool-edit.cjs`, `pretool-agent.cjs`, and `.agents/` are gone.
5. **Install paths in the docs.** The README lists six harnesses plus Manual; the plugin README lists the six. No HTTP-URL registration, no `install-codex`, no `uv`. Both name Node.js 22.5 or newer as the one prerequisite.

## End-to-end evidence (Task 6, live, julie `622c616d`, plugin `bbd2c53`)

Packed Linux archive (`julie-server` 8.0.0 plus sidecar 0.1.0, 55 MB) through `node hooks/run.cjs` with its own `JULIE_HOME`:

| Invariant | Result |
|---|---|
| Archive holds both binaries at the root | pass: `tar tzf` lists `julie-semantic-sidecar`, `julie-server`, the `libggml*` and `libllama*` libraries, licenses, `package-manifest.json` |
| `run.cjs` extracted both side by side | pass: `bin/x86_64-unknown-linux-gnu/` holds both; marker `.extracted-archive` names the 8.0.0 archive |
| `initialize` returned version 8.0.0 | pass |
| `initialize` returned instructions | fail at `622c616d`, then fixed in `9e38fdba`; a direct `initialize` through the plugin launcher against the fixed service returned 1,960 instruction bytes (`scratchpad/shim-env-livecheck.jsonl`) |
| `fast_search` returned a hit | pass: 6 hits, first `crates/julie-pipeline/src/embeddings/native/launch.rs:52` |
| Embedding child ready within 60 s | pass on the first poll; the sidecar process ran from the plugin's `bin/` directory beside the packed binary |
| Service stopped | pass |

## Harness checks (Task 7, live, service built from julie `9e38fdba`, packed shim from `622c616d`, plugin `97a4111` launcher)

Prompt for every harness: "Call the julie MCP tool fast_search with query "find_default_sidecar_binary" and backend "lexical". Reply with only the first hit line, verbatim." Expected reply: `:52 find_default_sidecar_binary function`. The checks ran from `/home/murphy/source/julie` against the developer service (8.0.0, built from `9e38fdba` and started 21:29:44Z). The launcher ran the packed shim from the Task 6 archive (`622c616d` build); the shim fix in `18403740` came later and has its own live row in the ledger. Every config edit was backed up first and restored byte for byte afterwards; both plugin installs were removed.

| Harness | Install used | Outcome | Doc |
|---|---|---|---|
| Claude Code 2.x | `claude -p --plugin-dir <plugin> --allowedTools mcp__plugin_julie_julie__fast_search --disallowedTools 'mcp__julie__*'` | pass: `plugin:julie:julie` connected, `mcp__plugin_julie_julie__fast_search` returned 6 hits, reply verbatim | https://code.claude.com/docs/en/plugin-marketplaces |
| Codex 0.154.0 | `codex plugin marketplace add <plugin>`, `codex plugin add julie@julie-plugin`, then `codex exec --approve-for-me -c 'mcp_servers.julie.command="node"' -c 'mcp_servers.julie.args=["<plugin>/hooks/run.cjs"]'` (the config entry the README shows) | pass: 6 hits, reply verbatim. Through the plugin's own `mcp.json` (since deleted): "No results found"; the service had registered `~/.codex/plugins/cache/julie-plugin/julie/7.18.0` (30 files) as the workspace | https://developers.openai.com/plugins/build/plugins |
| Antigravity 1.2.1 | `agy plugin install <plugin>`, then `agy -p --dangerously-skip-permissions` with the `mcpServers.julie` entry in `~/.gemini/config/mcp_config.json` | pass: reply verbatim in 33.6 s. Through the plugin's own `mcp_config.json` (since deleted): the service registered `~/.gemini/config/plugins/julie` (30 files) and the agent looped on `manage_workspace` for 90 s | https://antigravity.google/docs/cli/plugins/ |
| OpenCode 1.18.25 | `~/.config/opencode/opencode.json` `mcp.julie` local command, `opencode run --format json` | pass: `julie_fast_search` completed with 6 hits | https://opencode.ai/docs/mcp-servers/ |
| Hermes | `printf 'Y\n' \| hermes mcp add julie --command node --args <plugin>/hooks/run.cjs`; `hermes mcp test julie`; `hermes chat -q ... --oneshot` | pass: connected in 451 ms, 10 tools, reply verbatim | https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp |
| Cursor (cursor-agent) | `~/.cursor/mcp.json` `mcpServers.julie`; `cursor-agent mcp list-tools julie`; `cursor-agent -p --approve-mcps --trust --force` | pass: 10 tools listed, `julie-fast_search` returned, reply verbatim | https://cursor.com/docs/context/mcp |

Headless notes: `codex exec` refuses MCP tool calls under its default approval policy; `--approve-for-me` or `[plugins."<plugin>".mcp_servers.<server>] default_tools_approval_mode` unlocks them. `cursor-agent -p` rejects tool calls until `--force`. `hermes mcp add` asks "Enable all 10 tools?" on a TTY.

## What the harness checks changed

A scratch plugin whose MCP server logs its environment showed what each harness passes to a plugin-declared server:

- Codex: cwd is the plugin cache root; the environment holds nine variables (`PLUGIN_ROOT`, `PLUGIN_DATA`, `HOME`, `PATH`, `SHELL`, `TERM`, `USER`, `LANG`, `LOGNAME`), none of which names a project; `initialize` declares no `roots` capability and `roots/list` returns `[]`; `tools/call` `_meta` holds call, thread, and item ids plus turn metadata (sandbox, model, Codex version), nothing about the project.
- Antigravity: cwd is the plugin root; `roots.listChanged` is declared but `roots/list` returns `[]`; the environment is the full parent environment, with `PWD` set by the launch shell.

The v8 shim binds every unbound tool call to its own working directory (`bind_default_workspace`). So under both harnesses the plugin-declared server indexed the plugin directory. No signal from either harness names the project. Decision: the plugin declares no MCP server for Codex or Antigravity. Both READMEs show the config entry (`[mcp_servers.julie]` in `~/.codex/config.toml`; `mcpServers.julie` in `~/.gemini/config/mcp_config.json`) with `node /absolute/path/to/julie-plugin/hooks/run.cjs`. A server from the user's config starts in the project directory on both harnesses (probe cwd `/home/murphy/source/julie`). Claude Code starts the plugin server in the project directory, so its manifest keeps `mcpServers`.

Rejected: tell the agent to pass `workspace` on every call (fails silently when the model forgets); a session hook that writes the project path for the launcher (a handoff file, and Antigravity does not load `hooks/hooks.json`); read `PWD` (Codex passes none).

Product fix: `src/service/shim.rs` `shim_workspace_root()` returns `JULIE_WORKSPACE` when set, else the working directory, through `resolve_workspace_startup_hint`. `run_stdio_shim` used `current_dir()` only, while the README promised the variable. Test `shim_workspace_root_honors_julie_workspace_env`. The README's client-roots table is replaced by one workspace-resolution paragraph: the shim never reads MCP roots.

## Owner runbook

The plugin's `Update Plugin` workflow checks out the branch it runs on, downloads the four archives from the julie release, copies the skills and `JULIE_AGENT_INSTRUCTIONS.md` at the tag, bumps the six manifests, and force-pushes a fresh orphan commit to `main`. `main` on GitHub is such an orphan today (`09a2c23`, 7.18.1) and shares no history with `v8-deployment`, so a merge cannot work. `release.yml` dispatches that workflow on the plugin's `main` at the end of the release job (`PLUGIN_REPO_TOKEN`). So the v8 layout must be on plugin `main` before the tag.

1. In `/home/murphy/source/julie-plugin`: `git push --force origin v8-deployment:main`. `main` is a generated snapshot and the workflow replaces it again in step 3. Do not commit `bin/archives/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz`.
2. In `/home/murphy/source/julie`: merge `deployment-story` into `main`, then push `main`.
3. `git tag v8.0.0` and push the tag. `release.yml` builds the four targets, packs the sidecar with `.github/scripts/pack-release.sh`, publishes the release with `docs/release-notes/v8.0.0.md` as the body, then dispatches `Update Plugin` with `version=8.0.0` and `tag=v8.0.0`.
4. Open the release page. If the body is the workflow boilerplate, run `gh release edit v8.0.0 --notes-file docs/release-notes/v8.0.0.md`.
5. Check the archives by hand; the workflow's verify step prints fixed text only. `gh release download v8.0.0 --repo anortham/julie --pattern 'julie-v8.0.0-*' --dir /tmp/julie-v8`, then `tar tzf` each `.tar.gz` and `unzip -l` the `.zip`. Each listing must have `julie-server` and `julie-semantic-sidecar` at the root.
6. Check the plugin run: `gh run list --repo anortham/julie-plugin --workflow "Update Plugin"`. When it is green, `git fetch origin` and review `origin/main`: root `plugin.json` at 8.0.0, four 8.0.0 archives in `bin/archives/`, `JULIE_AGENT_INSTRUCTIONS.md`, `hooks/session-start.cjs`, no `mcp.json`. There is nothing to push; the workflow pushed `main` itself. If the dispatch did not fire, run `gh workflow run "Update Plugin" --repo anortham/julie-plugin --ref main -f version=8.0.0 -f tag=v8.0.0`.
7. On one machine with no prior install: `/plugin marketplace add anortham/julie-plugin`, `/plugin install julie@julie-plugin`, run one `fast_search`, then `~/.claude/plugins/cache/julie-plugin/julie/8.0.0/bin/<target>/julie-server service status` must show `embedding_child.state` `ready`. The launcher does not put `julie-server` on `PATH`.
8. In Codex: `codex plugin marketplace add anortham/julie-plugin`, `codex plugin add julie@julie-plugin`, add the `[mcp_servers.julie]` block, open `/hooks`, trust the two Julie hooks.

## Side findings

- Workers' Julie MCP calls failed with "could not start service: No such file or directory" during Batch A while the lead's calls worked (Task 3 report). Not reproduced by the lead. Likely the workers' shim resolved a `julie-server` path that did not exist in their environment; worth a look when a worker reports it again.
- `hooks/run.cjs findArchive` took the first `readdirSync` match. With the tracked 7.18.0 archives beside the local 8.0.0 archive it extracted 7.18.0, so the first round of harness checks ran the old binary. Fixed in plugin `97a4111`: the matches are sorted numerically and the last one wins.
- `cargo xtask sync-plugin` needs `--plugin-root` from a worktree; its default root is the worktree's parent.
- The plugin's `bin/archives/` is not git-ignored. The four 7.18.0 archives are tracked; a local 8.0.0 archive shows as untracked. `update-binaries.yml` replaces the set on release.
- The plugin's CLAUDE.md said `node --test hooks/`, which fails on Node 24 ("Cannot find module .../hooks"). Now `node --test hooks/*.test.cjs`.
- `bin/install-opencode.cjs` printed a block with `environment.JULIE_WORKSPACE` fixed to one project. Pasted into the global `opencode.json`, that pins every OpenCode session to that project. The block no longer carries it; OpenCode starts the server in the project directory.
- Antigravity reads a root `hooks.json`, not `hooks/hooks.json`, so the session hook does not load there. No hook shipped for Antigravity; the design promised hooks for Claude Code and Codex only.
- A running stdio shim from an older build refuses a newer service ("service version 8.0.0 does not match client version 7.18.1; run: julie-server service restart"). After a release rebuild plus `service restart`, every open MCP session keeps its old shim until the harness restarts it. Expected, but it surprised this session.
- The Codex and Antigravity plugin installs copy the whole plugin checkout, including untracked files and the extracted `bin/<target>/` directory.
- A shim started in `/tmp` without `JULIE_WORKSPACE` binds `/tmp` and the service indexes it (7,190 files into the stray `tmp_e9671acd` checkout that item 2 deferred). It also writes `/tmp/.julie/logs/`, a workspace marker in the system temp root that then broke `test_workspace_root_still_rejects_home_julie_under_new_vcs_markers` (CLAUDE.md Known Failures) until the directory was removed. Any client that starts Julie in a non-project directory pays the same cost. This is the reason the plugin declares no server for Codex and Antigravity.

## Deferred

- The Codex and Antigravity path is two steps (plugin plus config block). A one-step path needs a harness signal that names the project; neither harness sends one today.
- A hook for Antigravity (root `hooks.json`; format to verify against https://antigravity.google/docs/cli/plugins/).
- Plugin manifests stay at 7.18.0 on the branch; the workflow bumps them at release.
- The workspace-resolution table for VS Code and Windsurf is gone from the README; those clients were never checked on this branch.
