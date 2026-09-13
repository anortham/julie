---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-13T20:21:09.736Z
tags:
  - revival
  - plan-4
  - plan-5
  - qualification
  - blocked
---

# Julie revival Plans 4 and 5

## Goal

Finish Plan 4 local release-candidate qualification, then run Plan 5C and produce the Plan 5D decision ledger.

## Completed

Plans 1-3 are merged on local `main`. The current Plan 4 lineage passed `cargo xtask test full` 5/5 in 27.7 seconds at exact SHA `1ff96903eed69d136e7a89a6dc44f2223fbc7be3`; later commits change qualification scripts, workflow contracts, and evidence. Another broad run requires an owner decision.

Linux package, fresh-profile service, cached/offline semantics, and Codex/OpenCode/AGY client workflows pass. Local Windows build, package verification, cached/offline semantics, NTFS executable-lock, and service lifecycle pass for archive SHA-256 `5510d7c1cdff9418260def83a4b071196bdaa2ed7a029cc3e89d02f7bf3f4bfe`. Windows full remains reserved for final post-merge validation.

Plan 5A and 5B are complete on `fix/revival-qualification`.

## Remaining local release-candidate blockers

- macOS arm64 and Intel package qualification. A real Mac, Tailscale peer `Rocinante` at `100.89.204.7`, is online. SSH requires owner verification of its ED25519 host-key fingerprint `SHA256:X6EuBvc7l+6sWC21dKAXBxe4gRd2qPBnQRtm1xmIj30`; do not bypass host checking.
- Claude Code needs refreshed disposable OAuth, Hermes needs interactive OAuth in a disposable profile, and Cursor needs a disposable API key. New real-agent runs also require explicit external-model spend authority.
- No compatible prior versioned-v8 archive exists. The owner must authorize creation/preservation of a real v8 prerelease artifact, permit a v7-to-v8 migration qualification, or waive/change that row.

Publication-only public assets, awaited reusable-workflow evidence, and public plugin snapshot checks do not block the local release candidate. They remain pending separate push/tag/release/publication authority.

## Constraints

Do not trigger GitHub Actions, push, tag, release, publish, force-push, install into live profiles, copy rotating live OAuth state, spend on external models, or modify real user repositories without explicit authorization.

## Plan 5 gate

Plan 5C remains blocked until the macOS, client-authorization, and upgrade rows are disposed. Do not fabricate 5C or 5D results.
