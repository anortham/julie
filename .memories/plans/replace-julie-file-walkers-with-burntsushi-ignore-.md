---
id: replace-julie-file-walkers-with-burntsushi-ignore-
title: Replace Julie file walkers with BurntSushi ignore crate
status: completed
created: 2026-02-23T14:02:32.623Z
updated: 2026-02-23T14:52:10.041Z
tags:
  - refactor
  - ignore-crate
  - file-walking
  - gitignore
---

# Replace Julie File Walkers with BurntSushi `ignore` Crate

## Overview
Replace two independent manual file walkers (discovery.rs indexing + startup.rs stale detection) with the battle-tested `ignore` crate from BurntSushi/ripgrep. This adds native `.gitignore` support and eliminates ~300 lines of custom walking/filtering code.

## Status: Planning Complete

## Key Design Decisions
1. `ignore` crate with `hidden(false)` -- don't blanket-skip hidden files
2. `.git/` excluded via `filter_entry` (since `hidden(false)` includes `.git`)
3. `.julieignore` via `add_custom_ignore_filename`
4. `filter_entry` for BLACKLISTED_DIRECTORIES (prunes subtrees)
5. Post-filter for: blacklisted extensions, file size, minified, text check
6. Two walker configs for discovery.rs vendor detection vs final indexing
7. Watcher filtering.rs is OUT OF SCOPE (uses glob::Pattern, different concern)

## Tasks (see detailed plan in response)
1. Add `ignore` crate dependency
2. Create shared walker builder module
3. Rewrite discovery.rs walker
4. Rewrite startup.rs walker  
5. Delete dead code
6. Update tests
7. Remove `walkdir` dependency

