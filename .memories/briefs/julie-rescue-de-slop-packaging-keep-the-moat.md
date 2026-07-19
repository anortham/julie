---
id: julie-rescue-de-slop-packaging-keep-the-moat
title: Julie Maintenance Mode; v2.16 compatibility upgrade approved
status: active
created: 2026-06-03T14:35:36.960Z
updated: 2026-07-19T03:33:45.329Z
tags:
  - julie
  - maintenance-mode
  - miller
  - replacement
  - julie-extractors
  - v2.16
  - active-upgrade
---

## Decision

Julie remains in maintenance mode. Miller is the replacement project for new code-intelligence development.

## Current Status (2026-07-19)

- The approved `julie-extractors` v2.16.0 consumer upgrade is implemented and verified on `codex/julie-extractors-consumer-upgrade`.
- Julie pins v2.16.0, stamps the new extraction contract, persists source regions, structural facts, and complexity metrics across indexing/watcher/external-extract paths, and exposes them through `patterns`, region-filtered search, and deep-dive complexity.
- Docs and the verification ledger are synchronized in `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade-verification.md`.
- Extractor integration, system, dogfood, branch-diff changed, explicit dev, docs, clippy, build, and standalone CLI evidence pass. Full `cargo fmt --check` remains an inherited baseline failure: origin/main has 112 drift files, the branch has 110, and the upgrade adds zero new drift files.
- The branch is ready for review/integration; nothing has been pushed, tagged, published, or released.
- New agent workflows should still start with Miller instead of Julie.

## Constraints

- Do not present Julie as the preferred tool for new installs or new agent workflows.
- Treat the v2.16 work as an explicitly approved existing-user compatibility and extraction-consumer upgrade, not a reversal of maintenance mode.
- Keep extraction-language ownership in `anortham/julie-extractors`; Julie only consumes released contracts.
- Do not push, tag, publish, merge, or release without explicit user approval.

## Success Criteria

- Every Julie extractor dependency pins v2.16.0 and the engine stamp forces enrichment backfill.
- Full indexing, watcher updates, and external extraction persist the same typed enrichment data.
- Existing users can query structural facts, search source regions, and see symbol complexity without breaking current tools.
- User-facing docs continue to state Julie maintenance mode and direct new users to Miller.

## References

- Julie README: `README.md`
- Approved design: `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade-design.md`
- Approved plan: `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade.md`
- Verification ledger: `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade-verification.md`
- Miller repo: https://github.com/anortham/miller
