---
id: julie-quality-improvement-roadmap
title: Julie quality improvement roadmap
status: active
created: 2026-07-22T12:12:47.959Z
updated: 2026-07-22T19:02:21.994Z
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
  - rustfmt
---

## Goal

Execute the approved Julie quality roadmap: projection freshness, four focused module boundaries, evaluated mixed traversal, and reproducible macOS builds.

## Current Status

- Phase 1 projection freshness is complete and verified.
- The macOS release-toolchain prerequisite is complete: official Rust 1.97.0 builds are warning-free while preserving macOS 11.
- Phase 2A runner, Phase 2B changed-selection, Phase 2C watcher-runtime, and Phase 2D SearchIndex module boundaries are complete and verified.
- Phase 3 mixed traversal is promoted: opt-in web mode now continues across ordinary, identifier, HTTP, and SQL edges in one deterministic bounded walk.
- Phase 3 evidence is complete: 7/7 expected internal symbols found, zero unexpected internal links, unchanged default output, targeted checks green, 27-bucket dev green, and 49-bucket full green.
- The remaining roadmap item is the planned one-time repository-wide rustfmt normalization under the pinned formatter. Phase 3-owned Rust files already pass targeted rustfmt checks.

## Constraints

- Keep Julie's maintenance-mode/new-user positioning unchanged.
- Preserve macOS 11 support and do not suppress linker diagnostics.
- Completed Phase 2 splits are behavior-preserving behind stable facades.
- Mixed traversal remains opt-in through `mode = "web"`; default mode must remain byte-identical.
- Do not push, merge, publish, or release without explicit approval.

## Success Criteria

- Both durable projections share one policy-driven health contract.
- The four oversized implementation files are below their limits behind stable facades.
- Mixed traversal is promoted only with complete expected recall, zero unexpected internal links, and unchanged default output.
- Official pinned Rust builds are warning-free on macOS and repository-wide formatting is reproducible.

## References

- `docs/plans/2026-07-21-julie-improvement-roadmap-design.md`
- `docs/plans/2026-07-22-macos-release-toolchain.md`
- `docs/plans/2026-07-22-runner-module-boundary.md`
- `docs/plans/2026-07-22-changed-selection-module-boundary.md`
- `docs/plans/2026-07-22-watcher-runtime-module-boundary.md`
- `docs/plans/2026-07-22-search-index-module-boundary.md`
- `docs/plans/2026-07-22-impact-mixed-traversal.md`
- `docs/plans/2026-07-22-impact-mixed-traversal-verification.md`
