# Responsive UI Polish — Design

**Date:** 2026-03-09
**Scope:** CSS-only responsive pass across the Julie management dashboard

## Problem

The dashboard was built desktop-first with no breakpoints for narrow screens:
- Nav bar overflows (5 links + brand + toggle in fixed 56px flex row)
- Projects table clips content (`overflow: hidden` with no horizontal scroll)
- Form rows (Projects register, Search input) don't stack on mobile
- Main content padding too wide on small screens

## Design

### 1. Nav (≤768px) — Icon-only mode
- Hide `.brand-name` text (keep icon)
- Hide nav link text labels (keep icons)
- Reduce header gap and padding

### 2. Projects table — Horizontal scroll
- Change `.table-wrapper` from `overflow: hidden` to `overflow-x: auto`
- Add `min-width` on table to prevent column collapse

### 3. Form rows (≤600px) — Stack to column
- Projects `.form-row`: `flex-direction: column`
- Search `.search-row`: `flex-direction: column`, button full-width

### 4. Main padding (≤600px)
- Reduce `.app-main` padding from `1.5rem` to `1rem`

### Already responsive (no changes needed)
- Dashboard: `auto-fit` grid adapts naturally
- Memories: grid → single column at 900px
- Agents: form-row stacks at 700px
- Search filters: `flex-wrap` already wraps

## Files Modified
- `ui/src/App.vue` — nav breakpoint + main padding
- `ui/src/views/Projects.vue` — table scroll + form stacking
- `ui/src/views/Search.vue` — search row stacking

## Acceptance Criteria
- [ ] Nav doesn't overflow at 320px viewport width
- [ ] Projects table scrolls horizontally on narrow screens
- [ ] All form rows stack vertically below 600px
- [ ] No horizontal page-level scrollbar at any width down to 320px
