---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-12T21:37:56.257Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

# Machine service and revival hardening

## Direction

Ship Julie v8 as one Rust machine service with stateless HTTP MCP, a stdio shim and JSON API over one request engine. Preserve explicit workspace routing, one writer per checkout, facts.sqlite/Tantivy storage, the OS singleton and existing mutation gate. No leases, brokers, durable cursor store or derived-index migrations.

## Completed work

Plans 1 and 2 are merged locally into main. Plan 3 is merged at `53d177f1`, with post-merge evidence recorded at `347e88b2`. Runtime initialization now uses per-key slots, idle runtimes retire through service-owned maintenance, shutdown drains watchers and embedding writers, durable indexes and vectors survive reopen, and the unused alternate runtime manager is gone.

Linux post-merge formatting and `cargo xtask test full` passed. All seven Plan 3 lifecycle tests passed on Windows/NTFS. Windows full qualification remains incomplete because two untouched xtask tests assume Unix TOML-path escaping or command quoting; track those separately from Plan 3.

## Verification policy

Use exact tests during implementation. Broad gates run once after code freeze, with one retry after exact in-scope fixes. A third broad run requires an explicit owner decision. Unrelated, pre-existing, platform-only, and fixture-only failures become separate work. Evidence-only descendant commits may reuse passing evidence when their diff cannot affect the command.

## Constraints and later plans

Preserve existing tools, canonical evidence and all-language applicability. No fake exact spans or inferred complete values. Paging retains automatic backend policy. Source hashes prevent body pages from mixing source versions; no retained snapshots or cursors.

Plans 4 and 5 remain: installation and guidance qualification, then measured Miller replacement. Keep the phase6b naming decision after sufficient telemetry, no earlier than approximately 2026-09-25. Existing call_path web mode remains the bridge path. Navigation promotion, full Miller retirement, plugin publication and release remain separate owner decisions.

## References

- docs/plans/2026-09-11-revival-roadmap.md
- docs/plans/2026-09-11-revival-runtime-plan.md
- docs/plans/2026-09-11-revival-retrieval-plan.md
- docs/findings/revival-runtime-baseline.md
- docs/adr/ADR-0007-request-runtime-lifecycle-owner.md
