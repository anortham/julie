---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-14T15:53:53.547Z
tags:
  - revival
  - plan-4
  - release
  - v8
  - authorized
---

# Julie revival Plans 4 and 5

## Current goal

Julie v8.0.0 is released. Continue with Plan 5C/5D and the deferred client/macOS qualification work.

## Released state

- Julie release: `v8.0.0` at commit `1eb4977eeee9e1bfced5317cc268bd8676fed254`.
- Public release: https://github.com/anortham/julie/releases/tag/v8.0.0
- Release workflow `34859615247` built and verified all four archives. Its final public-asset step falsely rejected the valid GNU Windows `*filename` checksum format; the verifier regression fix is on main at `4ea5432474c9e7e410f2e16e544aa85b8e88c776`.
- Corrected plugin source: `d427fec9df777b045d4562a4e56e3f14f0a7a260`, immutable tag `v8.0.0-source.1`.
- Plugin publication workflow `34864907371` passed and published plugin main `a51922279f4191f26facdb67fc5c676e4ebd9c49`.
- Julie future-release contract/pin correction is on main at `671efca84c113db54baba6d2cf4196bd2e0d4ca2`.

## Verification

Windows `cargo xtask test full` passed 5/5 commands: 2,291/2,291 dev tests and 69/69 dogfood tests. The public release contains four platform archives plus four checksum files and substantive notes matching `docs/release-notes/v8.0.0.md`. The published plugin has six manifests at 8.0.0, exact archive hash parity with the release, required MCP/hook/instruction files, no symlinks, and no extracted target directories.

## Remaining work

- Local macOS arm64/Intel fresh-install and upgrade evidence.
- Claude Code, Hermes, and Cursor real-agent adoption/removal.
- Plan 5C/5D replacement decision work.
- Hosted native-semantic qualification probes remain vulnerable to external Hugging Face HTTP 429; the release archive workflow itself succeeded.

## Constraints

Do not install into live profiles, copy rotating OAuth state, or modify real user repositories. Keep retained evidence secret-free.
