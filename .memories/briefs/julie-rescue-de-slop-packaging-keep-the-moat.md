---
id: julie-rescue-de-slop-packaging-keep-the-moat
title: Julie Maintenance Mode; Miller is the replacement
status: active
created: 2026-06-03T14:35:36.960Z
updated: 2026-06-06T22:59:48.848Z
tags:
  - julie
  - maintenance-mode
  - miller
  - replacement
  - current-status
---

## Decision

Julie is now in maintenance mode. Miller is the replacement project for new code-intelligence development.

## Current Status (2026-06-06)

- Julie v7.13.3 is published as the final single-server rescue release line for existing users.
- `README.md` now tells users Julie is in maintenance mode and links to Miller: https://github.com/anortham/miller.
- Existing Julie users can keep using the current release line, especially deployments that depend on the MCP server, extraction behavior, or plugin packaging.
- New agent workflows should start with Miller instead of Julie.

## Constraints

- Do not present Julie as the preferred tool for new installs or new agent workflows.
- Maintenance work should focus on keeping existing Julie users unblocked, not building new strategic product surface.
- Avoid broad new Julie features unless explicitly approved; Miller should receive new code-intelligence investment.
- Do not push, tag, publish, or release further changes without explicit user approval.

## Success Criteria

- User-facing docs clearly state Julie maintenance mode and point new users to Miller.
- Existing Julie release/plugin paths remain usable for current users.
- Future sessions start from the Miller-replacement direction rather than the old rescue framing.

## References

- Julie README: `README.md`
- Julie release: https://github.com/anortham/julie/releases/tag/v7.13.3
- Miller repo: https://github.com/anortham/miller
