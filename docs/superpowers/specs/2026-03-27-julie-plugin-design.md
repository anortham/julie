# Julie Claude Code Plugin Design

**Date:** 2026-03-27
**Status:** Approved
**Repo:** https://github.com/anortham/julie-plugin

## Problem

Julie is a compiled Rust binary distributed via GitHub releases. Users who want Julie's code intelligence in Claude Code currently need to manually download the binary, configure MCP settings, and copy skills into their project. The IU team is adopting Claude Code and needs a frictionless install path.

Additionally, Claude Code's 2k character limit on MCP tool descriptions forced Julie's server instructions to be cut by ~4x. Behavioral guidance that previously lived in tool descriptions (e.g., "always use get_symbols before reading files") no longer fits and needs a new delivery mechanism.

## Solution

A separate `anortham/julie-plugin` repository that packages pre-built binaries, skills, and a SessionStart hook into a Claude Code plugin. Users install it like any other plugin. The julie source repo remains the single source of truth; the plugin repo is a pure distribution artifact updated by automation.

## Repository Structure

```
julie-plugin/
├── .claude-plugin/
│   └── plugin.json
├── bin/
│   ├── aarch64-apple-darwin/
│   │   └── julie-server
│   ├── x86_64-unknown-linux-gnu/
│   │   └── julie-server
│   └── x86_64-pc-windows-msvc/
│       └── julie-server.exe
├── hooks/
│   ├── hooks.json
│   ├── run-hook.cmd
│   ├── launch
│   └── session-start
├── skills/
│   ├── architecture/SKILL.md
│   ├── call-trace/SKILL.md
│   ├── dependency-graph/SKILL.md
│   ├── explore-area/SKILL.md
│   ├── impact-analysis/SKILL.md
│   ├── logic-flow/SKILL.md
│   ├── metrics/SKILL.md
│   └── type-flow/SKILL.md
├── package.json
├── LICENSE
└── README.md
```

## Plugin Manifest

`.claude-plugin/plugin.json`:

```json
{
  "name": "julie",
  "description": "Code intelligence server: search, navigation, and refactoring across 33 languages",
  "version": "6.1.6",
  "author": { "name": "Alan Northam" },
  "repository": "https://github.com/anortham/julie-plugin",
  "homepage": "https://github.com/anortham/julie",
  "license": "MIT",
  "keywords": [
    "code-intelligence", "search", "navigation",
    "refactoring", "tree-sitter", "mcp"
  ],
  "mcpServers": {
    "julie": {
      "command": "${CLAUDE_PLUGIN_ROOT}/hooks/run-hook.cmd",
      "args": ["launch"]
    }
  }
}
```

**Versioning:** Plugin version tracks the julie binary version exactly. When julie releases v6.1.7, the plugin becomes v6.1.7. One version, no confusion.

**MCP server name:** The `"julie"` key in `mcpServers` means Claude Code registers tools as `mcp__julie__*`, which matches the tool references in the skills' `allowed-tools` frontmatter.

## Binary Distribution

All three platform binaries are embedded directly in the plugin repo under `bin/<target>/`. Approximate sizes:

| Platform | Target | Binary | ~Size |
|----------|--------|--------|-------|
| macOS Apple Silicon | aarch64-apple-darwin | julie-server | ~20MB |
| Linux x86_64 | x86_64-unknown-linux-gnu | julie-server | ~20MB |
| Windows x86_64 | x86_64-pc-windows-msvc | julie-server.exe | ~20MB |

**Why embed instead of download-on-first-run:**
- No network dependency at session start (works behind university firewalls, proxies, air-gapped environments)
- No bootstrap script complexity or cross-platform download tool differences
- Claude Code caches plugins as version snapshots, so git history bloat is irrelevant
- ~60MB total is acceptable for a version snapshot

## Launch Script

`hooks/launch` detects the platform and `exec`s the correct binary:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}-${ARCH}" in
    Darwin-arm64)
        TARGET="aarch64-apple-darwin"
        BINARY="${PLUGIN_ROOT}/bin/${TARGET}/julie-server"
        ;;
    Linux-x86_64)
        TARGET="x86_64-unknown-linux-gnu"
        BINARY="${PLUGIN_ROOT}/bin/${TARGET}/julie-server"
        ;;
    MINGW*-x86_64|MSYS*-x86_64)
        TARGET="x86_64-pc-windows-msvc"
        BINARY="${PLUGIN_ROOT}/bin/${TARGET}/julie-server.exe"
        ;;
    *)
        echo "Unsupported platform: ${OS}-${ARCH}" >&2
        exit 1
        ;;
esac

if [ ! -x "$BINARY" ]; then
    echo "Julie binary not found: ${BINARY}" >&2
    exit 1
fi

exec "$BINARY" "$@"
```

**`exec` is critical:** It replaces the shell process with julie-server so stdio pipes directly between the MCP client and the binary. No intermediary process for signal handling or process management.

**Windows path:** The `run-hook.cmd` polyglot (borrowed from the superpowers plugin pattern) finds bash on Windows (Git Bash, MSYS2, Cygwin) and delegates to this same script.

**Testing note:** The polyglot wrapper is proven for fire-and-forget hooks (superpowers uses it), but using it for a long-running MCP server with stdio piping is new. Verify on Windows that stdin/stdout flow correctly through the cmd.exe -> bash -> exec chain.

## Cross-Platform Polyglot (`run-hook.cmd`)

Adapted from the superpowers plugin. A single file that works as both a Windows batch script and a Unix shell script:

- **Windows:** cmd.exe runs the batch portion, which locates bash (Git for Windows, MSYS2, or PATH) and delegates
- **Unix:** The shell interprets the batch portion as a no-op heredoc, then runs the bash portion with `exec`

This file is used for both the MCP server launch and the SessionStart hook.

## SessionStart Hook

### Registration

`hooks/hooks.json`:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|clear|compact",
        "hooks": [
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/hooks/run-hook.cmd\" session-start",
            "async": false
          }
        ]
      }
    ]
  }
}
```

**Matcher:** Fires on session startup, context clear, and context compaction. This ensures behavioral guidance survives long sessions where context gets compressed.

**Async: false:** The hook blocks until complete so the guidance is injected before Claude starts responding.

### Behavioral Guidance Content

The `hooks/session-start` script outputs JSON with behavioral rules via `hookSpecificOutput.additionalContext`. Content:

```
You have Julie, a code intelligence MCP server. Follow these rules:

1. **Search before coding**: Always fast_search before writing new code.
2. **Structure before reading**: Always get_symbols before Read (70-90% token savings).
3. **References before changes**: Always fast_refs before modifying any symbol.
4. **Deep dive for understanding**: Use deep_dive when you need to understand
   a symbol's full context (callers, callees, types) before modifying it.
5. **Trust results**: Pre-indexed and accurate. Never verify with grep/find/Read.

Don't use grep/find when Julie tools are available.
Don't read files without get_symbols first.
```

**Design rationale for the rules:**
- Each rule promotes the right tool for each step rather than pushing everything through `deep_dive`. Testing showed that surgical chains (fast_search -> get_symbols) are leaner and more token-efficient than monolithic calls.
- Rule 4 scopes `deep_dive` to its actual use case (understanding full context before modification) rather than framing it as a replacement for targeted tools.

### Script Structure

`hooks/session-start` follows the superpowers pattern:
1. Read the guidance content
2. Escape for JSON embedding
3. Output platform-appropriate JSON (`hookSpecificOutput.additionalContext` for Claude Code, `additional_context` for Cursor)

## Skills

### Included (8 skills)

All skills are agent-guidance workflows that teach Claude Code how to use Julie's tools effectively for common code intelligence tasks:

| Skill | Purpose | Key Tools |
|-------|---------|-----------|
| architecture | Generate architecture overview with entry points, module map, dependency flow | deep_dive, get_context, get_symbols, fast_search |
| call-trace | Trace call paths between two functions by following callers/callees | deep_dive, fast_refs, fast_search, get_context |
| dependency-graph | Show module dependencies by analyzing imports/exports/cross-references | deep_dive, fast_refs, get_symbols |
| explore-area | Orient on unfamiliar codebase areas with token-budgeted exploration | get_context, deep_dive, get_symbols |
| impact-analysis | Analyze blast radius of changing a symbol, grouped by risk level | fast_refs, deep_dive, get_context |
| logic-flow | Explain function logic step-by-step with control flow and decision points | deep_dive, get_symbols, fast_refs |
| metrics | Show operational metrics: session stats, tool usage, context efficiency | query_metrics |
| type-flow | Trace type transformations through function parameters and return types | deep_dive, get_symbols, fast_refs |

### Excluded

- **search-debug**: Replaced by the dashboard's search sandbox. Only useful for Julie development/dogfooding.

### Source of Truth

Skills are authored and tested in the julie repo's `.claude/skills/` directory. The plugin repo never has manual skill edits. The release automation copies them verbatim.

The tool names in skill `allowed-tools` frontmatter (e.g., `mcp__julie__deep_dive`) match the MCP server name `"julie"` declared in `plugin.json`, so they work without rewriting.

## Release Automation

### Trigger

The julie repo's `release.yml` workflow adds a final step after publishing the GitHub release:

```yaml
- name: Trigger plugin update
  env:
    GH_TOKEN: ${{ secrets.PLUGIN_REPO_TOKEN }}
  run: |
    gh workflow dispatch update-binaries \
      --repo anortham/julie-plugin \
      --field version="${{ steps.version.outputs.version }}" \
      --field tag="v${{ steps.version.outputs.version }}"
```

**Requires:** A fine-grained personal access token with `actions:write` scope on `anortham/julie-plugin`, stored as `PLUGIN_REPO_TOKEN` secret in the julie repo.

### Plugin Update Workflow

`anortham/julie-plugin/.github/workflows/update-binaries.yml`:

1. **Receive** version and tag via `workflow_dispatch` inputs
2. **Download** the three platform archives from the julie GitHub release
3. **Extract** binaries into `bin/<target>/`
4. **Clone** the julie repo at the release tag
5. **Copy skills** from `julie/.claude/skills/` into `skills/`, excluding `search-debug/`
6. **Update version** in `.claude-plugin/plugin.json` and `package.json`
7. **Commit** with message `feat: update to julie v{version}`
8. **Tag** with `v{version}`
9. **Push** commit and tag

### What This Means

- The julie repo is the single source of truth for binaries, skills, and version
- The plugin repo is a pure distribution artifact
- A julie release automatically produces a matching plugin release with zero manual steps
- The plugin repo's git history is a clean sequence of automated updates

## Package Metadata

`package.json` (required by the plugin system):

```json
{
  "name": "julie",
  "version": "6.1.6",
  "type": "module"
}
```

Updated by automation alongside `plugin.json` on each release.

## Open Questions

1. **Windows stdio through polyglot wrapper**: The `run-hook.cmd` -> bash -> exec chain for a long-running MCP server is untested. Verify stdio piping works correctly on Windows before shipping.

2. **macOS codesigning**: The release workflow does ad-hoc codesigning. Users may get Gatekeeper warnings when the binary runs. Since the user has an Apple Developer subscription, proper signing and notarization could be added to the release workflow later.

3. **Plugin marketplace submission**: Initial distribution is via GitHub repo URL (`claude plugin add anortham/julie-plugin`). Marketplace submission can happen later once the plugin is validated with real users.

4. **Git LFS for binaries**: At ~60MB total, the binaries are manageable without LFS. If the binary sizes grow significantly, LFS could be added to keep clone times reasonable. Monitor this.
