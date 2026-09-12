---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-12T18:12:16.134Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

# Machine service and revival hardening

## Direction
Ship Julie v8 as one Rust machine service with stateless HTTP MCP, a stdio shim and JSON API over one request engine. Preserve explicit workspace routing, one writer per checkout, facts.sqlite/Tantivy storage, the OS singleton and existing mutation gate. No leases, brokers, durable cursor store or derived-index migrations.

## Completed work
Plan1 merged locally into main at 22391e26882e54b9e4f5bb0995a69369793e067c. The owner rebuilt and restarted Julie.

Plan2 merged locally into main at cca0276078a51c6b1e03add3051fe72affd3a1ce. It adds precise reference sites, scoped content retrieval, self-contained stateless paging, and bounded source-hash-bound bodies with explicit completeness. The final Linux branch gate passed formatting, clippy, 2283 development tests, 13 CLI tests, and 69 dogfood tests. The isolated live probe passed seven public API, MCP, and CLI retrieval checks.

Windows exact checks validated the Plan2 behavior and several portability fixes. Repeated Windows full qualification exposed unrelated and fixture failures one at a time, so qualification is incomplete and separated from Plan2 implementation. Do not restart that loop inside later feature plans.

## Verification policy
Use exact tests during implementation. Broad gates run once after code freeze, with one retry after exact in-scope fixes. A third broad run requires an explicit owner decision. Unrelated, pre-existing, platform-only, and fixture-only failures become separate work. Unclear ownership gets at most 15 minutes or one exact diagnostic. Evidence-only descendant commits may reuse passing evidence when their diff cannot affect the command.

## Constraints and later plans
Preserve existing tools, canonical evidence and all-language applicability. No fake exact spans or inferred complete values. Paging retains automatic backend policy. Source hashes prevent body pages from mixing source versions; no retained snapshots or cursors.

Plans3-5 remain proposals: production runtime lifecycle, installation and guidance qualification, and measured Miller replacement. Keep the phase6b naming decision after sufficient telemetry, no earlier than approximately 2026-09-25. Existing call_path web mode remains the bridge path. Navigation promotion, full Miller retirement, plugin publication, and release remain separate owner decisions.

## References
docs/plans/2026-09-11-revival-roadmap.md
docs/plans/2026-09-11-revival-retrieval-plan.md
docs/findings/2026-09-12-windows-baseline.md
.memories/autonomous-run-2026-09-12-revival-correctness.md
