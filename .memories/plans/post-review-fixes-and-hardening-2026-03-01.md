---
id: post-review-fixes-and-hardening-2026-03-01
title: Post-review fixes and hardening (2026-03-01)
status: active
created: 2026-03-01T16:38:36.080Z
updated: 2026-03-01T16:38:36.080Z
tags:
  - hardening
  - embeddings
  - extractors
  - search
  - post-review
---

## Goals

- Fix high-risk regressions found in the last-24h code review.
- Harden sidecar IPC/runtime behavior under timeout and protocol mismatch.
- Eliminate known extractor false negatives/false positives across C#/TS/JS/Kotlin/Swift.
- Align docs and runtime behavior for embedding sidecar operations.

## Approach

- Execute strict TDD for each finding: failing regression test -> minimal fix -> targeted verification.
- Keep lock/contention and protocol fixes localized to avoid broad behavioral drift.
- Use small, reviewable commits per task.

## Tasks

- [ ] Task 1: Sidecar timeout/protocol errors trigger process reset and recovery tests
- [ ] Task 2: Python sidecar validates schema/version on requests
- [ ] Task 3: DirectML telemetry normalization for strict acceleration
- [ ] Task 4: Raw sidecar program override mode (no implicit args)
- [ ] Task 5: Remove heavy init work from workspace write-lock path
- [ ] Task 6: Lazy embedding init available from NL definition search path
- [ ] Task 7: C# top-level DI registration extraction
- [ ] Task 8: C# qualified interface inheritance kind classification
- [ ] Task 9: C# tuple-type false positive prevention
- [ ] Task 10: TS/JS qualified heritage extraction
- [ ] Task 11: Cross-language unresolved inheritance semantics consistency
- [ ] Task 12: Embed sidecar ops docs sync and final verification sweep

## Constraints

- TDD is mandatory for every behavioral change.
- Keep implementation files under size limits; avoid broad refactors unless tests demand it.
- Do not regress fast test tier runtime significantly.

## Plan Document

- Detailed step-by-step plan: `docs/plans/2026-03-01-post-review-fixes-hardening.md`
