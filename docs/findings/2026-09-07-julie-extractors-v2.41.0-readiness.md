# julie-extractors v2.41.0 consumer readiness

## Released identity

- Published release: [v2.41.0](https://github.com/anortham/julie-extractors/releases/tag/v2.41.0), 2026-09-07 20:16:20 UTC; neither draft nor prerelease.
- Annotated tag object: `b8b9f5498e37397e54ab8aac1e68e2d8936188f1`.
- Peeled source commit for the dependency: `a71e18c1a6fae67b15d2d0aaa20793330fda901f`.
- Inspected/tested local main: `5912bafe185ce2653ce22f86ec962aef615a27f8`. Diff from the tag contains publication documentation and memory only. The extractor crate source, manifests and lockfile are identical to the tag.
- Remote tag identity verified with `git ls-remote`; publication verified with `gh release view`.

## Contract review

The released optional `syntax-api` exports match the Julie migration plan: `parse_source`, `parse_source_with_options`, `ParsedSource`, `SyntaxOptions`, and all six `SyntaxError` variants. `PendingSpan` and `UnresolvedTarget` are publicly re-exported while implementation modules remain private. Default features do not enable the syntax module.

Inspected parsing orchestration, parser progress cancellation, strict and legacy C/C++ header selection, iterative diagnostics, and public exports. Bounded parsing checks cancellation/deadlines during parser progress, header scans/scoring, diagnostic traversal and before successful return. The diagnostic collector prunes error-free subtrees, not by a depth limit. Header scoring deliberately retains the existing depth policy to preserve detection behavior.

Host-only trees and JSONL refusal remain explicit. No source-body field or old manager was restored. Those are still Julie migration responsibilities. Extraction identity epoch remains 9; extraction-contract text is unchanged from v2.40.6.

No contract blocker was found in this inspected consumer boundary. This is not a claim that Julie already compiles against the release or that the entire upstream release was independently recertified here.

## Fresh verification

Executed serially in the clean upstream main checkout at `5912bafe185ce2653ce22f86ec962aef615a27f8` on Linux:

| Command | Result | Scope |
|---|---|---|
| `cargo test -p julie-extractors --features syntax-api --test syntax_api_contract` | 8 passed, 0 failed | Public syntax, all supported entries, Unicode/CRLF, host composition, typed interruption inputs |
| `cargo test -p julie-extractors --features syntax-api --lib tests::syntax_api_faults` | 14 passed, 0 failed | In-flight cancellation/deadlines, header failures and reuse, deep diagnostics, path and input-size faults |

The source-equality check against v2.41.0 was separate from execution. These runs are recorded at the actual tested HEAD, not mislabeled as executions on the tag. No implementation files changed during verification.

The upstream [publication evidence](../../../julie-extractors/docs/release-evidence/2026-09-07-v2-41-0-release.md) separately records its contract, public relationship, downstream feature-boundary and four-platform release checks. Those are reported upstream evidence, not fresh reruns by this review. Windows optional-feature tests and the full upstream certification suite were not rerun here.

## Downstream handoff

Use tag `v2.41.0` consistently in all six Julie manifests and confirm the lockfile resolves the peeled commit above. Enable `syntax-api` for the tools consumer. Continue with [J1](../plans/2026-09-07-julie-extractor-migration.md), retaining its atomic migration, source projection, receiver facts, syntax safety and index invalidation gates. Do not substitute a simple package bump for that plan.

Julie production remains at v2.34.3. This session updated only planning/readiness evidence and memory; it did not begin the worker implementation.
