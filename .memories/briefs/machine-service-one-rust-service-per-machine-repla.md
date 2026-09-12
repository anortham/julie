---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-12T13:29:14.598Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

# Machine service and revival hardening

## Direction
Ship Julie v8 as one Rust machine service with stateless HTTP MCP, a stdio shim and JSON API over one request engine. Preserve explicit workspace routing, one writer per checkout, facts.sqlite/Tantivy storage, the OS singleton and existing mutation gate. No leases, brokers, durable cursor store or derived-index migrations.

## Approved work
Plan1 is complete and merged locally into main at22391e26882e54b9e4f5bb0995a69369793e067c. Exact-HEAD formatting, clippy, full2245+13+69 tests and isolated native50-to-51 recovery/mode/freshness probe passed. The owner rebuilt and restarted Julie.

Plan2 was approved on2026-09-12: precise reference sites, scoped content retrieval, self-contained stateless paging and bounded bodies with completeness/source identity. Work stays in .worktrees/revival-retrieval on fix/revival-retrieval, branched from22391e26. Sol owns implementation; Astra owns architecture, inline review and integration. Tasks2A/2B edit independently, all Cargo is serialized;2C follows both,2D follows2C. Local commits authorized. Merge/push/tag/release and live profile/service changes require separate authorization.

## Verification
Linux baseline evidence is exact22391e26. Updated win-test supports Julie and the guest is running. That baseline SHA is synced to native NTFS at C:/work/revival-retrieval; Rust1.97 MSVC is present, nextest installation and first Windows baseline are underway. Use win-test CLI only, preserve emitted log paths, run Windows baseline/final gates as lead. No Windows pass is claimed yet.

## Constraints and later plans
Preserve existing tools, canonical evidence and all-language applicability. No fake exact spans or inferred complete values. Paging must retain auto backend policy rather than converting an inferred semantic backend into an explicit symbol-only override. Source hashes protect body pages from mixing versions; no retained snapshots/cursors.

Plans3–5 remain proposals: production runtime lifecycle, installation/guidance qualification and measured Miller replacement. Keep the phase6b naming decision after sufficient telemetry, no earlier than approximately2026-09-25; continuous testing requires its own decision. Existing call_path web mode must be extended, not replaced. Navigation promotion and full Miller retirement are separate owner decisions. Plugin source/publication and native artifact qualification remain pending; no publication is authorized.

## References
docs/plans/2026-09-11-revival-roadmap.md
docs/plans/2026-09-11-revival-retrieval-plan.md
.memories/autonomous-run-2026-09-12-revival-correctness.md
