# Standup View — Dashboard Feature

**Date:** 2026-03-09
**Status:** Approved

## Summary

Add a `/standup` route to the Julie web dashboard that generates a meeting-ready standup report from checkpoint data across all projects. The view synthesizes raw checkpoint and plan data client-side into a rendered markdown standup, suitable for screen-sharing in meetings.

## Design

### Data Flow
- Fetch projects from `/api/projects`
- Fetch checkpoints from `/api/memories?since={range}` per project (same pattern as Memories view)
- Fetch plans from `/api/plans` per project
- Synthesize client-side into markdown standup
- Render in a styled container with copy-to-clipboard

### Controls
- **Time range dropdown:** 1 day, 3 days, 7 days, 14 days
- **Project filter:** "All projects" or specific project (same dropdown pattern as Memories view)

### Standup Synthesis Logic
1. Group checkpoints by project (using workspace_id → project name mapping)
2. Within each project, group by theme (merge checkpoints with overlapping tags/symbols)
3. Extract "Up Next" from the most recent checkpoint's `next` field per project
4. Extract blockers from checkpoints containing block/stuck/wait keywords
5. Cross-reference active plans — show task completion counts if plan has `- [ ]` / `- [x]` items

### Output Format
- **Multi-project** (2+ projects with checkpoints): grouped by project with bullets + plan progress
- **Single-project** (1 project): Done / Up Next / Blocked sections
- Rendered as styled HTML (not raw markdown) in a card container
- Copy button copies plain markdown to clipboard

### UI Pattern
- Single-file Vue component (`Standup.vue`) following existing view patterns
- Uses existing CSS variables, `.card`, `.btn-*`, `.form-select` classes from App.vue
- PrimeIcons for icons (consistent with other views)
- Dark mode support via existing CSS variables

## Files to Create/Modify
- `ui/src/views/Standup.vue` — new view component
- `ui/src/router/index.ts` — add `/standup` route
- `ui/src/App.vue` — add nav link

## Acceptance Criteria
- [ ] New `Standup.vue` view at `/standup` route
- [ ] Nav link added to App.vue header
- [ ] Time range dropdown (1d/3d/7d/14d)
- [ ] Project filter dropdown
- [ ] Checkpoints grouped by project, synthesized into standup bullets
- [ ] Active plans shown with task completion counts
- [ ] Multi-project and single-project formats
- [ ] Copy-to-clipboard button for the rendered standup
- [ ] Dark mode support
- [ ] Loading/empty states
