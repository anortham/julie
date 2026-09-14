---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-14T00:10:16.932Z
tags:
  - revival
  - plan-4
  - release
  - v8
  - authorized
---

# Julie revival Plans 4 and 5

## Goal

Release v8 from the completed owner-approved Plan 4 release scope. Complete Plan 5C/5D and deferred client/macOS install qualification afterward.

## Completed

Plans 1-3 are merged on local `main`. Plan 4 implementation, Linux/Windows package qualification, cached/offline semantics, service lifecycle, executable-lock behavior, and real `v7.18.1 → v8` migrations pass. Codex, OpenCode, and AGY client workflows pass.

Linux v7/v8 archive SHAs are `a65ce05e23c823c1fd48fbb2e4b112347bda86140c71e474504f8ba399235374` and `9d9095713c3572407722ad105ada7a85fd193619751271d10eb8f23a0fe89bba`. Windows v7/v8 archive SHAs are `8bb85e37efec0813a1b959e46725dcf957dbb20ff0f3e774d53ef9bbc217350e` and `5510d7c1cdff9418260def83a4b071196bdaa2ed7a029cc3e89d02f7bf3f4bfe`.

Plan 5A and 5B are complete on `fix/revival-qualification`.

## Owner authorization

On 2026-09-13 the owner instructed `tag it and release it`. This explicitly disposes unavailable local macOS pre-release qualification and authorizes the v8 tag/release path. The release workflow still hard-gates both actual macOS packages before its dependent GitHub-release job can create the release.

The owner also moved Claude Code, Hermes, and Cursor agent-adoption/removal rows to post-release qualification. They remain visible gaps, not passes.

## Release path

Freeze the clean Julie and plugin source SHAs. Run the final Linux branch gate and the owner-reserved post-merge Windows full. Publish the reviewed plugin workflow source under an immutable source tag. Push the verified Julie source, tag `v8.0.0`, await the release and plugin workflows, inspect all public assets/source, and run fresh public-install smoke checks.

Because source state changes while recording the waiver and gate evidence, report the final clean exact SHAs immediately before the first push/tag action.

## Deferred work

- Local macOS arm64/Intel fresh-install and upgrade evidence. Rocinante SSH authentication rejected the verified one-use key.
- Claude Code, Hermes, and Cursor real-agent adoption/removal.
- Plan 5C/5D replacement decision work.

## Constraints

Do not trigger qualification-only GitHub Actions. Do not install into live profiles, copy rotating OAuth state, or modify real user repositories. Keep retained evidence secret-free.
