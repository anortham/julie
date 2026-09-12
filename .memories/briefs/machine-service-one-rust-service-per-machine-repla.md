---
id: machine-service-one-rust-service-per-machine-repla
title: "Machine service: one Rust service per machine replaces per-session
  Miller and Julie"
status: active
created: 2026-09-09T21:12:03.049Z
updated: 2026-09-12T01:00:49.178Z
tags:
  - machine-service
  - architecture
  - complexity-rule
---

# Machine service: one Rust service per machine replaces per-session Miller and Julie

## Direction

Ship v8 as one Rust service per developer machine with stateless MCP over HTTP, a stdio shim, and a JSON API over the shared request engine. The approved design is `docs/plans/2026-09-09-machine-service-design.md`.

## Non-negotiable architecture

- Every workspace-scoped MCP tool requires an explicit absolute workspace path or registered workspace ID. The shim never infers a project from its startup directory, cwd, or `JULIE_WORKSPACE`; ordinary terminal CLI commands may resolve cwd.
- Each checkout keeps `facts.sqlite` and `tantivy/`; one registry and one service process exist per machine.
- The only coordination exception is an OS-held `service.lock` for the service lifetime. Do not add PID election, stale-lock recovery, per-checkout locks, brokers, leases, or migrations for derived indexes.
- Preserve the proven tool surface unless telemetry supports a rename. Phase 6b is deferred until about 2026-09-25 after two weeks of telemetry against `docs/findings/2026-09-11-machine-service-phase6a-gate.md`. Continuous testing requires its own design review.

## Integration status (2026-09-12)

The owner authorized local integration. Julie `main` now contains the full deployment story and audit at `cda68bed`; the companion plugin `main` contains `db4dd894`. Both were fast-forward merges. No push, tag, publication, deployment, or release was authorized or performed.

Committed-source verification at `cda68bed`: full gate 5/5 in 27.9s; 2,221 development, 13 CLI, and 69 dogfood tests; no process leaks. The preceding audit also passed formatting and clippy. The plugin main at `db4dd894` passed all 22 tests. Fresh isolated service startup and workspace/edit probes passed. See `docs/findings/2026-09-11-v8-architecture-audit.md`.

The plugin changes are on local `main` in `~/source/julie-plugin`. Its recorded `origin/main` ref has an unrelated pre-existing root history; reconcile that publication path before a push. Fresh plugin installation and native macOS/Windows archive builds remain unverified and must be checked before release.

## Product sequence

1. Semantic-by-default search — merged.
2. Registry and index hygiene — merged.
3. Test speed — merged.
4. Deployment story — merged locally to main; publication remains unapproved.
5. Code cleanup.
6. Dashboard.
7. Dashboard troubleshooting and bug-report evidence.

Each later item gets its own brainstorm, plan, worktree, and gate. Push and release remain owner approval boundaries.

## Known follow-ups

- Service memory remains unbounded by an LRU and was observed at 1.4–2.9 GB with multiple checkouts.
- `RuntimeFactory::acquire` holds the runtimes write lock during cold initialization.
- Paging trailers can omit workspace and non-default filters.
- Partial semantic vector backfill does not resume after restart.
- A running old stdio shim rejects a newer service until its harness restarts.
