# Install Command — System Service Setup

**Date:** 2026-03-09
**Status:** Approved
**Release:** v4.0.1

## Summary

Add `julie-server install` and `julie-server uninstall` subcommands that set up Julie as an auto-starting system service. This replaces the manual `julie-server daemon start` step and ensures the dashboard is always available at `localhost:7890`.

## Motivation

Current onboarding requires too many steps and the daemon only runs when manually started or when an MCP session triggers it. The web dashboard and API are unavailable unless the user remembers to start the daemon. This affects all MCP clients, not just Claude Code.

## Design

### `julie-server install`

**Idempotent** — safe to run multiple times (updates binary, restarts service).

Steps:
1. Create `~/.julie/bin/` directory if needed
2. Copy the running binary to `~/.julie/bin/julie-server` (or `.exe` on Windows)
3. Detect platform and create auto-start config:
   - **macOS**: `~/Library/LaunchAgents/com.julie.server.plist` (LaunchAgent with `RunAtLoad`, `KeepAlive`, stdout/stderr to `~/.julie/logs/`)
   - **Linux**: `~/.config/systemd/user/julie.service` (user-level systemd unit with `Restart=on-failure`), then `systemctl --user daemon-reload && systemctl --user enable --now julie`
   - **Windows**: Scheduled Task via `schtasks.exe` (trigger: user logon, action: run `~/.julie/bin/julie-server.exe daemon run`)
4. Start the daemon immediately (or restart if already running)
5. Print status summary and next steps

Output example:
```
Julie installed successfully!
  Binary: ~/.julie/bin/julie-server
  Service: ~/Library/LaunchAgents/com.julie.server.plist
  Status: running (pid 12345)
  Dashboard: http://localhost:7890/ui/
  API docs: http://localhost:7890/api/docs

Next steps — configure your AI tool:
  Claude Code: /plugin marketplace add anortham/julie
  Cursor/other: see https://github.com/anortham/julie#installation
```

### `julie-server uninstall`

Steps:
1. Stop the daemon if running
2. Remove the platform service config
3. Remove `~/.julie/bin/julie-server`
4. Leave `~/.julie/` data intact (indexes, logs, memories, config)
5. Print confirmation

### Service Configuration Details

**macOS LaunchAgent plist:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.julie.server</string>
    <key>ProgramArguments</key>
    <array>
        <string>~/.julie/bin/julie-server</string>
        <string>daemon</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>~/.julie/logs/launchd-stdout.log</string>
    <key>StandardErrorPath</key>
    <string>~/.julie/logs/launchd-stderr.log</string>
</dict>
</plist>
```

**Linux systemd unit:**
```ini
[Unit]
Description=Julie Code Intelligence Server
After=network.target

[Service]
Type=simple
ExecStart=%h/.julie/bin/julie-server daemon run
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
```

**Windows scheduled task:**
```
schtasks /Create /TN "Julie Server" /TR "\"%USERPROFILE%\\.julie\\bin\\julie-server.exe\" daemon run" /SC ONLOGON /RL LIMITED /F
```

### Plugin Changes

Plugin stays as HTTP — with the daemon as a system service with auto-restart (`KeepAlive`/`Restart=on-failure`), the HTTP endpoint is reliably available. No binary path resolution or tilde expansion needed across platforms.

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

### README Changes

Installation section becomes:
```
## Installation

### Step 1: Install Julie

Download the latest release for your platform from the Releases page, extract it, then run:

    julie-server install

This installs the binary to ~/.julie/bin/, registers it as a system service,
and starts the daemon. Julie will auto-start on login from now on.

Dashboard: http://localhost:7890/ui/
API docs:  http://localhost:7890/api/docs

### Step 2: Connect Your AI Tool

Claude Code (recommended):
    /plugin marketplace add anortham/julie
    /plugin install julie@julie

Claude Code (standalone MCP, no plugin):
    claude mcp add julie -- ~/.julie/bin/julie-server connect

Cursor / Other MCP clients:
    {
      "mcpServers": {
        "julie": {
          "type": "http",
          "url": "http://localhost:7890/mcp"
        }
      }
    }
```

## Implementation

### Files to Create
- `src/cli/install.rs` — install/uninstall command logic

### Files to Modify
- `src/cli/mod.rs` — add Install/Uninstall subcommands to clap
- `src/main.rs` — wire up the new subcommands
- `julie-plugin/.claude-plugin/plugin.json` — switch to stdio/connect
- `README.md` — simplified installation section

### Note on `daemon run` vs `daemon start`
The service configs use `daemon run` (foreground mode) rather than `daemon start` (which daemonizes). The OS service manager handles backgrounding, restart, and lifecycle — we don't want the process to double-fork away from launchd/systemd.

## Acceptance Criteria

- [ ] `julie-server install` copies binary to `~/.julie/bin/`
- [ ] macOS: creates LaunchAgent plist, loads it
- [ ] Linux: creates systemd user unit, enables and starts it
- [ ] Windows: creates scheduled task via schtasks
- [ ] Daemon starts immediately after install
- [ ] Running `install` again updates binary and restarts (idempotent)
- [ ] `julie-server uninstall` stops daemon, removes service config, removes binary
- [ ] `uninstall` preserves `~/.julie/` data (indexes, logs, memories)
- [ ] Plugin version bumped to 4.0.1
- [ ] README updated with simplified installation flow
- [ ] `daemon run` subcommand exists (foreground mode for service managers)
