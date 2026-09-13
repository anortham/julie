---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-13T16:33:24.282Z
tags:
  - revival
  - plan-4
  - plan-5
  - qualification
  - blocked
---

# Julie revival Plans 4 and 5

## Goal

Complete Plan 4 external qualification, then run Plan 5C and produce the Plan 5D decision ledger.

## Completed

Plans 1-3 are merged on local `main`. Historical Plan 4 Linux archive source `055645e02ebf261355c7db6659a9fe1b5159a791` passed `cargo xtask test full` 5/5 in 43.9 seconds. Its archive SHA-256 is `9d9095713c3572407722ad105ada7a85fd193619751271d10eb8f23a0fe89bba`; Codex, OpenCode, and Antigravity/AGY real-client workflows pass with isolated removal checks.

The current Plan 4 source lineage passed `cargo xtask test full` 5/5 in 27.7 seconds at exact SHA `1ff96903eed69d136e7a89a6dc44f2223fbc7be3` after one in-scope docs-contract correction and its single allowed retry. Later commits change qualification scripts, workflow contracts, and evidence only; a third broad run requires an owner decision.

Local Windows qualification now passes on the disposable `win-test` VM. Commit `163804df` keeps the packaged service alive while offline semantics finish; `a9f9c66c` waits for service discovery removal; `67a37cb3` moves Windows lifecycle verification into the shared archive verifier. The Windows archive `julie-v8.0.0-x86_64-pc-windows-msvc.zip` has SHA-256 `5510d7c1cdff9418260def83a4b071196bdaa2ed7a029cc3e89d02f7bf3f4bfe`. Build, archive verification/lifecycle, cached/offline native semantics, and NTFS executable-lock qualification all pass locally. No GitHub Actions run was requested after the owner directed local VM qualification.

Plan 5A and 5B remain complete on `fix/revival-qualification`; the corrected runner was full-gated at `2297f57015e699b05811803d8c71b5736ffc66b4`.

## Remaining qualification

- macOS arm64 and Intel native package qualification remain open because no local Mac runner is available and hosted reruns are no longer the chosen path.
- Final Claude Code evidence needs refreshed disposable OAuth; Cursor needs a disposable API key; Hermes needs interactive OAuth. Do not copy rotating live OAuth credentials again.
- No genuine prior compatible versioned v8 archive exists. The owner must provide one or explicitly waive that initial-v8 upgrade row.
- Publication-only public snapshot and reusable-workflow checks remain pending separate publication authority.
- Windows full is intentionally reserved for final post-merge validation.

## Authority

Do not trigger more GitHub Actions, push, tag, release, publish, force-push, install into live profiles, spend on external models, or modify real user repositories.

## Plan 5 gate

Plan 5C remains blocked until Plans 1-4 have an integrated native-qualified release candidate and the remaining macOS, client-authorization, and upgrade gates are disposed. Do not fabricate 5C or 5D results.
