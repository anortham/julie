---
id: julie-quality-improvement-roadmap
title: Julie quality improvement roadmap
status: completed
created: 2026-07-22T12:12:47.959Z
updated: 2026-07-22T20:15:46.720Z
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

Execute the approved Julie quality roadmap: projection freshness, four focused module boundaries, evaluated mixed traversal, reproducible macOS builds, and repository-wide formatter normalization.

## Final Status

- Phase 1 projection freshness is complete and verified.
- The macOS release-toolchain prerequisite is complete: official Rust 1.97.0 builds are warning-free while preserving macOS 11.
- Phase 2A runner, Phase 2B changed-selection, Phase 2C watcher-runtime, and Phase 2D SearchIndex module boundaries are complete and verified.
- Phase 3 mixed traversal is promoted: opt-in web mode continues across ordinary, identifier, HTTP, and SQL edges in one deterministic bounded walk, with unchanged default output.
- Phase 4 repository-wide rustfmt normalization is complete in mechanical commit `cda64f0bb1787615982ba53ea762cfe66fa21e13`: exactly 85 Rust files, 304 insertions, and 268 deletions.
- Phase 4 gates are green: formatter, compile, pinned toolchain contract, 39-bucket affected union, 27-bucket dev, warning-free two-binary release build, and 49-bucket full.
- The approved quality roadmap is complete locally on `codex/julie-improvement-roadmap`. No push, merge, publish, deploy, tag, or release has occurred.

## Preserved Constraints

- Julie's maintenance-mode/new-user positioning is unchanged.
- macOS 11 support is preserved and linker diagnostics are not suppressed.
- Phase 2 splits remain behavior-preserving behind stable facades.
- Mixed traversal remains opt-in through `mode = "web"`; default mode remains unchanged.
- Integration requires explicit approval.

## References

- `docs/plans/2026-07-21-julie-improvement-roadmap-design.md`
- `docs/plans/2026-07-22-macos-release-toolchain.md`
- `docs/plans/2026-07-22-runner-module-boundary.md`
- `docs/plans/2026-07-22-changed-selection-module-boundary.md`
- `docs/plans/2026-07-22-watcher-runtime-module-boundary.md`
- `docs/plans/2026-07-22-search-index-module-boundary.md`
- `docs/plans/2026-07-22-impact-mixed-traversal.md`
- `docs/plans/2026-07-22-impact-mixed-traversal-verification.md`
- `docs/plans/2026-07-22-rustfmt-normalization.md`
- `docs/plans/2026-07-22-rustfmt-normalization-verification.md`
