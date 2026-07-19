---
id: julie-rescue-de-slop-packaging-keep-the-moat
title: Julie Maintenance Mode; v2.16 compatibility upgrade approved
status: active
created: 2026-06-03T14:35:36.960Z
updated: 2026-07-19T00:45:17.151Z
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

## Current Status (2026-07-18)

- Julie v7.15.4 is the current release and consumes `julie-extractors` v2.14.0.
- The owner explicitly approved a maintenance consumer upgrade to `julie-extractors` v2.16.0 so existing Julie users retain extraction compatibility and can use the upstream source-region, structural-fact, and complexity data already being computed.
- The approved architecture and execution plan are `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade-design.md` and `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade.md`.
- New agent workflows should still start with Miller instead of Julie.

## Constraints

- Do not present Julie as the preferred tool for new installs or new agent workflows.
- Treat the v2.16 work as an explicitly approved existing-user compatibility and extraction-consumer upgrade, not a reversal of maintenance mode.
- Keep extraction-language ownership in `anortham/julie-extractors`; Julie only consumes released contracts.
- Do not push, tag, publish, or release without explicit user approval.

## Success Criteria

- Every Julie extractor dependency pins v2.16.0 and the engine stamp forces enrichment backfill.
- Full indexing, watcher updates, and external extraction persist the same typed enrichment data.
- Existing users can query structural facts, search source regions, and see symbol complexity without breaking current tools.
- User-facing docs continue to state Julie maintenance mode and direct new users to Miller.

## References

- Julie README: `README.md`
- Approved design: `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade-design.md`
- Approved plan: `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade.md`
- Miller repo: https://github.com/anortham/miller
