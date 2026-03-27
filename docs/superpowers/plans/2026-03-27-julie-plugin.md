# Julie Claude Code Plugin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `anortham/julie-plugin` repo that packages Julie's pre-built binaries, skills, and behavioral hooks as a Claude Code plugin.

**Architecture:** A distribution-only repo containing platform-specific binaries under `bin/<target>/`, a polyglot launcher for cross-platform MCP server startup, a SessionStart hook for behavioral guidance injection, and 8 agent-guidance skills. Automated via GitHub Actions cross-repo dispatch from the julie source repo.

**Tech Stack:** Bash (scripts), JSON (manifests), YAML (GitHub Actions), GitHub CLI (`gh`)

**Spec:** `docs/superpowers/specs/2026-03-27-julie-plugin-design.md`

---

### Task 1: Bootstrap Plugin Repo

**Context:** The repo `anortham/julie-plugin` exists on GitHub but is empty. We need to clone it and create the foundational files.

**Files:**
- Create: `julie-plugin/.claude-plugin/plugin.json`
- Create: `julie-plugin/package.json`
- Create: `julie-plugin/LICENSE`
- Create: `julie-plugin/.gitignore`

- [ ] **Step 1: Clone the empty repo**

```bash
cd ~/source
git clone https://github.com/anortham/julie-plugin.git
cd julie-plugin
```

- [ ] **Step 2: Create directory structure**

```bash
mkdir -p .claude-plugin bin/aarch64-apple-darwin bin/x86_64-unknown-linux-gnu bin/x86_64-pc-windows-msvc hooks skills
```

- [ ] **Step 3: Create `.claude-plugin/plugin.json`**

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

- [ ] **Step 4: Create `package.json`**

```json
{
  "name": "julie",
  "version": "6.1.6",
  "type": "module"
}
```

- [ ] **Step 5: Create `LICENSE`**

Use MIT license with copyright "2025 Alan Northam". Copy from the julie source repo:

```bash
cp ~/source/julie/LICENSE ~/source/julie-plugin/LICENSE
```

- [ ] **Step 6: Create `.gitignore`**

```
.DS_Store
*.log
```

Note: Do NOT gitignore `bin/` since we want the binaries committed.

- [ ] **Step 7: Verify structure**

```bash
find . -not -path './.git/*' -not -name '.git' | sort
```

Expected output should show the directory tree matching the spec.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: bootstrap plugin repo with manifest and metadata"
```

---

### Task 2: Cross-Platform Polyglot Launcher

**Context:** The `run-hook.cmd` file is a polyglot that works as both a Windows batch script and a Unix shell script. It's the entry point for both the MCP server launch and the SessionStart hook. Adapted from the superpowers plugin pattern.

**Files:**
- Create: `hooks/run-hook.cmd`

- [ ] **Step 1: Create `hooks/run-hook.cmd`**

```bash
: << 'CMDBLOCK'
@echo off
REM Cross-platform polyglot wrapper for Julie plugin scripts.
REM On Windows: cmd.exe runs the batch portion, which finds and calls bash.
REM On Unix: the shell interprets this as a script (: is a no-op in bash).
REM
REM Usage: run-hook.cmd <script-name> [args...]

if "%~1"=="" (
    echo run-hook.cmd: missing script name >&2
    exit /b 1
)

set "HOOK_DIR=%~dp0"

REM Try Git for Windows bash in standard locations
if exist "C:\Program Files\Git\bin\bash.exe" (
    "C:\Program Files\Git\bin\bash.exe" "%HOOK_DIR%%~1" %2 %3 %4 %5 %6 %7 %8 %9
    exit /b %ERRORLEVEL%
)
if exist "C:\Program Files (x86)\Git\bin\bash.exe" (
    "C:\Program Files (x86)\Git\bin\bash.exe" "%HOOK_DIR%%~1" %2 %3 %4 %5 %6 %7 %8 %9
    exit /b %ERRORLEVEL%
)

REM Try bash on PATH (e.g. user-installed Git Bash, MSYS2, Cygwin)
where bash >nul 2>nul
if %ERRORLEVEL% equ 0 (
    bash "%HOOK_DIR%%~1" %2 %3 %4 %5 %6 %7 %8 %9
    exit /b %ERRORLEVEL%
)

REM No bash found
echo Julie plugin requires bash (Git for Windows, MSYS2, or WSL) >&2
exit /b 1
CMDBLOCK

# Unix: run the named script directly
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCRIPT_NAME="$1"
shift
exec bash "${SCRIPT_DIR}/${SCRIPT_NAME}" "$@"
```

- [ ] **Step 2: Make executable**

```bash
chmod +x hooks/run-hook.cmd
```

- [ ] **Step 3: Verify it dispatches correctly on macOS**

```bash
# Create a tiny test script
echo '#!/usr/bin/env bash
echo "dispatch works"' > hooks/test-dispatch
chmod +x hooks/test-dispatch

# Run through the polyglot
bash hooks/run-hook.cmd test-dispatch
```

Expected output: `dispatch works`

- [ ] **Step 4: Clean up test script and commit**

```bash
rm hooks/test-dispatch
git add hooks/run-hook.cmd
git commit -m "feat: add cross-platform polyglot launcher"
```

---

### Task 3: Launch Script

**Context:** The `hooks/launch` script is the MCP server entry point. It detects the platform, resolves the binary path, and `exec`s it. The `exec` is critical so stdio pipes directly between the MCP client and julie-server with no intermediary.

**Files:**
- Create: `hooks/launch`

- [ ] **Step 1: Create `hooks/launch`**

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
    echo "Platform detected: ${OS}-${ARCH} -> ${TARGET}" >&2
    exit 1
fi

exec "$BINARY" "$@"
```

- [ ] **Step 2: Make executable**

```bash
chmod +x hooks/launch
```

- [ ] **Step 3: Verify platform detection**

Test that it detects the right platform (will fail on binary check since we haven't placed binaries yet, but the error message confirms detection):

```bash
bash hooks/launch 2>&1 || true
```

Expected output on macOS ARM: `Julie binary not found: .../bin/aarch64-apple-darwin/julie-server`
This confirms the platform detection works. The binary will be placed in Task 6.

- [ ] **Step 4: Verify it works through the polyglot**

```bash
bash hooks/run-hook.cmd launch 2>&1 || true
```

Expected: Same output as Step 3, confirming the full chain works.

- [ ] **Step 5: Commit**

```bash
git add hooks/launch
git commit -m "feat: add platform-detecting launch script for MCP server"
```

---

### Task 4: SessionStart Hook

**Context:** The SessionStart hook injects behavioral guidance into Claude Code sessions. It compensates for the 2k character limit on MCP tool descriptions by injecting rules like "use get_symbols before reading files" at session start, context clear, and context compaction. Follows the superpowers plugin pattern.

**Files:**
- Create: `hooks/session-start`
- Create: `hooks/hooks.json`

- [ ] **Step 1: Create `hooks/hooks.json`**

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

- [ ] **Step 2: Create `hooks/session-start`**

```bash
#!/usr/bin/env bash
set -euo pipefail

# Behavioral guidance for Julie code intelligence tools.
# Injected at session start, context clear, and context compaction
# to compensate for the 2k MCP tool description character limit.
GUIDANCE="You have Julie, a code intelligence MCP server. Follow these rules:\n\n1. **Search before coding**: Always fast_search before writing new code.\n2. **Structure before reading**: Always get_symbols before Read (70-90% token savings).\n3. **References before changes**: Always fast_refs before modifying any symbol.\n4. **Deep dive for understanding**: Use deep_dive when you need to understand a symbol's full context (callers, callees, types) before modifying it.\n5. **Trust results**: Pre-indexed and accurate. Never verify with grep/find/Read.\n\nDon't use grep/find when Julie tools are available.\nDon't read files without get_symbols first."

# Escape string for JSON embedding using bash parameter substitution.
escape_for_json() {
    local s="$1"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/\\n}"
    s="${s//$'\r'/\\r}"
    s="${s//$'\t'/\\t}"
    printf '%s' "$s"
}

escaped=$(escape_for_json "$GUIDANCE")

# Output platform-appropriate JSON.
# Claude Code reads hookSpecificOutput.additionalContext.
# Cursor reads additional_context.
if [ -n "${CURSOR_PLUGIN_ROOT:-}" ]; then
    printf '{\n  "additional_context": "%s"\n}\n' "$escaped"
elif [ -n "${CLAUDE_PLUGIN_ROOT:-}" ]; then
    printf '{\n  "hookSpecificOutput": {\n    "hookEventName": "SessionStart",\n    "additionalContext": "%s"\n  }\n}\n' "$escaped"
else
    printf '{\n  "additional_context": "%s"\n}\n' "$escaped"
fi

exit 0
```

- [ ] **Step 3: Make executable**

```bash
chmod +x hooks/session-start
```

- [ ] **Step 4: Verify JSON output is valid**

```bash
# Run with CLAUDE_PLUGIN_ROOT set to simulate Claude Code environment
CLAUDE_PLUGIN_ROOT=/tmp bash hooks/session-start | python3 -m json.tool > /dev/null && echo "Valid JSON"
```

Expected output: `Valid JSON`

- [ ] **Step 5: Verify guidance content is present in output**

```bash
CLAUDE_PLUGIN_ROOT=/tmp bash hooks/session-start | python3 -c "
import json, sys
data = json.load(sys.stdin)
ctx = data['hookSpecificOutput']['additionalContext']
assert 'fast_search' in ctx, 'Missing fast_search rule'
assert 'get_symbols' in ctx, 'Missing get_symbols rule'
assert 'fast_refs' in ctx, 'Missing fast_refs rule'
assert 'deep_dive' in ctx, 'Missing deep_dive rule'
print('All rules present')
"
```

Expected output: `All rules present`

- [ ] **Step 6: Verify it works through the polyglot**

```bash
CLAUDE_PLUGIN_ROOT=/tmp bash hooks/run-hook.cmd session-start | python3 -m json.tool > /dev/null && echo "Polyglot chain OK"
```

Expected output: `Polyglot chain OK`

- [ ] **Step 7: Commit**

```bash
git add hooks/hooks.json hooks/session-start
git commit -m "feat: add SessionStart hook for behavioral guidance injection"
```

---

### Task 5: Copy Skills

**Context:** Julie has 8 agent-guidance skills in its source repo that teach Claude Code how to use Julie's tools effectively. These are copied verbatim into the plugin. The `search-debug` skill is excluded (replaced by the dashboard search sandbox).

**Files:**
- Create: `skills/architecture/SKILL.md`
- Create: `skills/call-trace/SKILL.md`
- Create: `skills/dependency-graph/SKILL.md`
- Create: `skills/explore-area/SKILL.md`
- Create: `skills/impact-analysis/SKILL.md`
- Create: `skills/logic-flow/SKILL.md`
- Create: `skills/metrics/SKILL.md`
- Create: `skills/type-flow/SKILL.md`

- [ ] **Step 1: Copy skills from julie source repo**

```bash
cd ~/source/julie-plugin

for skill in architecture call-trace dependency-graph explore-area \
             impact-analysis logic-flow metrics type-flow; do
    mkdir -p "skills/${skill}"
    cp -r ~/source/julie/.claude/skills/${skill}/* "skills/${skill}/"
done
```

- [ ] **Step 2: Verify all 8 skills are present with SKILL.md files**

```bash
ls skills/*/SKILL.md | wc -l
```

Expected output: `8`

- [ ] **Step 3: Verify tool references use `mcp__julie__` prefix**

The skills reference tools as `mcp__julie__fast_search`, etc. The `"julie"` key in our `plugin.json` mcpServers means Claude Code will register them with this exact prefix. Verify alignment:

```bash
grep -h 'allowed-tools:' skills/*/SKILL.md
```

Every line should contain `mcp__julie__` prefixed tool names. If any skill uses a different prefix, it needs updating.

- [ ] **Step 4: Verify search-debug is NOT included**

```bash
ls skills/search-debug/SKILL.md 2>/dev/null && echo "ERROR: search-debug should not be included" || echo "OK: search-debug excluded"
```

Expected output: `OK: search-debug excluded`

- [ ] **Step 5: Commit**

```bash
git add skills/
git commit -m "feat: add 8 agent-guidance skills from julie source repo"
```

---

### Task 6: Add Platform Binaries

**Context:** Download the v6.1.6 release binaries from `anortham/julie` and place them in the correct directories. The release archives are named `julie-v{version}-{target}.{ext}`.

**Files:**
- Create: `bin/aarch64-apple-darwin/julie-server`
- Create: `bin/x86_64-unknown-linux-gnu/julie-server`
- Create: `bin/x86_64-pc-windows-msvc/julie-server.exe`

- [ ] **Step 1: Download release archives**

```bash
cd ~/source/julie-plugin

gh release download v6.1.6 \
    --repo anortham/julie \
    --pattern "julie-v6.1.6-*.tar.gz" \
    --pattern "julie-v6.1.6-*.zip" \
    --dir /tmp/julie-artifacts
```

If `gh` doesn't have access, download manually from:
`https://github.com/anortham/julie/releases/tag/v6.1.6`

- [ ] **Step 2: Extract macOS binary**

```bash
tar -xzf /tmp/julie-artifacts/julie-v6.1.6-aarch64-apple-darwin.tar.gz \
    -C bin/aarch64-apple-darwin/
chmod +x bin/aarch64-apple-darwin/julie-server
```

- [ ] **Step 3: Extract Linux binary**

```bash
tar -xzf /tmp/julie-artifacts/julie-v6.1.6-x86_64-unknown-linux-gnu.tar.gz \
    -C bin/x86_64-unknown-linux-gnu/
chmod +x bin/x86_64-unknown-linux-gnu/julie-server
```

- [ ] **Step 4: Extract Windows binary**

```bash
unzip -o /tmp/julie-artifacts/julie-v6.1.6-x86_64-pc-windows-msvc.zip \
    -d bin/x86_64-pc-windows-msvc/
```

- [ ] **Step 5: Verify all three binaries exist**

```bash
ls -lh bin/aarch64-apple-darwin/julie-server \
       bin/x86_64-unknown-linux-gnu/julie-server \
       bin/x86_64-pc-windows-msvc/julie-server.exe
```

All three should be present and ~15-25MB each.

- [ ] **Step 6: Test launch script end-to-end (macOS)**

Now that the binary is in place, the launch script should work:

```bash
# Start julie-server and immediately send it EOF to verify it launches
echo '' | timeout 5 bash hooks/launch 2>/dev/null; echo "Exit code: $?"
```

The exit code should be 0 or a clean shutdown code (not 127/file-not-found).

- [ ] **Step 7: Test launch through the polyglot**

```bash
echo '' | timeout 5 bash hooks/run-hook.cmd launch 2>/dev/null; echo "Exit code: $?"
```

Same expectation as Step 6.

- [ ] **Step 8: Clean up download artifacts**

```bash
rm -rf /tmp/julie-artifacts
```

- [ ] **Step 9: Commit**

Binaries are large, so this commit will be ~60MB:

```bash
git add bin/
git commit -m "feat: add v6.1.6 platform binaries (macOS, Linux, Windows)"
```

---

### Task 7: README

**Context:** The README is the first thing users see. It should explain what Julie is, how to install the plugin, and what tools become available.

**Files:**
- Create: `README.md`

- [ ] **Step 1: Create `README.md`**

Write the following content to `README.md`. Note: the install section should contain a bash code block with the install command.

````markdown
# Julie - Code Intelligence Plugin for Claude Code

Julie is a code intelligence server that gives Claude Code LSP-quality search,
navigation, and refactoring across 33 programming languages. It uses tree-sitter
for parsing and Tantivy for full-text search to provide instant, accurate results.

## Install

```bash
claude plugin add anortham/julie-plugin
```

## What You Get

### MCP Tools

Julie registers as an MCP server providing these tools:

| Tool | Purpose |
|------|---------|
| `fast_search` | Find code by text or symbol name |
| `get_symbols` | File structure without reading full content |
| `deep_dive` | Full symbol investigation: definition, callers, callees, types |
| `fast_refs` | All references to a symbol |
| `get_context` | Token-budgeted area orientation |
| `rename_symbol` | Workspace-wide rename with preview |
| `manage_workspace` | Index management and health checks |
| `query_metrics` | Session stats, tool usage, and efficiency metrics |

### Skills

Skills teach Claude Code how to combine Julie's tools for common tasks:

| Skill | What it does |
|-------|-------------|
| `/explore-area` | Orient on an unfamiliar area of the codebase |
| `/call-trace` | Trace the call path between two functions |
| `/impact-analysis` | Analyze what breaks if a symbol changes |
| `/dependency-graph` | Show module import/export relationships |
| `/logic-flow` | Step through a function's control flow |
| `/type-flow` | Trace type transformations through a pipeline |
| `/architecture` | Generate an architecture overview |
| `/metrics` | Show Julie's session stats and efficiency |

## Supported Languages (33)

**Core:** Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin, Scala
**Systems:** C, C++, Go, Lua, Zig
**Functional:** Elixir
**Specialized:** GDScript, Vue, Razor, QML, R, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart
**Documentation:** Markdown, JSON, TOML, YAML

## Supported Platforms

| Platform | Architecture |
|----------|-------------|
| macOS | Apple Silicon (ARM64) |
| Linux | x86_64 |
| Windows | x86_64 |

## How It Works

Julie runs as a background daemon that indexes your codebase using tree-sitter
parsers and Tantivy full-text search. The first time you open a project, Julie
indexes it automatically. Subsequent sessions use the cached index with
incremental updates for changed files.

## More Information

- [Julie source repo](https://github.com/anortham/julie) - full documentation and development
- [Architecture docs](https://github.com/anortham/julie/blob/main/docs/ARCHITECTURE.md)
````

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add README with install instructions and tool reference"
```

---

### Task 8: Push Initial Plugin to GitHub

**Context:** All plugin files are ready. Push to the remote to make the repo installable.

- [ ] **Step 1: Verify the full directory structure**

```bash
find . -not -path './.git/*' -not -name '.git' -type f | sort
```

Expected files:
```
./.claude-plugin/plugin.json
./.gitignore
./LICENSE
./README.md
./bin/aarch64-apple-darwin/julie-server
./bin/x86_64-unknown-linux-gnu/julie-server
./bin/x86_64-pc-windows-msvc/julie-server.exe
./hooks/hooks.json
./hooks/launch
./hooks/run-hook.cmd
./hooks/session-start
./package.json
./skills/architecture/SKILL.md
./skills/call-trace/SKILL.md
./skills/dependency-graph/SKILL.md
./skills/explore-area/SKILL.md
./skills/impact-analysis/SKILL.md
./skills/logic-flow/SKILL.md
./skills/metrics/SKILL.md
./skills/type-flow/SKILL.md
```

- [ ] **Step 2: Check total repo size**

```bash
du -sh .git/
```

Should be around 60-70MB (dominated by the three binaries).

- [ ] **Step 3: Push to GitHub**

```bash
git push -u origin main
```

- [ ] **Step 4: Verify repo is accessible**

```bash
gh repo view anortham/julie-plugin --json name,description
```

---

### Task 9: Plugin Update Workflow

**Context:** This GitHub Actions workflow lives in the plugin repo. It receives a `workflow_dispatch` event from the julie release workflow, downloads new binaries, copies updated skills, bumps versions, and commits. This is what makes the plugin repo a zero-maintenance distribution artifact.

**Files:**
- Create: `.github/workflows/update-binaries.yml`

- [ ] **Step 1: Create workflow directory**

```bash
mkdir -p .github/workflows
```

- [ ] **Step 2: Create `.github/workflows/update-binaries.yml`**

```yaml
name: Update Plugin

on:
  workflow_dispatch:
    inputs:
      version:
        description: 'Julie version (e.g., 6.1.7)'
        required: true
      tag:
        description: 'Julie release tag (e.g., v6.1.7)'
        required: true

permissions:
  contents: write

jobs:
  update:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout plugin repo
        uses: actions/checkout@v4

      - name: Download release archives
        env:
          GH_TOKEN: ${{ secrets.JULIE_REPO_TOKEN }}
        run: |
          VERSION="${{ inputs.version }}"
          mkdir -p /tmp/artifacts

          gh release download "${{ inputs.tag }}" \
            --repo anortham/julie \
            --pattern "julie-v${VERSION}-aarch64-apple-darwin.tar.gz" \
            --dir /tmp/artifacts

          gh release download "${{ inputs.tag }}" \
            --repo anortham/julie \
            --pattern "julie-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz" \
            --dir /tmp/artifacts

          gh release download "${{ inputs.tag }}" \
            --repo anortham/julie \
            --pattern "julie-v${VERSION}-x86_64-pc-windows-msvc.zip" \
            --dir /tmp/artifacts

      - name: Extract binaries
        run: |
          VERSION="${{ inputs.version }}"

          # macOS
          mkdir -p bin/aarch64-apple-darwin
          tar -xzf "/tmp/artifacts/julie-v${VERSION}-aarch64-apple-darwin.tar.gz" \
            -C bin/aarch64-apple-darwin/
          chmod +x bin/aarch64-apple-darwin/julie-server

          # Linux
          mkdir -p bin/x86_64-unknown-linux-gnu
          tar -xzf "/tmp/artifacts/julie-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz" \
            -C bin/x86_64-unknown-linux-gnu/
          chmod +x bin/x86_64-unknown-linux-gnu/julie-server

          # Windows
          mkdir -p bin/x86_64-pc-windows-msvc
          unzip -o "/tmp/artifacts/julie-v${VERSION}-x86_64-pc-windows-msvc.zip" \
            -d bin/x86_64-pc-windows-msvc/

      - name: Update skills from julie source
        run: |
          # Clone julie repo at the release tag
          git clone --depth 1 --branch "${{ inputs.tag }}" \
            https://github.com/anortham/julie.git /tmp/julie

          # Clear existing skills and copy fresh set
          rm -rf skills/
          mkdir -p skills

          for skill in architecture call-trace dependency-graph explore-area \
                       impact-analysis logic-flow metrics type-flow; do
            if [ -d "/tmp/julie/.claude/skills/${skill}" ]; then
              cp -r "/tmp/julie/.claude/skills/${skill}" "skills/${skill}"
            else
              echo "WARNING: skill '${skill}' not found in julie repo" >&2
            fi
          done

          # Verify we got the expected count
          SKILL_COUNT=$(ls skills/*/SKILL.md 2>/dev/null | wc -l)
          echo "Copied ${SKILL_COUNT} skills"
          if [ "$SKILL_COUNT" -lt 8 ]; then
            echo "WARNING: Expected 8 skills, got ${SKILL_COUNT}" >&2
          fi

      - name: Update version in manifests
        run: |
          VERSION="${{ inputs.version }}"

          # Update plugin.json
          jq --arg v "$VERSION" '.version = $v' .claude-plugin/plugin.json > tmp.json
          mv tmp.json .claude-plugin/plugin.json

          # Update package.json
          jq --arg v "$VERSION" '.version = $v' package.json > tmp.json
          mv tmp.json package.json

          echo "Updated manifests to v${VERSION}"

      - name: Commit, tag, and push
        run: |
          VERSION="${{ inputs.version }}"

          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

          git add -A
          git commit -m "feat: update to julie v${VERSION}"
          git tag "v${VERSION}"
          git push origin main --tags
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/update-binaries.yml
git commit -m "ci: add automated plugin update workflow"
git push origin main
```

---

### Task 10: Add Release Trigger to Julie Repo

**Context:** The julie source repo's release workflow needs a final step that dispatches the plugin update after a release is published. This requires a GitHub token with `actions:write` scope on the plugin repo.

**Files:**
- Modify: `~/source/julie/.github/workflows/release.yml` (add trigger step to release job)

- [ ] **Step 1: Verify current release workflow structure**

Read `~/source/julie/.github/workflows/release.yml` and locate the `release` job. The trigger step should be added after the "Verify Release" step at the end of the `release` job.

- [ ] **Step 2: Add the trigger step**

Add this step at the end of the `release` job's steps in `release.yml`:

```yaml
      - name: Trigger plugin update
        env:
          GH_TOKEN: ${{ secrets.PLUGIN_REPO_TOKEN }}
        run: |
          echo "Triggering plugin repo update for v${{ steps.version.outputs.version }}..."
          gh workflow dispatch update-binaries \
            --repo anortham/julie-plugin \
            --field version="${{ steps.version.outputs.version }}" \
            --field tag="v${{ steps.version.outputs.version }}"
```

- [ ] **Step 3: Commit the workflow change**

```bash
cd ~/source/julie
git add .github/workflows/release.yml
git commit -m "ci: trigger plugin repo update on release"
```

- [ ] **Step 4: Set up the GitHub secret**

This step must be done manually by the repo owner:

1. Create a fine-grained personal access token at https://github.com/settings/tokens
   - Scope: `actions:write` on `anortham/julie-plugin`
   - Scope: `contents:read` on `anortham/julie` (for release asset downloads)
2. Add it as a secret named `PLUGIN_REPO_TOKEN` in the julie repo:
   - Go to https://github.com/anortham/julie/settings/secrets/actions
   - Click "New repository secret"
   - Name: `PLUGIN_REPO_TOKEN`
   - Value: the token from step 1

Also add a secret named `JULIE_REPO_TOKEN` in the plugin repo (for downloading release assets in the update workflow):
   - Go to https://github.com/anortham/julie-plugin/settings/secrets/actions
   - Name: `JULIE_REPO_TOKEN`
   - Value: same token (or a separate token with `contents:read` on `anortham/julie`)

Note: If both repos are public, `GITHUB_TOKEN` may suffice for reading release assets, and `JULIE_REPO_TOKEN` can be skipped. Test this first.

- [ ] **Step 5: Push the julie workflow change**

```bash
cd ~/source/julie
git push origin main
```

---

### Task 11: End-to-End Verification

**Context:** Install the plugin in Claude Code and verify everything works: MCP server starts, tools are available, SessionStart hook fires, skills are registered.

- [ ] **Step 1: Install the plugin**

```bash
claude plugin add anortham/julie-plugin
```

- [ ] **Step 2: Start a new Claude Code session and verify MCP server**

Open a new Claude Code session in any project directory. Check that Julie tools are available by asking Claude to run:

```
fast_search(query="test")
```

If the tool is available and returns results (or an empty result for an un-indexed project), the MCP server is running.

- [ ] **Step 3: Verify SessionStart hook fired**

Check that the session context includes Julie behavioral guidance. Ask Claude: "What rules do you have about using Julie tools?" Claude should reference the injected rules (search before coding, get_symbols before Read, etc.).

- [ ] **Step 4: Verify skills are registered**

Run `/explore-area test` or ask Claude to list available skills. The 8 Julie skills should appear.

- [ ] **Step 5: Test workspace indexing**

In a project directory, ask Claude to index the workspace:

```
manage_workspace(operation="index")
```

Then search for something:

```
fast_search(query="main")
```

Results should come back from the indexed codebase.

- [ ] **Step 6: Test automation (manual dispatch)**

If you haven't tagged a new julie release yet, manually trigger the plugin update workflow to verify it works:

```bash
gh workflow run update-binaries \
    --repo anortham/julie-plugin \
    --field version="6.1.6" \
    --field tag="v6.1.6"
```

Watch the workflow run:

```bash
gh run watch --repo anortham/julie-plugin
```

It should complete successfully (even though it's re-applying the same version, it validates the full pipeline).
