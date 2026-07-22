---
id: julie-quality-improvement-roadmap
title: Julie quality improvement roadmap
status: active
created: 2026-07-22T12:12:47.959Z
updated: 2026-07-22T13:51:20.334Z
tags:
  - julie
  - quality
  - roadmap
  - module-boundaries
  - projection-freshness
  - macos
  - toolchain
---

## Goal

Execute the approved Julie quality roadmap: projection freshness, four focused module boundaries, evaluated mixed traversal, and reproducible macOS builds.

## Current Status

- Phase 1 projection freshness is complete and verified.
- The macOS release-toolchain work originally numbered Phase 4 was pulled forward and is complete: official Rust 1.97.0 builds are warning-free while preserving macOS 11.
- Phase 2A runner and Phase 2B changed-selection module boundaries are complete and verified.
- Phase 2C watcher-runtime impact analysis and implementation planning are active.
- Phase 2D SearchIndex and Phase 3 evaluated mixed traversal remain pending.

## Constraints

- Keep Julie's maintenance-mode/new-user positioning unchanged.
- Preserve macOS 11 support and do not suppress linker diagnostics.
- Phase 2 splits are behavior-preserving and each split has its own approved plan.
- Preserve watcher mutation-gate, cancellation, retry, shutdown, and durable projection semantics during Phase 2C.
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
