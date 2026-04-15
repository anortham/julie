---
id: structured-unresolved-hardening-2026-04-14
title: Structured Unresolved Relationship Hardening
status: active
created: 2026-04-15T00:28:03.181Z
updated: 2026-04-15T00:28:03.181Z
tags:
  - treesitter
  - extractors
  - relationship-precision
  - planning
  - hardening
---

# Structured Unresolved Relationship Hardening Implementation Plan

> Source file: `docs/plans/2026-04-14-structured-unresolved-hardening.md`

## Goal
Finish the structured unresolved-relationship migration for the remaining legacy extractor wave and lock the contract down with shared invariant tests.

## Architecture
Keep downstream consumers stable and migrate producers. Each remaining extractor should emit `StructuredPendingRelationship` first, then preserve compatibility by degrading to legacy `PendingRelationship`. Canonical extraction and normalization stay the shared contract, with invariant tests proving structured identity survives extend, offset, and rekey flows.

## Task Breakdown
1. Expand shared invariant coverage in `relationship_precision.rs`, `results_normalization.rs`, `path_identity.rs`, and `api_surface.rs`.
2. Migrate the systems-style extractor wave: `c`, `cpp`, `rust`, `zig`, plus their cross-file suites.
3. Migrate the dynamic and package-aware wave: `go`, `python`, `ruby`, `gdscript`, `dart`, plus their cross-file suites.
4. Finish canonical registry coverage and parity checks in `registry.rs`, `manager.rs`, `factory.rs`, and `api_surface.rs`.
5. Run final verification and cleanup, ending with `cargo xtask test dev`.

## Key Files
- `crates/julie-extractors/src/base/results_normalization.rs`
- `crates/julie-extractors/src/registry.rs`
- `crates/julie-extractors/src/tests/relationship_precision.rs`
- `crates/julie-extractors/src/tests/api_surface.rs`
- `crates/julie-extractors/src/c/**`
- `crates/julie-extractors/src/cpp/**`
- `crates/julie-extractors/src/rust/**`
- `crates/julie-extractors/src/zig/**`
- `crates/julie-extractors/src/go/**`
- `crates/julie-extractors/src/python/**`
- `crates/julie-extractors/src/ruby/**`
- `crates/julie-extractors/src/gdscript/**`
- `crates/julie-extractors/src/dart/**`

## Notes
- Design spec lives at `docs/plans/2026-04-14-structured-unresolved-hardening-design.md`.
- Follow strict TDD for each task with narrow RED/GREEN loops before the final dev-tier run.
- This plan is sized for same-session execution; in this harness the recommended parallel path is `razorback:subagent-driven-development` for independent tasks.
