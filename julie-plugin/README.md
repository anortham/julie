# Julie Plugin for Claude Code

Cross-platform code intelligence server — fast search, navigation, refactoring, and developer memory across 31 languages.

This plugin connects Claude Code to the Julie daemon, giving agents LSP-quality code intelligence without reading raw files. The result: fewer tokens per operation, longer sessions, and more productive coding.

## Prerequisites

The Julie binary must be installed separately. The plugin is a lightweight connection layer — it tells Claude Code where to find the running daemon.

## Installing the Binary

### Option 1: GitHub Releases (Recommended)

Download the latest release for your platform from the [Releases page](https://github.com/anortham/julie/releases).

**macOS (Apple Silicon):**

```bash
curl -L https://github.com/anortham/julie/releases/latest/download/julie-aarch64-apple-darwin.tar.gz | tar xz
xattr -d com.apple.quarantine julie-server
sudo mv julie-server /usr/local/bin/julie
```

**macOS (Intel):**

```bash
curl -L https://github.com/anortham/julie/releases/latest/download/julie-x86_64-apple-darwin.tar.gz | tar xz
xattr -d com.apple.quarantine julie-server
sudo mv julie-server /usr/local/bin/julie
```

**Linux (x86_64):**

```bash
curl -L https://github.com/anortham/julie/releases/latest/download/julie-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv julie-server /usr/local/bin/julie
```

**Windows (PowerShell):**

Download and extract the latest release, then install:

```powershell
Invoke-WebRequest -Uri "https://github.com/anortham/julie/releases/latest/download/julie-x86_64-pc-windows-msvc.zip" -OutFile julie.zip
Expand-Archive julie.zip -DestinationPath .
.\julie-server.exe install
```

Or download `julie-x86_64-pc-windows-msvc.zip` from the releases page manually, extract it, and run `julie-server.exe install`.

### Option 2: Cargo Install

```bash
cargo install julie
```

### Option 3: Build from Source

```bash
git clone https://github.com/anortham/julie.git
cd julie
cargo build --release
# Binary is at target/release/julie-server
```

## Starting the Daemon

The plugin connects to Julie via HTTP, so the daemon must be running before you use the plugin.

```bash
julie daemon start
```

The daemon listens on `http://localhost:7890` by default. It runs in the background and serves all your projects.

To check daemon status:

```bash
julie daemon status
```

To stop the daemon:

```bash
julie daemon stop
```

## Installing the Plugin

In Claude Code:

```
/plugin marketplace add anortham/julie
/plugin install julie@julie
```

Or from a local clone:

```bash
claude plugin add ./julie-plugin
```

## Verification

After installing the binary, starting the daemon, and adding the plugin:

1. **Confirm the daemon is running:**

   ```bash
   julie daemon status
   ```

   You should see output indicating the daemon is active on port 7890.

2. **Confirm the plugin is loaded in Claude Code:**

   ```bash
   claude mcp list
   ```

   You should see `julie` listed with its HTTP endpoint.

3. **Test a tool call:** Ask Claude to run a Julie tool in your project:

   ```
   Use fast_search to find "main" in this project
   ```

   If Julie returns search results, everything is working.

## How It Works

Unlike stdio-based MCP servers, Julie runs as a persistent daemon. This architecture enables:

- **Shared index**: One daemon serves all your Claude Code sessions and projects
- **Background indexing**: File changes are picked up automatically
- **Cross-project search**: Search across related codebases
- **Dashboard**: Web UI at `http://localhost:7890` for monitoring and exploration

The plugin simply points Claude Code at the daemon's HTTP endpoint — no binary spawning, no per-session startup cost.

## Other AI Coding Tools

Julie's MCP server works with any MCP-compatible tool — not just Claude Code. The plugin (hooks, slash commands) is Claude Code-specific, but all of Julie's core tools are available to any agent that speaks MCP.

### Cursor

Add to `.cursor/mcp.json` in your project (daemon mode):

```json
{
  "mcpServers": {
    "julie": {
      "type": "http",
      "url": "http://localhost:7890/mcp"
    }
  }
}
```

Or stdio mode (no daemon required):

```json
{
  "mcpServers": {
    "julie": {
      "command": "julie",
      "args": [],
      "env": {
        "JULIE_WORKSPACE": "/path/to/your/project"
      }
    }
  }
}
```

### Windsurf / Other MCP-Compatible Tools

Most tools use a `.mcp.json` or similar config file. Copy the appropriate config from [`mcp.json.example`](../mcp.json.example) at the repository root. Daemon mode (`type: http`) is recommended for the full experience; stdio mode works for single-project use.

### What Works Everywhere (Any MCP Client)

All Julie tools work via the MCP protocol regardless of client:

- **Search** — `fast_search` for definitions and content across 31 languages
- **Navigation** — `fast_refs` for finding references and call sites
- **Context** — `get_context` for token-budgeted code retrieval
- **Investigation** — `deep_dive` for progressive-depth symbol exploration
- **Symbols** — `get_symbols` for file-level symbol listing
- **Refactoring** — `rename_symbol` for cross-file renames
- **Memory** — `checkpoint`, `recall`, `plan` for cross-session persistence
- **Workspace** — `manage_workspace` for adding reference projects

Tool descriptions and server instructions are served via the MCP protocol itself, so agents see usage guidance regardless of which client they're running in.

### What's Claude Code-Only (Requires the Plugin)

These features depend on Claude Code's plugin hooks and skills system:

- **Automatic recall** on session start (`SessionStart` hook)
- **Automatic checkpoint** before context compaction (`PreCompact` hook)
- **Automatic plan save** after plan approval (`PostToolUse` → `ExitPlanMode` hook)
- **Slash commands** — `/checkpoint`, `/recall`, `/standup`, `/plan`, `/plan-status`

### For Agents Without Hooks

Without the plugin hooks, memory features still work — they just aren't automatic. Julie's tool descriptions explain when to checkpoint and recall, so agents that read tool descriptions will naturally use them at appropriate times. The difference is that Claude Code's hooks *guarantee* it happens at key moments (session start, before compaction, after plan approval) rather than relying on agent initiative.

## Troubleshooting

**"Connection refused" errors:**
The daemon isn't running. Start it with `julie daemon start`.

**Plugin not showing in `claude mcp list`:**
Re-add the plugin with `claude plugin add /path/to/julie-plugin`.

**No results from searches:**
Julie may still be indexing your project. Check `julie daemon status` or the dashboard at `http://localhost:7890` for indexing progress.
