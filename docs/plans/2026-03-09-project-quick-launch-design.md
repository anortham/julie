# Project Quick-Launch (#12)

**Date:** 2026-03-09
**Status:** Approved

## Summary

Add quick-launch actions to the Projects table: copy path, open in editor, open in terminal. Editor command is user-configurable via UI settings stored in localStorage.

## Backend

### `POST /api/launch`

New endpoint in `src/api/projects.rs`:

```json
// Request
{ "command": "code-insiders", "path": "/Users/murphy/Source/julie" }

// Response 200
{ "ok": true }

// Response 400 (missing fields)
{ "error": "command and path are required" }

// Response 500 (spawn failed)
{ "error": "Failed to launch: No such file or directory" }
```

**Behavior:**
- Spawns `{command} {path}` as a detached process (fire-and-forget)
- Validates `path` exists on disk before spawning
- Returns immediately — doesn't wait for the process
- Register in router at `/api/launch` and in OpenAPI schema

**Terminal launch:** Same endpoint — UI sends `{ "command": "open", "path": "-a Terminal /path" }` on macOS. The endpoint splits command from args.

**Actually simpler:** Two dedicated actions to avoid shell injection:
- `POST /api/launch/editor` — `{ "editor": "code-insiders", "path": "/abs/path" }` — spawns `editor path`
- `POST /api/launch/terminal` — `{ "path": "/abs/path" }` — spawns platform-appropriate terminal (macOS: `open -a Terminal path`)

## Frontend

### Action buttons in Projects table

Add an "Actions" column (rightmost) to the projects table with three icon buttons per row:

1. **Copy path** (clipboard icon) — `navigator.clipboard.writeText(p.path)`, shows brief "Copied!" tooltip
2. **Open in Editor** (code icon) — calls `POST /api/launch/editor` with configured editor + project path
3. **Open in Terminal** (terminal icon) — calls `POST /api/launch/terminal` with project path

Buttons should be small, icon-only, with tooltips. Stop click propagation so they don't toggle the stats row.

### Editor settings

Store in `localStorage` key `julie-editor-command`, default `"code"`.

Add a small inline editor config to the Projects page header area — a text input that shows when you click a gear/pencil icon near the action column header. Or simpler: on first "Open in Editor" click, if no editor is configured, prompt with a small popover.

**Simplest approach:** Add an editor command input to the existing settings gear (top-right dark mode toggle area) as a global setting dropdown/input.

## Files to modify

- `src/api/projects.rs` — add `launch_editor()` and `launch_terminal()` handlers
- `src/api/mod.rs` — register routes + OpenAPI
- `ui/src/views/Projects.vue` — action buttons column, copy feedback, editor config
- `ui/src/App.vue` — editor command setting in settings popover (if using global settings)

## Acceptance Criteria

- [ ] Copy path button on each project row — copies to clipboard with "Copied!" feedback
- [ ] "Open in Editor" button — calls backend with configured editor command
- [ ] "Open in Terminal" button — opens system terminal at project path
- [ ] Editor command configurable via UI (stored in localStorage, default: "code")
- [ ] Backend `POST /api/launch/editor` endpoint — spawns `{editor} {path}`, validates path exists
- [ ] Backend `POST /api/launch/terminal` endpoint — platform-appropriate terminal open
- [ ] Action buttons don't trigger row expand/collapse
- [ ] Buttons have tooltips explaining their action
