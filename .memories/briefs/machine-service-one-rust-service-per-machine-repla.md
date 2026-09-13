---
id: machine-service-one-rust-service-per-machine-repla
title: Julie revival Plans 4 and 5
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-13T14:10:43.109Z
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

Plans 1-3 are merged on local `main`. Historical Plan 4 candidate `055645e02ebf261355c7db6659a9fe1b5159a791` passed `cargo xtask test full` 5/5 in 43.9 seconds. Its Linux archive SHA-256 is `9d9095713c3572407722ad105ada7a85fd193619751271d10eb8f23a0fe89bba`; Codex, OpenCode, and Antigravity/AGY real-client workflows pass with isolated removal checks.

Plan 5A and 5B remain complete on `fix/revival-qualification`; the corrected runner was full-gated at `2297f57015e699b05811803d8c71b5736ffc66b4`.

## Native qualification state

Owner-authorized final hosted run `34760490971` tested pushed qualification SHA `8c84e683977b3538e22aebd7f7cf94b2c807edad`. All three targets built and archive-verified. macOS arm64 and Intel then returned auto-mode lexical fallback during cached/offline semantics. Windows stopped earlier when model preparation received Hugging Face HTTP 429. Lifecycle, Windows lock, and artifact upload steps were skipped.

Local commits `ce574773` and `3115c6c8` correct the probe request/fallback contract and preserve real RequestEngine readiness in generic CLI JSON. Exact self-test, exact Rust regression, and `cargo check --bin julie-server` pass. Evidence commit `79a948f8` records the terminal run.

Because product code changed after `055645e`, that SHA is historical rather than the final candidate. A new frozen-tree full gate and another explicitly authorized native run are required. No further hosted run is currently authorized.

## Remaining blockers and authority

- Final Claude Code evidence needs refreshed disposable OAuth; Cursor needs a disposable API key; Hermes needs interactive OAuth. Do not copy rotating live OAuth credentials again.
- No genuine prior compatible versioned v8 archive exists. The owner must provide one or explicitly waive that initial-v8 upgrade row.
- Do not tag, release, publish, force-push, install into live profiles, or modify real user repositories.
- Publication-only public snapshot and reusable-workflow checks remain pending separate publication authority.

## Plan 5 gate

Plan 5C remains blocked until Plans 1-4 have an integrated qualified release candidate and the owner disposes of the remaining client and upgrade gates. Do not fabricate 5C or 5D results.
