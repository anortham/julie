---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-13T21:57:00.878Z
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

Linux and Windows package, fresh-profile service, cached/offline semantics, executable/lifecycle checks, and real `v7.18.1 → v8` migrations pass. Linux v7/v8 archive SHAs are `a65ce05e23c823c1fd48fbb2e4b112347bda86140c71e474504f8ba399235374` and `9d9095713c3572407722ad105ada7a85fd193619751271d10eb8f23a0fe89bba`. Windows v7/v8 archive SHAs are `8bb85e37efec0813a1b959e46725dcf957dbb20ff0f3e774d53ef9bbc217350e` and `5510d7c1cdff9418260def83a4b071196bdaa2ed7a029cc3e89d02f7bf3f4bfe`. Codex, OpenCode, and AGY client workflows pass. Windows full remains reserved for final post-merge validation.

Plan 5A and 5B are complete on `fix/revival-qualification`.

## Owner authorization

On 2026-09-13 the owner approved Rocinante host-key trust, paid remaining-client qualification, the realistic `v7.18.1 → v8` upgrade path, and the intent to release v8. Use disposable profiles and scratch repositories only. Because release approval applies to a verified exact state, report the final clean Julie/plugin SHAs before the first push, tag, release, or publication action.

## Remaining local release-candidate blockers

- macOS arm64 and Intel package/install/upgrade qualification. Rocinante's ED25519 host key is verified, but SSH authentication needs the owner to add the generated one-use qualification key.
- Claude Code, Hermes, and Cursor disposable device-login sessions need owner completion. Never copy rotating live OAuth state.

Publication-only public assets, awaited reusable-workflow evidence, and public plugin snapshot checks remain pending until exact-state release authorization is reconfirmed.

## Constraints

Do not trigger qualification-only GitHub Actions. Do not push, tag, release, publish, force-push, install into live profiles, copy rotating live OAuth state, or modify real user repositories before the final exact-state report. External model use is authorized only for the remaining Plan 4 client qualification and Plan 5C frozen tasks; keep retained evidence secret-free.

## Plan 5 gate

Plan 5C starts only after the macOS and remaining client rows pass. Preserve all raw outcomes and do not fabricate results.
