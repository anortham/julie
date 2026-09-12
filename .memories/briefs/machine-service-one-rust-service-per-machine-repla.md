---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-12T21:52:19.517Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

# Julie revival Plans 4 and 5

## Goal

Complete Plan 4 deployment and guidance qualification, then Plan 5 replacement qualification. Plan 4 must leave a locally reviewable release candidate with consistent agent instructions, immutable versioned installation, truthful cold/offline semantics, executable archive checks and a fresh-install evidence matrix.

## Completed

Plans 1 through 3 are merged on local `main`. Plan 3 added the single runtime lifecycle owner, bounded idle retirement, cooperative writer shutdown and durable reopen behavior.

## Current work

Plan 4 is active on Julie branch `fix/revival-deployment` and plugin branch `fix/revival-deployment-plugin`. Work follows `docs/plans/2026-09-11-revival-deployment-plan.md` in serial order from 4A through 4E.

## Constraints

- Keep the stdio shim, one machine service and native semantic sidecar.
- Use immutable versioned install directories and explicit service restart on version mismatch.
- Preserve exact workspace routing and the pinned extractor inventory.
- Use current official harness documentation before changing harness guidance.
- Do not write live user profiles, installed plugin caches or the maintainer JULIE_HOME.
- Do not push, force-push, tag, release or publish without separate owner approval.
- Local commits inside the approved plan are authorized.
- Windows qualification failures unrelated to the active diff remain separate follow-up work.

## Success

The local release candidate has passing Julie and plugin gates, verified native archives, isolated fresh-profile evidence, documented unresolved native/client gaps, and no publication side effects.

## References

- docs/plans/2026-09-11-revival-roadmap.md
- docs/plans/2026-09-11-revival-deployment-plan.md
- docs/plans/2026-09-11-revival-qualification-plan.md
