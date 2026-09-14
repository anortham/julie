---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-14T14:32:38.648Z
tags:
  - revival
  - plan-4
  - release
  - v8
  - authorized
---

# Julie revival Plans 4 and 5

## Goal

Release v8 from the completed owner-approved Plan 4 scope, then complete Plan 5C/5D and deferred client/macOS qualification.

## Current state

The local Windows v8 blocker is fixed in the uncommitted `main` worktree at base commit `aec74cf3`. The fresh Windows `cargo xtask test full` gate passes all 5/5 commands: 2,291/2,291 dev tests and 69/69 dogfood tests. Formatting, diff checks, and an independent Miller-first review are clean.

Windows fixes cover non-inheriting detached service startup, explicit workspace root resolution, hermetic fixture indexes and service lifecycles, repo-local heavy-test temp storage, Windows command/TOML path serialization, and stable deadline assertions.

## Owner authorization

On 2026-09-13 the owner instructed `tag it and release it`, disposing unavailable local macOS pre-release qualification and authorizing the v8 tag/release path. Current work remains uncommitted and no push/tag/release was performed in the Windows debugging session.

## Remaining release path

Reconcile and commit the reviewed Windows fixes, recheck exact clean SHAs/worktrees, then address the separate hosted native-semantic probe failure. The latest known hosted Windows and macOS semantic probes failed because Hugging Face returned HTTP 429; archive build/verification succeeded. Do not trigger qualification-only workflows. Before any push/tag/release, report the clean exact state required by the approval boundary.

## Deferred work

- Local macOS arm64/Intel fresh-install and upgrade evidence.
- Claude Code, Hermes, and Cursor real-agent adoption/removal.
- Plan 5C/5D replacement decision work.

## Constraints

Do not install into live profiles, copy rotating OAuth state, or modify real user repositories. Keep retained evidence secret-free.
