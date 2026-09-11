# Deployment Story (v8.0.0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** Prepare v8.0.0 so a user on Claude Code, Codex, Antigravity, OpenCode, Hermes, or Cursor installs Julie in one step and gets the machine service with semantic search, from one set of release archives.

**Architecture:** One launcher (`hooks/run.cjs` in the plugin), one archive per platform that carries `julie-server` and `julie-semantic-sidecar` side by side, one routing text (`JULIE_AGENT_INSTRUCTIONS.md`) served as MCP instructions and printed by a SessionStart/SubagentStart hook, and one plugin repo with a manifest per harness. Design: `docs/plans/2026-09-11-deployment-story-design.md`.

**Tech Stack:** GitHub Actions (bash steps), Node 22 `node:test` in the plugin, Rust xtask, Markdown docs.

**Architecture Quality:** No product module changes. `release.yml` calls one new pack script. `xtask sync-plugin` mirrors one more file. The plugin repo changes layout; `run.cjs` is untouched. Risk: harness manifest shapes drift; every manifest task cites the doc URL it verified against.

## Global Constraints

- Two repos. Julie: `/home/murphy/source/julie/.worktrees/deployment-story` (branch `deployment-story`). Plugin: `/home/murphy/source/julie-plugin` (branch `v8-deployment`, currently clean at version 7.18.0).
- Sidecar pin: `anortham/julie-semantic-sidecar` release `v0.1.0`. Asset names and sha256 of the archives:
  - `julie-semantic-sidecar-0.1.0-aarch64-apple-darwin-metal-portable.tar.gz` `bd84211306145690c1033338c775c5f5af7bcf0752d8e6b25c579dbd082e0ab1`
  - `julie-semantic-sidecar-0.1.0-x86_64-apple-darwin-metal-portable.tar.gz` `c4e996abdd711efde1075af0cd222643a298354ec10f03f1f7c3bee9f70a8bdf`
  - `julie-semantic-sidecar-0.1.0-x86_64-unknown-linux-gnu-vulkan-portable.tar.gz` `14b369076776fc7e7ed0ee2a8d8261311e09b634bdc7d57eed928c4fcfa61212`
  - `julie-semantic-sidecar-0.1.0-x86_64-pc-windows-msvc-vulkan-portable.zip` `659c952b405a087b98775e9fc68223b067955b61f525d7647e50087fe052972d`
  Each asset has a sibling `<asset>.sha256` whose content is `<sha256>  <asset>` (two spaces). Sidecar archives hold the binary, its shared libraries (Linux `libggml-*.so*`, Windows `ggml-*.dll`; macOS has none), `LICENSE`, `README.md`, `THIRD_PARTY-LICENSES.html`, `package-manifest.json`, all at the archive root.
- Julie archive names stay `julie-v<version>-<target>.tar.gz` (`.zip` on Windows), all files at the archive root, because `run.cjs` matches `julie-v` + `-<target>.tar.gz|.zip` and extracts the whole archive into `bin/<target>/`.
- Julie finds the sidecar beside `julie-server` first (`crates/julie-pipeline/src/embeddings/native/launch.rs:52-58`), then `~/.cache/julie-semantic/bin/`, then `PATH`.
- `JULIE_AGENT_INSTRUCTIONS.md` stays under 2,000 bytes (today 1,883). It is embedded with `include_str!` in `src/handler.rs:1450`.
- Every MCP declaration in the plugin runs `node <plugin-root>/hooks/run.cjs`. Path forms per harness: Claude Code `${CLAUDE_PLUGIN_ROOT}/hooks/run.cjs`; Agent Plugins `mcp.json` command `node`, args `["./hooks/run.cjs"]`; Antigravity `mcp_config.json` command `node`, args `["./hooks/run.cjs"]`; OpenCode, Hermes, Cursor use the absolute path of the clone.
- Harness docs must be web-verified before a task writes a manifest or an install block, and the task's report cites the URL. The design doc's "What exists" section lists the shapes verified on 2026-09-11.
- Version 8.0.0 everywhere: `Cargo.toml`, `Cargo.lock`, `docs/site/index.html`. The plugin manifests stay at 7.18.0 in this branch; the plugin workflow bumps them to 8.0.0 when the owner runs it.
- Writing rules: Simplified Technical English in every doc and note. No narration comments. Zero comments in tests. No em dashes in new prose.
- Julie repo: `CLAUDE.md` and `AGENTS.md` byte-identical (pre-commit hook). Goldfish checkpoint before every commit, checkpoint file staged in that commit. Commit trailers `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_012VvBh5y2tRkpmtPZmUTFod`.
- Leave the owner's uncommitted `.codex/config.toml` and `.memories/2026-09-11/133457_b2d2.md` in the main checkout untouched.
- Never push, tag, or publish. The owner does that with the runbook in the finding.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` "RUNNING TESTS" (julie); `CLAUDE.md` in the plugin repo (`node --test hooks/*.test.cjs`).

**Worker red/green scope:** julie: `cargo nextest run --lib <exact_test_name>` or `cargo nextest run -p xtask <exact_test_name>`. Plugin: `node --test hooks/<file>.test.cjs`. Docs-only tasks: the exact contract test named in the task.

**Worker ceiling:** the exact tests named in the task, at most two runs per change. No xtask tiers.

**Worker gate invariant:** each task lists the test that proves its behavior.

**Lead affected-change scope:** `cargo xtask test dev` once after the batch lands.

**Branch gate:** `cargo xtask test dev` in the julie worktree and `node --test hooks/*.test.cjs` in the plugin repo, both at the final HEADs, plus the local end-to-end check of Task 6 and the harness checks of Task 7.

**Security scope:** none declared.

**Replay/metric evidence:** hard gates: dev tier green, plugin tests green, embedding child ready in the end-to-end check, `JULIE_AGENT_INSTRUCTIONS.md` under 2,000 bytes. Report-only: per-harness install results in the finding.

**Escalation triggers:** a change to `src/` beyond the one test repoint escalates to `cargo xtask test full`.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-11-deployment-story-ledger.md` (copy of `docs/plans/verification-ledger-template.md`). Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse a passing entry for the same HEAD and scope instead of rerunning.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Release archives carry the sidecar | Batch A | julie: create `.github/scripts/pack-release.sh`; modify `.github/workflows/release.yml` | No | None - safe parallel batch. |
| Task 2: Version 8.0.0 and release notes | Batch A | julie: modify `Cargo.toml`, `Cargo.lock`, `docs/site/index.html`; create `docs/release-notes/v8.0.0.md` | No | None - safe parallel batch. |
| Task 3: One instructions file in julie | Batch A | julie: modify `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/hooks/session-start.cjs`, `CLAUDE.md`, `AGENTS.md`, `src/tests/cli_tools_tests.rs`, `xtask/tests/docs_contract_tests.rs`, `xtask/src/sync_plugin.rs`; delete `.claude/hooks/julie-routing-block.md`, `.claude/hooks/pretool-edit.cjs`, `.claude/hooks/pretool-agent.cjs`, `.agents/` | No | None - safe parallel batch. |
| Task 4: README install paths | Batch A | julie: modify `README.md` | No | None - safe parallel batch. |
| Task 5: Plugin repo, one manifest per harness | Batch A | plugin repo, every file except `hooks/run.cjs`, `hooks/run.test.cjs`, `skills/`, `bin/archives/` | No | None - safe parallel batch (separate repo). |
| Task 6: Local end-to-end through run.cjs | None - serial | julie: none; plugin: `bin/archives/` (untracked scratch only); scratch dir | Yes | Needs Task 1's pack script, Task 3's instructions file, and Task 5's plugin layout committed. |
| Task 7: Six harness install checks (lead) | None - serial | user harness configs (backed up and restored); no repo files | Yes | Needs Task 6's packed archive in the plugin checkout. |
| Task 8: Finding, ledger, gate (lead) | None - serial | julie: create `docs/findings/2026-09-11-deployment-story.md`, modify `docs/plans/2026-09-11-deployment-story-ledger.md`, `.memories/` | Yes | Needs every earlier task's evidence. |

Commit mode: Batch A uses `parallel-lead-commit`. Tasks 6, 7, 8 run under `serial-worker-commit` or by the lead directly.

---

### Task 1: Release archives carry the sidecar

**Files:**
- Create: `.github/scripts/pack-release.sh`
- Modify: `.github/workflows/release.yml:15-21` (comment block), `:29-47` (matrix comments), `:88-103` (archive step), `:150-152` and `:194-206` (release notes boilerplate and verify step text)

**Interfaces:**
- Consumes: the four sidecar asset names and sha256 values from Global Constraints; `gh` is on every GitHub runner with `GH_TOKEN`.
- Produces: `.github/scripts/pack-release.sh <target> <version> <release-dir> <out-dir>` that writes `<out-dir>/julie-v<version>-<target>.tar.gz` (or `.zip` for `x86_64-pc-windows-msvc`) holding `julie-server[.exe]` and the sidecar files at the archive root. It prints the archive path as its last stdout line. Task 6 runs it locally.

**Contract inputs:** `run.cjs` archive naming from Global Constraints; the sidecar archive contents listed there.

**File ownership:** julie: create `.github/scripts/pack-release.sh`; modify `.github/workflows/release.yml`

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Move the archive step into a bash script the workflow and the local check both run. The script downloads the pinned sidecar asset for the target from `anortham/julie-semantic-sidecar` with `gh release download v0.1.0 --repo anortham/julie-semantic-sidecar --pattern <asset>` into a scratch dir, computes `sha256sum` and compares it with the pinned value in the script (a case block keyed by target holds asset name and sha256), unpacks it (`tar xzf` or `unzip`; Windows runners have both in Git Bash), drops the sidecar's `README.md` and renames its `LICENSE` to `LICENSE-julie-semantic-sidecar`, copies `julie-server[.exe]` from the release dir into the same staging dir, and packs the staging dir contents at the archive root with `tar -czf` or `7z a` on Windows (`7z a <archive> ./*` from inside the staging dir).

**Approach:** Pin `SIDECAR_VERSION=0.1.0` once at the top of the script. Use `set -euo pipefail`. Mismatched sha256 exits 1 with the expected and actual values on stderr. The workflow step becomes: `bash .github/scripts/pack-release.sh "${{ matrix.target }}" "${GITHUB_REF#refs/tags/v}" "target/${{ matrix.target }}/release" .` then `echo "ARCHIVE=..." >> $GITHUB_ENV`. Add `GH_TOKEN: ${{ github.token }}` to that step's env. Rewrite the stale comments in `release.yml` (lines 15-21 and 29-47 mention PyTorch, CUDA, MPS; say "sidecar bundled by pack-release.sh, metal on macOS, vulkan on Linux and Windows"). Update the boilerplate notes text at lines 150-152 and the verify step at 194-206 to say each archive ships `julie-server` and `julie-semantic-sidecar`. Add a `script-self-check`: the script accepts `--dry-run` that prints the resolved asset and sha256 without network, and the worker proves the mapping for all four targets with it. Run `actionlint` if `command -v actionlint` finds it, else `bash -n` on the script and a YAML parse (`python3 -c 'import yaml,sys; yaml.safe_load(open(".github/workflows/release.yml"))'`).

**Acceptance criteria:**
- [x] `.github/scripts/pack-release.sh --dry-run <target>` prints the pinned asset name and sha256 for each of the four targets and exits 0; an unknown target exits 2.
- [x] `bash .github/scripts/pack-release.sh x86_64-unknown-linux-gnu 8.0.0 <dir-with-julie-server> <out>` on this machine produces `julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz` whose listing has `julie-server`, `julie-semantic-sidecar`, `libggml-*.so*`, `THIRD_PARTY-LICENSES.html`, `LICENSE-julie-semantic-sidecar`, `package-manifest.json`, and no `README.md` (use `target/release/julie-server` from the main checkout's `target` symlink; the worker copies it to a scratch dir first).
- [x] `release.yml` calls the script for all four matrix targets, has no Python or CUDA text, and parses.
- [x] Worker-scope verification passes and the change is handed to the lead per commit mode.

### Task 2: Version 8.0.0 and release notes

**Files:**
- Modify: `Cargo.toml:15`, `Cargo.lock` (the `julie-server` package entry), `docs/site/index.html:784`
- Create: `docs/release-notes/v8.0.0.md`

**Interfaces:**
- Consumes: the machine-service history on this branch (`git log --oneline v7.18.1..HEAD`), the phase findings in `docs/findings/2026-09-11-*.md`, `docs/findings/2026-09-10-*.md`, and the brief `.memories/briefs/machine-service-one-rust-service-per-machine-repla.md`.
- Produces: `docs/release-notes/v8.0.0.md`, which `release.yml` picks up by name (`docs/release-notes/v${VERSION}.md`).

**Contract inputs:** Notes must state: the sidecar `julie-semantic-sidecar` 0.1.0 is inside every archive (metal on macOS, vulkan on Linux and Windows); install table for the six harnesses points at the README; plugin version 8.0.0.

**File ownership:** julie: modify `Cargo.toml`, `Cargo.lock`, `docs/site/index.html`; create `docs/release-notes/v8.0.0.md`

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Bump the version and write the notes by hand in the style of `docs/release-notes/v7.18.1.md` (read it first for the section shape), but with the depth CLAUDE.md asks for: user-visible changes, important fixes, dogfood and test evidence, upgrade notes, and caveats.

**Approach:** Sections: Summary; Machine service (one service per machine, stdio shim, `julie-server service status|stop|restart`, idle exit, dashboard, `$JULIE_HOME`); Semantic search by default (bundled sidecar, backfill runs in the background at about 10 vectors per second, `JULIE_EMBEDDING_PROVIDER`); Removed (the daemon, HTTP-URL registration in docs, the two edit tools deleted in phase 6a, `install-codex.cjs`); Index and registry hygiene (from `docs/findings/2026-09-11-index-hygiene*.md` if present, else the brief); Test tiers (`dev`, `dogfood`, `full`); Install (six harnesses, link to README); Upgrade notes (old `~/.julie` daemon state is ignored, first start reindexes each workspace, remove any old HTTP-URL MCP registration); Known caveats (Windows binary lock, first backfill time). Get the facts from the findings and git log, not from memory; cite the finding file per section in the worker report, not in the notes. `cargo check` after the version bump updates `Cargo.lock`; commit the lock change.

**Acceptance criteria:**
- [x] `Cargo.toml` says `version = "8.0.0"`, `Cargo.lock` has `julie-server` at 8.0.0, `docs/site/index.html` footer says `v8.0.0`, and `/usr/bin/grep -rn '7\.18\.1' Cargo.toml docs/site` prints nothing.
- [x] `docs/release-notes/v8.0.0.md` exists with the sections above and names the sidecar version 0.1.0.
- [x] `cargo check` passes.
- [x] Worker-scope verification passes and the change is handed to the lead per commit mode.

### Task 3: One instructions file in julie

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/hooks/session-start.cjs:7,21-26`, `CLAUDE.md` and `AGENTS.md` (Plugin Distribution section, lines 397-433, and the Dogfooding section item 2), `src/tests/cli_tools_tests.rs:659-675`, `xtask/tests/docs_contract_tests.rs:199-230`, `xtask/src/sync_plugin.rs:15-60` and its tests at `:250-400`
- Delete: `.claude/hooks/julie-routing-block.md`, `.claude/hooks/pretool-edit.cjs`, `.claude/hooks/pretool-agent.cjs`, `.agents/` (the whole directory; it holds only stale skill copies)
- Test: `src/tests/cli_tools_tests.rs`, `xtask/tests/docs_contract_tests.rs`, `xtask/src/sync_plugin.rs` tests

**Interfaces:**
- Consumes: the current routing block text (`.claude/hooks/julie-routing-block.md`) and instructions file.
- Produces: `JULIE_AGENT_INSTRUCTIONS.md` as the only routing text, under 2,000 bytes; `cargo xtask sync-plugin` copies it to `<plugin>/JULIE_AGENT_INSTRUCTIONS.md` (Task 6 relies on this); `.claude/hooks/session-start.cjs` prints it.

**Contract inputs:** the instructions file must keep every word the contract tests check: `patterns`, `regions`, `source_regions`, `structural_facts`, `complexity_metrics`, and the tool names plus `offset` and `dry_run`.

**File ownership:** julie: modify `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/hooks/session-start.cjs`, `CLAUDE.md`, `AGENTS.md`, `src/tests/cli_tools_tests.rs`, `xtask/tests/docs_contract_tests.rs`, `xtask/src/sync_plugin.rs`; delete `.claude/hooks/julie-routing-block.md`, `.claude/hooks/pretool-edit.cjs`, `.claude/hooks/pretool-agent.cjs`, `.agents/`

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Merge the routing block into the instructions file and delete the block. Add a `## Workflows` section with the four generic lines from the block (new task, flow tracing, change impact, bug fix, refactor; drop the extractor re-pin line, which is julie-dev only) and the sentence "Do not grep or find when Julie tools exist. Do not read a file without `get_symbols` first." Add `offset` to the paging sentence (the block's wording: "pass `offset` for that page"). Keep the file under 2,000 bytes; tighten tool lines if needed. The session-start hook reads `path.join(__dirname, '..', '..', 'JULIE_AGENT_INSTRUCTIONS.md')` instead of the routing block. `sync_plugin.rs`: `run_sync_plugin` also copies `JULIE_AGENT_INSTRUCTIONS.md` from the workspace root to `<plugin>/JULIE_AGENT_INSTRUCTIONS.md` (updated, unchanged, or dry-run preview), reported in the summary line; keep `diff_hooks` report-only. Tests: repoint `test_agent_instructions_recommend_standalone_for_quick_dogfood_checks` at `CLAUDE.md` with the two sentences moved there; rename `docs_contract_tests_routing_block_carries_the_full_guidance` to `docs_contract_tests_instructions_carry_the_full_guidance`, read `JULIE_AGENT_INSTRUCTIONS.md`, ceiling 2000 bytes, keep the hooks.json assertions; add one sync-plugin test that the instructions file is mirrored and one that dry-run leaves the plugin unchanged; the existing sync-plugin tests that mention `pretool-edit.cjs` use temp fixtures only and stay.

**Approach:** CLAUDE.md edits (mirror to AGENTS.md with `cp CLAUDE.md AGENTS.md`): in Dogfooding item 2 add "Standalone CLI does not prove MCP serving or handler binding. Use named wrappers such as `julie-server call-path FROM TO` for quick checks before live MCP; capture stderr `julie: mode=...` for execution-path evidence." In Plugin Distribution: the Hooks row says julie's `.claude/hooks/hooks.json` and the plugin's `hooks/hooks.json` both run a session-start hook that prints `JULIE_AGENT_INSTRUCTIONS.md`; the Agent instructions row says `JULIE_AGENT_INSTRUCTIONS.md` (mirrored by `cargo xtask sync-plugin`); "How distribution works" step 3 says skills and the instructions file; delete the sentence "cargo xtask sync-plugin reports hook divergence but does NOT auto-sync hooks (...)" and replace with "`cargo xtask sync-plugin` mirrors skills and `JULIE_AGENT_INSTRUCTIONS.md`; hooks stay separate and it reports their divergence." Check `git grep -n 'routing-block\|pretool-\|\.agents/skills'` returns only release notes, plans, memories, and findings afterwards.

**Acceptance criteria:**
- [ ] `wc -c JULIE_AGENT_INSTRUCTIONS.md` is under 2000 and the file has the `## Workflows` section.
- [ ] `node .claude/hooks/session-start.cjs session-start < /dev/null` prints JSON whose `additionalContext` equals the trimmed instructions file.
- [ ] `cargo nextest run --lib test_agent_instructions_recommend_standalone_for_quick_dogfood_checks` passes; `cargo nextest run -p xtask docs_contract_tests_instructions_carry_the_full_guidance` passes; the new sync-plugin tests pass.
- [ ] `cargo xtask sync-plugin --dry-run` lists `JULIE_AGENT_INSTRUCTIONS.md`.
- [ ] `.claude/hooks/julie-routing-block.md`, `pretool-edit.cjs`, `pretool-agent.cjs`, and `.agents/` do not exist; `cmp CLAUDE.md AGENTS.md` is silent.
- [ ] Worker-scope verification passes and the change is handed to the lead per commit mode.

### Task 4: README install paths

**Files:**
- Modify: `README.md:84-230` (Installation through the Claude Code manual block), `README.md:477-494` (Installing Skills table), `README.md:539-560` (CLI section)

**Interfaces:**
- Consumes: the plugin layout and commands from the design doc section 5 table; the verified harness config shapes from the design doc "What exists".
- Produces: the README install table that the plugin README (Task 5) and the release notes (Task 2) point at.

**Contract inputs:** README must keep the words `patterns`, `regions`, `source_regions`, `structural_facts`, `complexity_metrics`, and the three tier commands `cargo xtask test dev|dogfood|full` (contract tests `docs_contract_tests_agent_docs_and_readme_list_the_three_tiers` and `docs_contract_tests_extractor_enrichment_surfaces_are_documented`).

**File ownership:** julie: modify `README.md`

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Replace the Installation section with: one sentence that every path runs the same launcher and the machine service; a table with the six harnesses (Claude Code, Codex, Antigravity, OpenCode, Hermes, Cursor) giving the install commands or config block from the design doc section 5; a "Manual" subsection (download the archive, extract, register `julie-server` with no arguments as a stdio MCP server, `julie-server service status` to check); the `JULIE_WORKSPACE` client table stays. Delete "Streamable HTTP Registration", the Codex and OpenCode helper sections, and the VS Code and other client blocks that repeat the manual shape (keep one generic JSON stdio example). In Installing Skills, drop the `install-codex.cjs` mention; say Claude Code, Codex, and Antigravity get skills from the plugin, and OpenCode from `node bin/install-opencode.cjs`. In the CLI section add the `search "TODO" --regions comment,doc_comment --json` example and the `tool call_path --params '{"from":"...","to":"..."}' --json` example from the old routing block, and a sentence that `extract` is documented in External Extract.

**Approach:** Web-verify each harness block before writing it (design doc lists what was verified; re-check the commands: `codex plugin marketplace add`, `codex plugin add`, `agy plugin install`, OpenCode `mcp.<name>` local shape, Hermes `mcp_servers.<name>`, Cursor `mcpServers.<name>`), and put the URLs in the worker report. Config blocks use `/absolute/path/to/julie-plugin/hooks/run.cjs`. Keep the README's `## Retired` section and everything outside the three ranges untouched.

**Acceptance criteria:**
- [ ] README Installation lists exactly the six harnesses plus Manual, with no `http://127.0.0.1` text and no `install-codex`.
- [ ] `cargo nextest run -p xtask docs_contract_tests_agent_docs_and_readme_list_the_three_tiers` and `cargo nextest run -p xtask docs_contract_tests_extractor_enrichment_surfaces_are_documented` pass.
- [ ] The worker report lists one doc URL per harness block.
- [ ] Worker-scope verification passes and the change is handed to the lead per commit mode.

### Task 5: Plugin repo, one manifest per harness

**Files (all under `/home/murphy/source/julie-plugin`, branch `v8-deployment`):**
- Create: `plugin.json`, `.codex-plugin/plugin.json`, `mcp.json`, `mcp_config.json`, `.agents/plugins/marketplace.json`, `hooks/session-start.cjs`, `hooks/session-start.test.cjs`
- Modify: `.claude-plugin/plugin.json` (add `"hooks": "./hooks/hooks.json"`, description "36 languages"), `.claude-plugin/marketplace.json` (description "36 languages"), `hooks/hooks.json`, `hooks/installers-no-hooks.test.cjs` (drop the Codex case), `.github/workflows/update-binaries.yml:75-116` (copy `JULIE_AGENT_INSTRUCTIONS.md` at the tag; bump version in `plugin.json`, `.codex-plugin/plugin.json`, `.agents/plugins/marketplace.json` too), `README.md`, `CLAUDE.md`
- Delete: `bin/install-codex.cjs`
- Test: `hooks/session-start.test.cjs`, `hooks/installers-no-hooks.test.cjs`

**Interfaces:**
- Consumes: `hooks/run.cjs` unchanged; `JULIE_AGENT_INSTRUCTIONS.md` arrives at the plugin root from `cargo xtask sync-plugin` (Task 3) and from the workflow at release time.
- Produces: the layout in design doc section 3. `hooks/session-start.cjs <session-start|subagent-start>` prints `{"hookSpecificOutput":{"hookEventName":"SessionStart"|"SubagentStart","additionalContext":"<file text>"}}` and exits 0 silently when the file is missing or `JULIE_SESSION_HOOKS` is `0` or `false`.

**Contract inputs:** path forms from Global Constraints; hook JSON shape from `/home/murphy/source/julie/.worktrees/deployment-story/.claude/hooks/hooks.json` with `${CLAUDE_PLUGIN_ROOT}/hooks/session-start.cjs` as the command path; Codex uses the same hooks file (`"hooks": "./hooks/hooks.json"` in `.codex-plugin/plugin.json`).

**File ownership:** plugin repo, every file except `hooks/run.cjs`, `hooks/run.test.cjs`, `skills/`, `bin/archives/`

**Serialization required:** No

**Dependency reason:** None - safe parallel batch (separate repo).

**What to build:** The manifests: root `plugin.json` with `$schema` from the Agent Plugins standard, `name` `julie`, `version` `7.18.0`, `description`; `.codex-plugin/plugin.json` with `name`, `version`, `description`, `author`, `homepage`, `repository`, `license`, `skills: "./skills/"`, `hooks: "./hooks/hooks.json"`, `mcpServers` if the Codex doc shows it there, else rely on `mcp.json`; `mcp.json` `{"mcpServers":{"julie":{"type":"stdio","command":"node","args":["./hooks/run.cjs"]}}}`; `mcp_config.json` `{"mcpServers":{"julie":{"command":"node","args":["./hooks/run.cjs"]}}}`; `.agents/plugins/marketplace.json` with `name` `julie-plugin`, `plugins[0]` `name` `julie`, `source` `./`, `version`. Port `session-start.cjs` from the julie repo's `.claude/hooks/session-start.cjs` with the file path `path.join(__dirname, '..', 'JULIE_AGENT_INSTRUCTIONS.md')`. Tests with `node:test` (pattern in `hooks/run.test.cjs`): prints the file text for `session-start` and `subagent-start`, prints nothing when the file is missing, prints nothing with `JULIE_SESSION_HOOKS=0`, exits 0 on an unknown event. README: replace the `uv` prerequisite with "Nothing to install first: the archive carries the semantic sidecar"; the six-harness table from the design doc section 5 with the clone path for OpenCode, Hermes, Cursor; keep Supported Platforms and Project Structure (update the tree). CLAUDE.md: replace the "No behavior hooks" decision with the reason from the design doc section 3 (Claude Code shares about a 4 KB instruction budget across servers and truncates, issue anthropics/claude-code#43474; the hook prints the one shared file; `JULIE_SESSION_HOOKS=0` disables it) and update the file tree.

**Approach:** Web-verify each manifest shape before writing it and cite the URLs in the report: Agent Plugins standard (agent-plugins.org), Codex plugin docs, Antigravity plugin docs, Claude Code plugin reference for `hooks` in `plugin.json`. Do not touch `run.cjs`. Do not commit; the lead commits in this repo too (`parallel-lead-commit`), so leave the working tree with the edits and the deletion staged with `git rm`.

**Acceptance criteria:**
- [ ] `node --test hooks/session-start.test.cjs` and `node --test hooks/installers-no-hooks.test.cjs` and `node --test hooks/run.test.cjs` pass.
- [ ] Every JSON file in the list parses (`node -e 'JSON.parse(require("fs").readFileSync(f))'` per file) and `hooks/hooks.json` has `SessionStart` and `SubagentStart`.
- [ ] `bin/install-codex.cjs` is deleted; README has no `uv`, no `Python`, and lists the six harnesses; CLAUDE.md records the hook reason.
- [ ] The worker report lists one doc URL per manifest.
- [ ] Worker-scope verification passes and the change is handed to the lead per commit mode.

### Task 6: Local end-to-end through run.cjs

**Files:**
- Scratch only: `/tmp/claude-1000/-home-murphy-source-julie/6ac6eabe-d6ec-4831-a3a3-7c3de95237d6/scratchpad/e2e/` and the plugin's untracked `bin/archives/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz`
- Modify: none in either repo (the report goes to the SDD workspace; the lead copies the evidence into the finding in Task 8)

**Interfaces:**
- Consumes: `.github/scripts/pack-release.sh` (Task 1); `cargo xtask sync-plugin` (Task 3); plugin layout with `hooks/run.cjs` (Task 5).
- Produces: evidence lines for the finding: archive listing, `initialize` result, `fast_search` result, `service status` JSON with the embedding child ready, `ls bin/x86_64-unknown-linux-gnu/` in the plugin showing `julie-semantic-sidecar` beside `julie-server`.

**Contract inputs:** `run.cjs` extracts `bin/archives/julie-v*-x86_64-unknown-linux-gnu.tar.gz` into `bin/x86_64-unknown-linux-gnu/` and execs `julie-server` with no arguments, which is the stdio shim. The shim auto-starts the service from the same binary when `$JULIE_HOME/service.json` is missing.

**File ownership:** julie: none; plugin: `bin/archives/` (untracked scratch only); scratch dir

**Serialization required:** Yes

**Dependency reason:** Needs Task 1's pack script, Task 3's instructions file, and Task 5's plugin layout committed.

**What to build:** Run the release path on this machine. Steps: `cargo build --release` in the worktree (the `target` symlink points at the main checkout's target dir); copy `target/release/julie-server` to the scratch dir; run `bash .github/scripts/pack-release.sh x86_64-unknown-linux-gnu 8.0.0 <scratch> <plugin>/bin/archives`; in the julie worktree run `cargo xtask sync-plugin` so the plugin holds the current skills and `JULIE_AGENT_INSTRUCTIONS.md`; with `JULIE_HOME=<scratch>/home` and `JULIE_SERVICE_IDLE_SECS=300` and `JULIE_WORKSPACE=<julie worktree>` exported, pipe two JSON-RPC lines (`initialize` with protocol version `2025-06-18`, then `notifications/initialized`, then `tools/call` `fast_search` with `{"query":"find_default_sidecar_binary"}`) into `node <plugin>/hooks/run.cjs` and capture stdout; then run `<plugin>/bin/x86_64-unknown-linux-gnu/julie-server service status` with the same `JULIE_HOME` and wait until the embedding field reports ready (poll at most 60 s with a shell loop); finally `julie-server service stop` under that `JULIE_HOME`. Record every command and its output in the report.

**Approach:** Use `JULIE_HOME` so the packed binary starts its own service and does not attach to the developer service running from `target/debug`. Read `src/service/status` output fields with `julie-server service status` first to learn the exact embedding field name; do not guess it. Delete the scratch `JULIE_HOME` at the end. Leave the packed archive in the plugin's `bin/archives/` (git-ignored) for Task 7.

**Acceptance criteria:**
- [ ] The packed archive lists `julie-server` and `julie-semantic-sidecar` at the root.
- [ ] `run.cjs` extracted both beside each other in `<plugin>/bin/x86_64-unknown-linux-gnu/`.
- [ ] `initialize` returned `serverInfo.version` `8.0.0` and the instructions text; `fast_search` returned at least one result.
- [ ] `service status` reported the embedding child ready before the 60 s limit.
- [ ] The scratch service is stopped and the report holds every command and output.

### Task 7: Six harness install checks (lead)

**Files:**
- Modify (backed up and restored): `~/.config/opencode/opencode.json`, `~/.hermes/config.yaml`, `~/.cursor/mcp.json`; Codex and Antigravity and Claude Code plugin installs from the local checkout, removed after the check
- Repo files: none

**Interfaces:**
- Consumes: the plugin checkout with the packed archive from Task 6.
- Produces: one result line per harness for the finding: install command used, whether the harness started Julie, and whether one `fast_search` answered.

**Contract inputs:** the design doc section 5 install commands; the harness CLIs are installed here (`codex`, `agy`, `opencode`, `hermes`, `cursor`, `claude`).

**File ownership:** user harness configs (backed up and restored); no repo files

**Serialization required:** Yes

**Dependency reason:** Needs Task 6's packed archive in the plugin checkout.

**What to build:** For each harness: back up the config file to the scratch dir; add Julie from the local plugin checkout with the documented command or block; run one non-interactive prompt that calls `fast_search` (`claude -p --plugin-dir <plugin> ...`, `codex exec ...`, `opencode run ...`, `hermes ...`, `agy ...`; read each CLI's `--help` first for the exact non-interactive flag); record the result; remove the install and restore the backup. Cursor has no headless prompt path: record that its check is the config block validation (JSON parses, `cursor --help` confirms the config path) and note it as manual in the finding.

**Approach:** `JULIE_HOME` stays default here so the harness sessions use the developer service, which is fine: the check proves the launcher and the manifests, Task 6 proved the packed binary. Never edit the owner's repo-local `.codex/config.toml`. If a harness cannot be driven non-interactively, record "manual" with the exact steps the owner runs.

**Acceptance criteria:**
- [ ] Six result lines, each with the command, the outcome, and the doc URL for the install path.
- [ ] Every backed-up config is restored byte for byte (`cmp`), and no plugin install remains.

### Task 8: Finding, ledger, gate (lead)

**Files:**
- Create: `docs/findings/2026-09-11-deployment-story.md`
- Modify: `docs/plans/2026-09-11-deployment-story-ledger.md` (create from the template at the start of the run), plan checkboxes, `.memories/` checkpoint

**Interfaces:**
- Consumes: all task reports and the ledger rows.
- Produces: the finding with the owner runbook (design doc section 6) and the evidence; the brief update.

**Contract inputs:** the design doc acceptance criteria list; CLAUDE.md release-notes rule (`gh release edit <tag> --notes-file <file>` if the workflow page is generic).

**File ownership:** julie: create `docs/findings/2026-09-11-deployment-story.md`, modify the ledger, `.memories/`

**Serialization required:** Yes

**Dependency reason:** Needs every earlier task's evidence.

**What to build:** Run `cargo xtask test dev` in the worktree and `node --test hooks/*.test.cjs` in the plugin at the final HEADs; record both in the ledger. Write the finding: verdict, what shipped (per design section), the end-to-end evidence (Task 6), the six harness results (Task 7), the owner runbook as a numbered checklist (push main; `git tag v8.0.0`; push the tag; wait for `release.yml`; check the release page body equals `docs/release-notes/v8.0.0.md`; in the plugin repo push `v8-deployment` and merge it; run `update-binaries.yml` with `version=8.0.0 tag=v8.0.0`; review and push; fresh install on one machine; `julie-server service status` shows the embedding child ready after one search), deferred items, and side findings. Update the brief: item 4 done, next item per the owner sequence. Run razorback:finishing-a-development-branch afterwards; the owner decides the merge.

**Acceptance criteria:**
- [ ] Ledger has `dev` and plugin-tests rows at the final HEADs, both passed.
- [ ] Finding exists with the runbook and the evidence for every design acceptance criterion.
- [ ] Brief updated; checkpoint committed with the finding.
