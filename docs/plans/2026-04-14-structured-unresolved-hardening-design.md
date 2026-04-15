# Structured Unresolved Relationship Hardening

**Status:** Design
**Author:** OpenCode
**Date:** 2026-04-14
**Area:** `crates/julie-extractors/src/base/`, `crates/julie-extractors/src/registry.rs`, remaining legacy extractor modules, `crates/julie-extractors/src/tests/`

---

## Problem

The branch already introduced `UnresolvedTarget`, `StructuredPendingRelationship`, canonical plumbing, normalization support, and a first set of migrated extractors. That work proved the model, but the producer side is still inconsistent.

Several extractors still emit raw `PendingRelationship` values with only `callee_name`, which throws away receiver, namespace, import, and caller-scope context before canonical extraction ever sees the edge. That leaves the stack in an awkward middle state: some languages carry rich unresolved-call metadata, others still collapse to the lossy compatibility form.

If we tighten semantic policy now, before the remaining producers are migrated, we risk changing resolution behavior while the input data is still uneven. That is backwards. The next hardening slice needs to finish the data-shape migration first, then add invariant coverage that enforces the contract across the stack instead of relying on language-by-language spot checks.

## Goals

- Migrate the remaining legacy unresolved-call emitters to produce `StructuredPendingRelationship` values first, degrading to `PendingRelationship` only for compatibility.
- Preserve canonical extraction parity: `ExtractionResults` must continue to expose both `pending_relationships` and `structured_pending_relationships`.
- Expand tests from language-specific regressions to shared invariants around normalization, degradation, and identity preservation.
- Keep the migration incremental and low-risk: no broad redesign of relationship resolution in the same slice.

## Non-Goals

- Changing unresolved-edge policy for builtins, stdlib calls, or external package calls.
- Rewriting `PendingRelationship` consumers to require structured data.
- Refactoring unrelated extractor logic while touching each language.
- Solving every unresolved-relationship precision issue in one pass.

## Scope

### Languages in this wave

- `c`
- `cpp`
- `go`
- `rust`
- `python`
- `ruby`
- `dart`
- `zig`
- `gdscript`

### Core files

- `crates/julie-extractors/src/base/creation_methods.rs`
- `crates/julie-extractors/src/base/relationship_resolution.rs`
- `crates/julie-extractors/src/base/results_normalization.rs`
- `crates/julie-extractors/src/registry.rs`
- `crates/julie-extractors/src/tests/relationship_precision.rs`
- `crates/julie-extractors/src/tests/api_surface.rs`
- `crates/julie-extractors/src/tests/path_identity.rs`

### Remaining extractor files

- `crates/julie-extractors/src/c/mod.rs`
- `crates/julie-extractors/src/c/relationships.rs`
- `crates/julie-extractors/src/cpp/mod.rs`
- `crates/julie-extractors/src/cpp/relationships.rs`
- `crates/julie-extractors/src/go/mod.rs`
- `crates/julie-extractors/src/go/relationships.rs`
- `crates/julie-extractors/src/rust/mod.rs`
- `crates/julie-extractors/src/rust/relationships.rs`
- `crates/julie-extractors/src/python/mod.rs`
- `crates/julie-extractors/src/python/relationships.rs`
- `crates/julie-extractors/src/ruby/mod.rs`
- `crates/julie-extractors/src/ruby/relationships.rs`
- `crates/julie-extractors/src/dart/mod.rs`
- `crates/julie-extractors/src/dart/relationships.rs`
- `crates/julie-extractors/src/zig/mod.rs`
- `crates/julie-extractors/src/zig/relationships.rs`
- `crates/julie-extractors/src/gdscript/mod.rs`
- `crates/julie-extractors/src/gdscript/relationships.rs`

### Existing language regression suites to extend

- `crates/julie-extractors/src/tests/c/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/cpp/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/go/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/rust/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/python/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/ruby/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/dart/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/zig/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/gdscript/cross_file_relationships.rs`

## Design

### 1. Migrate producers, not consumers

Each remaining extractor should follow the pattern already established in the JS/TS, OO, and pending-call waves:

1. Build an `UnresolvedTarget` at the point where the extractor knows the most context.
2. Call `BaseExtractor::create_pending_relationship(...)` to create a `StructuredPendingRelationship`.
3. Store the structured record in the extractor's `structured_pending_relationships` collection.
4. Preserve `pending_relationships` by also pushing the degraded `pending` compatibility payload.

This keeps the migration localized. The extractor already knows whether the unresolved target came from a plain identifier, member call, imported alias, namespaced access, or package-qualified symbol. That information should be captured there, before canonical extraction flattens multiple files together.

The compatibility contract stays intact: old consumers can still read `pending_relationships`, while canonical and newer code can use `structured_pending_relationships`.

### 2. Match richness to what the parser exposes

Not every language will populate every `UnresolvedTarget` field, and that is fine. The hard rule is not "fill every field," it is "do not discard context the parser already exposed."

Expected examples:

- Plain cross-file function calls can populate `display_name == terminal_name`, with no receiver or namespace.
- Member calls should preserve receiver when the grammar exposes it.
- Go package calls should preserve package or selector context when available.
- Ruby constant or module-qualified calls should preserve namespace path when available.
- Dart member-access pending calls should preserve receiver and terminal name when the AST exposes both.

The migration should avoid fake precision. If a grammar path only exposes a terminal identifier, store that cleanly rather than inventing namespace or receiver fields.

### 3. Keep normalization and rekey invariants explicit

`ExtractionResults` already carries structured pending relationships through `extend`, record-offset application, and symbol-ID rekeying. This slice should harden that behavior with explicit tests rather than trusting earlier implementation work.

The invariant suite should cover:

- `extend` keeps `structured_pending_relationships` intact.
- `apply_record_offset` updates `pending.line_number` on structured entries.
- `rekey_normalized_locations` updates `pending.from_symbol_id` and `caller_scope_symbol_id` when symbol IDs are refreshed.
- Structured targets remain distinguishable after normalization even when legacy degraded names collide.

This is the point of the model. If normalization or rekeying silently erases caller or target identity, the richer producer work is wasted.

### 4. Add a shared invariant test layer

Current coverage leans on language-specific cross-file tests. Those are useful, but they are noisy and duplicative when validating contract-level behavior.

This slice should strengthen `crates/julie-extractors/src/tests/relationship_precision.rs` into the shared invariant suite for structured unresolved relationships. The suite should remain small and surgical, focused on cross-language guarantees rather than parser quirks.

Language-specific tests should only prove that each migrated extractor emits the structured target fields that its parser can observe. Shared tests should prove that the core contract survives canonical plumbing and normalization.

### 5. Defer policy tightening to the next slice

Once the remaining legacy emitters are migrated, the stack will have a uniform data model. That is the right time to audit builtin calls, external APIs, package-qualified unresolved edges, and any edges that should be dropped instead of retained.

Trying to do that now would mix two kinds of change:

- data shape migration
- semantic policy changes

That combination is how branches become a swamp. This slice should stay disciplined and finish the structural migration first.

## Implementation Outline

1. Add structured pending storage/getters to each remaining extractor that still exposes only legacy pending records.
2. Replace raw `PendingRelationship` creation in unresolved call paths with `BaseExtractor::create_pending_relationship(...)` where applicable.
3. Preserve compatibility by pushing the degraded legacy payload alongside the structured record.
4. Update `registry.rs` entrypoints for any remaining languages that are not yet returning `get_structured_pending_relationships()`.
5. Extend each language's cross-file suite to assert structured target shape for at least one representative unresolved-call path.
6. Expand `relationship_precision.rs` with shared invariants for extend, offset, rekey, and stable degradation behavior.
7. Run narrow RED/GREEN tests during each migration, then run `cargo xtask test dev` once after the full batch.

## Risks And Guardrails

### Risk: accidental semantic drift

Changing unresolved-call producers can silently alter whether a call is emitted as resolved, pending, or absent.

Guardrail:

- TDD per migrated language.
- Keep the call-detection logic unchanged where possible; change only the payload being emitted.

### Risk: fake precision

Some languages expose less AST context than others. Pretending otherwise will create unstable metadata that looks rich but lies.

Guardrail:

- Populate only fields directly supported by the local AST path.
- Assert only those fields in the corresponding language tests.

### Risk: compatibility breakage

Older tests and downstream code still read `pending_relationships`.

Guardrail:

- Every structured record must continue to emit its degraded legacy companion.
- The invariant suite should lock degradation behavior where it matters.

## Acceptance Criteria

- [ ] `c`, `cpp`, `go`, `rust`, `python`, `ruby`, `dart`, `zig`, and `gdscript` produce `structured_pending_relationships` for their unresolved-call paths where context is available.
- [ ] Canonical extraction for those languages returns both `pending_relationships` and `structured_pending_relationships`.
- [ ] Legacy compatibility remains intact: migrated extractors still populate degraded `pending_relationships` records.
- [ ] Existing cross-file suites for the migrated languages assert structured target data for representative pending-call cases.
- [ ] `crates/julie-extractors/src/tests/relationship_precision.rs` covers normalization and identity invariants for structured pending relationships.
- [ ] `crates/julie-extractors/src/tests/api_surface.rs` continues to prove canonical parity for structured pending output.
- [ ] `cargo xtask test dev` passes after the migration batch.

## Test Strategy

- Follow strict TDD for each language or invariant change:
  1. write or tighten a failing test
  2. run the narrow test and confirm RED
  3. implement the smallest producer change
  4. rerun the same narrow test and confirm GREEN
- Use existing language cross-file suites as the RED entrypoint for extractor migrations.
- Use `relationship_precision.rs` for shared invariant RED/GREEN cycles.
- Run `cargo xtask test dev` once after the batch is complete.

## Exit Condition For This Slice

This slice is done when the remaining legacy unresolved-call producers are migrated and the shared invariant suite makes it hard to regress the structured contract.

The next slice can then tackle semantic policy with one uniform input model instead of a half-legacy mess.
