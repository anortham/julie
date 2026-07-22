---
id: julie-quality-improvement-roadmap
title: Julie quality improvement roadmap
status: active
created: 2026-07-22T12:12:47.959Z
updated: 2026-07-22T12:12:47.959Z
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

Execute the approved four-phase Julie quality roadmap: projection freshness, focused module boundaries, evaluated mixed traversal, and reproducible macOS builds.

## Why Now

The main-branch review exposed projection consistency gaps, oversized implementation files, an unproven traversal opportunity, and a local release toolchain mismatch. Phase 1 is complete; the live Homebrew Rust failure makes the release-build slice an immediate prerequisite to Phase 2A.

## Constraints

- Keep Julie's maintenance-mode/new-user positioning unchanged.
- Preserve macOS 11 support and do not suppress linker diagnostics.
- Phase 2 splits are behavior-preserving and each split has its own approved plan.
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
