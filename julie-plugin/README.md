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
sudo mv julie-server /usr/local/bin/julie
```

**macOS (Intel):**

```bash
curl -L https://github.com/anortham/julie/releases/latest/download/julie-x86_64-apple-darwin.tar.gz | tar xz
sudo mv julie-server /usr/local/bin/julie
```

**Linux (x86_64):**

```bash
curl -L https://github.com/anortham/julie/releases/latest/download/julie-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv julie-server /usr/local/bin/julie
```

**Windows:**

Download `julie-x86_64-pc-windows-msvc.zip` from the releases page, extract `julie-server.exe`, and add it to your PATH.

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

The daemon listens on `http://localhost:3141` by default. It runs in the background and serves all your projects.

To check daemon status:

```bash
julie daemon status
```

To stop the daemon:

```bash
julie daemon stop
```

## Installing the Plugin

```bash
claude plugin add /path/to/julie-plugin
```

Or, if you cloned the Julie repository:

```bash
claude plugin add ./julie-plugin
```

## Verification

After installing the binary, starting the daemon, and adding the plugin:

1. **Confirm the daemon is running:**

   ```bash
   julie daemon status
   ```

   You should see output indicating the daemon is active on port 3141.

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
- **Dashboard**: Web UI at `http://localhost:3141` for monitoring and exploration

The plugin simply points Claude Code at the daemon's HTTP endpoint — no binary spawning, no per-session startup cost.

## Troubleshooting

**"Connection refused" errors:**
The daemon isn't running. Start it with `julie daemon start`.

**Plugin not showing in `claude mcp list`:**
Re-add the plugin with `claude plugin add /path/to/julie-plugin`.

**No results from searches:**
Julie may still be indexing your project. Check `julie daemon status` or the dashboard at `http://localhost:3141` for indexing progress.
