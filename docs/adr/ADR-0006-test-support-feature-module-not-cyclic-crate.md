# ADR-0006: Test-support helpers live in a feature-gated `julie_core::test_support` module, not a cyclic dev-dep crate

## Context

The Phase 0 crate split (see `docs/plans/2026-06-03-julie-rescue-phase0-plan.md`) relocates the
pure database test slice into `julie-core` so it compiles into julie-core's own test binary,
curing the relink tax. Those tests need shared helpers — db row builders
(`file_info_builder`, `symbol_builder`, …), `open_test_connection`, `unique_temp_dir`,
`atomic_cleanup_julie_dir`.

The plan's Task 6 put those helpers in a **separate `julie-test-support` crate** and made it a
**dev-dependency of julie-core** so julie-core's relocated tests could call them. That forms a
dependency cycle:

```
julie-core --(dev-dependency)--> julie-test-support --(normal dependency)--> julie-core
```

Under this cycle, building julie-core's test binary compiles **two distinct instances** of
julie-core: the `--test` instance (the binary under test) and the plain-lib instance that
`julie-test-support` links. Rust treats a type from one crate instance as *different* from the
"same" type in the other instance. So a builder in `julie-test-support` that returns a
`julie_core::database::SymbolDatabase` produces the plain-lib instance's type, which julie-core's
`--test` code refuses to accept — a confusing "expected SymbolDatabase, found SymbolDatabase"
error. This blocks julie-core's own tests from using the shared builders for **julie-core-owned
types** (`SymbolDatabase`, `FileInfo`). Helpers whose signatures touch only external types
(`rusqlite::Connection`, `tempfile::TempDir`, `julie_extractors::*`) unify fine, which is why the
problem is partial and easy to misdiagnose.

## Decision

Host the handler-free test helpers **in julie-core itself**, in a module gated:

```rust
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;   // db::rows builders, open_test_connection, tempdir, cleanup
```

- julie-core's own tests use `crate::test_support::*` — same crate instance, no boundary.
- Downstream consumers (the top `julie` crate's ~20 helper-using suites) enable julie-core's
  `test-support` feature to get them.
- `julie-test-support` is retained as a **thin re-export** (`pub use julie_core::test_support::*;`,
  depending on `julie-core = { features = ["test-support"] }`) so existing `julie_test_support::*`
  import paths and the top-crate shims keep working unchanged.
- **julie-core no longer depends on julie-test-support** — the cycle is gone.
- `tempfile` becomes an *optional* dependency of julie-core enabled by `test-support` (for
  downstream) plus a dev-dependency (for julie-core's own `cfg(test)` build). `resolver = "2"`
  keeps the dev-only feature out of the production `julie` binary.

Rejected alternative: keep the separate crate and re-implement the julie-core-type builders
locally inside julie-core's tests (the worker's first instinct). This re-introduces the helper
duplication that Task 6 existed to remove and leaves the fragile cycle in place.

## Consequences

- **Single source** for the builders; no duplication.
- **No dev-dep cycle, no two-rlib mismatch** — all ~108 database tests relocate into julie-core.
- Production `julie` binary does not link `tempfile` or `test_support` (dev-only feature under
  resolver 2).
- Minor cost: the test-support helpers live in the leaf crate (slightly larger), and `tempfile`
  appears as both an optional `[dependencies]` entry and a `[dev-dependencies]` entry.

## Applies To

- `crates/julie-core` — the `test_support` module + `test-support` feature.
- `crates/julie-test-support` — reduced to a thin re-export of `julie_core::test_support`.
- Top-crate test helper shims (`src/tests/helpers/{db,tempdir,cleanup}`, `open_test_connection`).

## Future Agents

When a leaf crate must share test builders with **both its own tests and downstream crates**, put
them in a **feature-gated module in the leaf** (`#[cfg(any(test, feature = "..."))]`). Do NOT put
them in a separate crate that dev-depends back on the leaf — that cycle triggers the two-rlib type
mismatch and silently blocks the leaf's own tests from using builders for the leaf's own types. A
sibling crate may *re-export* the feature-gated module, but the leaf must never depend on the
sibling.
