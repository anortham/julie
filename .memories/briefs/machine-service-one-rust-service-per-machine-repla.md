---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-13T14:19:39.882Z
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

After native qualification exposed a CLI readiness defect, local commits `ce574773` and `3115c6c8` corrected the probe request/fallback contract and preserved real RequestEngine readiness in generic CLI JSON. Current source candidate `1ff96903eed69d136e7a89a6dc44f2223fbc7be3` passed `cargo xtask test full` 5/5 in 27.7 seconds after one in-scope docs-contract correction and its single allowed retry.

Plan 5A and 5B remain complete on `fix/revival-qualification`; the corrected runner was full-gated at `2297f57015e699b05811803d8c71b5736ffc66b4`.

## Native qualification state

Owner-authorized hosted run `34760490971` tested pushed qualification SHA `8c84e683977b3538e22aebd7f7cf94b2c807edad`. All three targets built and archive-verified. macOS arm64 and Intel then returned auto-mode lexical fallback during cached/offline semantics. Windows stopped earlier when model preparation received Hugging Face HTTP 429. Lifecycle, Windows lock, and artifact upload steps were skipped.

The current source candidate is full-gated locally. Another explicitly authorized native run is still required. No further hosted run is currently authorized.

## Remaining blockers and authority

- Final Claude Code evidence needs refreshed disposable OAuth; Cursor needs a disposable API key; Hermes needs interactive OAuth. Do not copy rotating live OAuth credentials again.
- No genuine prior compatible versioned v8 archive exists. The owner must provide one or explicitly waive that initial-v8 upgrade row.
- Do not tag, release, publish, force-push, install into live profiles, or modify real user repositories.
- Publication-only public snapshot and reusable-workflow checks remain pending separate publication authority.

## Plan 5 gate

Plan 5C remains blocked until Plans 1-4 have an integrated native-qualified release candidate and the owner disposes of the remaining client and upgrade gates. Do not fabricate 5C or 5D results.
