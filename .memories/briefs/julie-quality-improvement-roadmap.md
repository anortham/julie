---
id: julie-quality-improvement-roadmap
title: Julie quality improvement roadmap
status: active
created: 2026-07-22T12:12:47.959Z
updated: 2026-07-22T14:20:05.133Z
tags:
  - julie
  - quality
  - roadmap
  - module-boundaries
  - projection-freshness
  - macos
  - toolchain
  - search-index
  - mixed-traversal
---

## Goal

Execute the approved Julie quality roadmap: projection freshness, four focused module boundaries, evaluated mixed traversal, and reproducible macOS builds.

## Current Status

- Phase 1 projection freshness is complete and verified.
- The macOS release-toolchain work originally numbered Phase 4 was pulled forward and is complete: official Rust 1.97.0 builds are warning-free while preserving macOS 11.
- Phase 2A runner, Phase 2B changed-selection, and Phase 2C watcher-runtime module boundaries are complete and verified.
- Phase 2D SearchIndex impact analysis and implementation planning are active.
- Phase 3 evaluated mixed traversal remains pending.

## Constraints

- Keep Julie's maintenance-mode/new-user positioning unchanged.
- Preserve macOS 11 support and do not suppress linker diagnostics.
- Phase 2 splits are behavior-preserving and each split has its own approved plan.
- Keep `SearchIndex` as the public facade and preserve schema compatibility, open lifecycle, writer mutation, query behavior, scoring, serialized results, and concurrency semantics during Phase 2D.
- Mixed traversal remains evaluation-first and opt-in.
- Do not push, merge, publish, or release without explicit approval.

## Success Criteria

- Both durable projections share one policy-driven health contract.
- The four oversized implementation files are below their limits behind stable facades.
- Mixed traversal ships only after precision evidence.
- Official pinned Rust builds are warning-free on macOS and formatting is reproducible.

## References

- `docs/plans/2026-07-21-julie-improvement-roadmap-design.md`
- `docs/plans/2026-07-22-macos-release-toolchain.md`
- `docs/plans/2026-07-22-runner-module-boundary.md`
- `docs/plans/2026-07-22-changed-selection-module-boundary.md`
- `docs/plans/2026-07-22-watcher-runtime-module-boundary.md`
