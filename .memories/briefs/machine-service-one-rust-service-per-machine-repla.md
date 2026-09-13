---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-13T21:48:27.142Z
tags:
  - revival
  - plan-4
  - plan-5
  - qualification
  - release-authorized
---

# Julie revival Plans 4 and 5

## Goal

Finish Plan 4 local release-candidate qualification, complete Plan 5C/5D, and release v8.

## Completed

Plans 1-3 are merged on local `main`. The current Plan 4 lineage passed `cargo xtask test full` 5/5 in 27.7 seconds at exact SHA `1ff96903eed69d136e7a89a6dc44f2223fbc7be3`; later commits change qualification scripts, workflow contracts, and evidence. Another broad run requires an owner decision.

Linux package, fresh-profile service, cached/offline semantics, and Codex/OpenCode/AGY client workflows pass. Local Windows build, package verification, cached/offline semantics, NTFS executable-lock, service lifecycle, and real `v7.18.1 → v8` migration pass. The v7 ZIP SHA-256 is `8bb85e37efec0813a1b959e46725dcf957dbb20ff0f3e774d53ef9bbc217350e`; the v8 ZIP SHA-256 is `5510d7c1cdff9418260def83a4b071196bdaa2ed7a029cc3e89d02f7bf3f4bfe`. Windows full remains reserved for final post-merge validation.

Plan 5A and 5B are complete on `fix/revival-qualification`.

## Owner authorization

On 2026-09-13 the owner approved Rocinante host-key trust, paid remaining-client qualification, the realistic `v7.18.1 → v8` upgrade path, and the intent to release v8. Use disposable profiles and scratch repositories only. Because release approval applies to a verified exact state, report the final clean Julie/plugin SHAs before the first push, tag, release, or publication action.

## Remaining local release-candidate blockers

- macOS arm64 and Intel package qualification. Rocinante's ED25519 host key is verified, but strict plain SSH and the exact Tailscale ProxyCommand both fail authentication because no SSH identity/access is available.
- Claude Code, Hermes, and Cursor disposable profiles are unauthenticated. Device-login sessions are open for owner completion; never copy rotating live OAuth state.
- Linux and macOS upgrade evidence remain. Windows upgrade passes.

Publication-only public assets, awaited reusable-workflow evidence, and public plugin snapshot checks remain pending until exact-state release authorization is reconfirmed.

## Constraints

Do not trigger qualification-only GitHub Actions. Do not push, tag, release, publish, force-push, install into live profiles, copy rotating live OAuth state, or modify real user repositories before the final exact-state report. External model use is authorized only for the remaining Plan 4 client qualification and Plan 5C frozen tasks; keep retained evidence secret-free.

## Plan 5 gate

Plan 5C starts only after the macOS, client, and remaining upgrade rows pass. Preserve all raw outcomes and do not fabricate results.
