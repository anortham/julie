# Julie Extractor Data Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Improve the trustworthiness and product usefulness of tree-sitter extractor data without turning the extractor contract into broad schema churn.

**Architecture:** Treat `julie-extractors` as a public data contract used by Julie and downstream consumers such as Eros. Changes are additive first, proven through existing capability/certification evidence, and only promoted to canonical fields after Julie persistence and downstream adapters can consume them. The plan is organized around four product-quality milestones: trust, stability, useful richness, and graph quality.

**Tech Stack:** Rust, tree-sitter native parsers, `julie-extractors`, SQLite, Tantivy, `cargo xtask`, nextest, existing golden fixtures, existing real-world evidence corpus, and Eros as an external canary when available.

**Architecture Quality:** Affected modules are `crates/julie-extractors`, Julie persistence/search/embedding consumers, generated certification evidence, and downstream adapters. Caller-facing interfaces are `julie_extractors::*`, `ExtractionResults`, `capability_snapshot()`, Julie indexed workspace data, MCP tool outputs, and Eros' PyO3 extraction adapter. Risk is high if IDs or kind meanings change silently; this plan keeps `Symbol.id` stable, adds new fields as optional, and requires existing docs/evidence surfaces to be updated instead of creating parallel documentation.

---

## Documentation Source of Truth

Do not create new tree-sitter quality docs for this work unless an existing canonical doc truly cannot hold the information.

Authoritative docs and artifacts:

1. `docs/TREE_SITTER_QUALITY_BAR.md` - human rubric for what quality means.
2. `docs/EXTRACTION_CONTRACT.md` - downstream-facing `julie-extractors` API and data contract.
3. `fixtures/extraction/capabilities.json` - machine-checked per-language capability truth.
4. `docs/LANGUAGE_CERTIFICATION_REPORT.md` - generated certification report; do not hand-edit except through generator changes.
5. `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` - generated real-world evidence.
6. `docs/TREE_SITTER_UPGRADES.md` - parser dependency upgrade policy and history only.

Plan docs under `docs/plans/` are proposals or historical handoffs, not current authority. If this plan changes the quality bar, update `TREE_SITTER_QUALITY_BAR.md`. If it changes public output shape, update `EXTRACTION_CONTRACT.md`. If it changes evidence, update `capabilities.json` and the generated reports.

## Core Rules

- Additive first. New extractor fields start as optional. Existing fields stay populated until all in-repo consumers and Eros can read the new shape.
- No `Symbol.id` replacement in this plan. Add `semantic_id`, `location_id`, and `body_hash`; keep current `id` semantics stable.
- No mass kind reclassification. Do not turn existing `function` results into `lambda` or `macro` until old filters and consumers have compatibility aliases.
- No new standalone quality docs. Use existing docs and generated reports.
- No "relationship zoo" unless a product workflow needs it and a fixture proves it.
- Every milestone must prove data survives Julie's full pipeline: extraction, SQLite persistence, row reads, Tantivy projection when searchable, embedding text when semantic, MCP output when visible, and downstream adapter behavior when Eros uses it.

## Compatibility Surface

Julie currently stores old extractor fields through:

- `src/database/helpers.rs` - symbol column list and row conversion.
- `src/database/schema.rs` - SQLite schema.
- `src/database/symbols/storage.rs` and `src/database/bulk_operations.rs` - write paths.
- `src/search/index.rs` - Tantivy symbol projection.
- `src/embeddings/metadata.rs` - embedding text.
- `src/tools/**` - MCP formatting and graph/navigation behavior.

Eros currently depends on `julie-extractors` through `../julie/crates/julie-extractors` and wraps old fields in `~/source/eros/src/lib.rs`. Its normalizer derives Eros node/symbol IDs from raw Julie symbol IDs in `~/source/eros/python/eros/extractors/normalizer.py`. Eros is young enough to change, but changes must be intentional and tested.

## Migration and Versioning Strategy

`EXTRACTION_CONTRACT_VERSION` remains the downstream signal for extractor output-shape changes.

`SEMANTIC_INDEX_ENGINE_VERSION` must embed `EXTRACTION_CONTRACT_VERSION`. Current code stores the constant in `src/tools/workspace/indexing/engine_version.rs`; Milestone 1 must either make that composition explicit or update the constant and its engine-version test in the same commit.

Version bumps happen only at milestone boundaries:

| Milestone | Contract version codename | Purpose |
| --- | --- | --- |
| 1 | `trust-contract-v1` | Capability/evidence contract, doc source cleanup, Eros canary |
| 2 | `additive-identity-v1` | Optional semantic identity and body hash, no `id` replacement |
| 3 | `useful-richness-v1` | Structured signatures/modifiers/test role under the all-language capability contract |
| 4 | `graph-quality-v1` | Product-driven graph and identifier quality improvements |

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TREE_SITTER_QUALITY_BAR.md`, `docs/EXTRACTION_CONTRACT.md`, `fixtures/extraction/capabilities.json`, `docs/LANGUAGE_CERTIFICATION_REPORT.md`, and `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json`.

**Worker red/green scope:** exact tests only: `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10` or `cargo nextest run -p julie-extractors --lib <narrow_filter>` when the exact fixture module is the narrowest meaningful scope.

**Worker ceiling:** workers may run only the narrow extractor or unit test they own. Workers do not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test full`, or real-world release evidence.

**Worker gate invariant:** every assigned test must name the data contract it proves: capability claim, persistence field survival, stable identity behavior, structured field output, graph precision, or downstream adapter compatibility.

**Lead affected-change scope:** `cargo xtask test changed` after each coherent batch.

**Branch gate:** `cargo xtask test dev` before handoff. Add `cargo xtask test system` when schema, workspace repair, startup, watcher, or persistence semantics change. Add `cargo xtask test dogfood` when graph/search/navigation behavior changes. Run `cargo xtask test full` for release-candidate confidence.

**Replay/metric evidence:** `cargo xtask certify tree-sitter --check` is a hard gate for capability/certification changes. `cargo xtask certify tree-sitter --real-world --profile release --out docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` is a hard gate when changing real-world evidence or release-profile thresholds.

**Escalation triggers:** changing public structs, `Symbol.id` derivation, `Identifier.id` derivation, `RelationshipKind`/`SymbolKind`/`IdentifierKind` meanings, SQLite schema, Tantivy projection, embedding text recipes, or downstream adapter contracts.

**Assigned verification failure:** workers stop and report when assigned verification fails. They do not reinterpret failing capability, parser, or downstream canary gates without lead review.

**Verification ledger:** milestone closing evidence goes into `docs/TREE_SITTER_QUALITY_BAR.md`'s `## Verification Ledger` section only when the command and commit SHA match.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** milestone design, contract shape, lead review, finding triage, and gate interpretation.
- Harness mapping: follow `RAZORBACK.md`.

**Implementation tier:** single-language extractor changes, bounded persistence plumbing after the contract is fixed, focused fixture updates.
- Harness mapping: follow `RAZORBACK.md`.

**Mechanical tier:** doc wording, generated report plumbing after schema is fixed, fixture file renames, ledger row authoring from already-captured evidence.
- Harness mapping: follow `RAZORBACK.md`.

**Gate-interpretation reviewer:** failing capability matrix, parser-upgrade, Eros canary, or certification evidence where the correct answer could be "the plan is wrong."
- Harness mapping: follow `RAZORBACK.md`.

**Escalation tier:** identity design, schema migrations, downstream compatibility, search/projection semantics, graph precision, and any public API compatibility question.
- Harness mapping: follow `RAZORBACK.md`.

**Worker eligibility:** implementation-tier workers may own a task only when the public contract for that task is already fixed, write scope is narrow, and the assigned verification is exact.

**Mechanical exclusion:** mechanical workers cannot own failing tests, replay evidence, metrics, capability interpretation, or Eros compatibility decisions.

## Milestone 1 - Trust Contract

**Goal:** Make extractor claims easy to trust without creating more documents.

**What changes:**
- Extend `capabilities.json` and `capability_snapshot()` from five coarse booleans to a per-kind coverage matrix for existing kinds only.
- Add depth coverage to the existing tree-sitter certification report.
- Strengthen downstream smoke so public fields and capability snapshot shape are exercised.
- Add an Eros canary path for lead-owned verification when `~/source/eros` is present.

**Files:**
- Modify: `fixtures/extraction/capabilities.json`
- Modify: `crates/julie-extractors/src/capability_snapshot.rs`
- Modify: `crates/julie-extractors/src/lib.rs`
- Modify: `crates/julie-extractors/src/tests/capability_matrix.rs`
- Create or modify: `crates/julie-extractors/src/tests/capability_matrix_per_kind.rs`
- Modify: `xtask/src/tree_sitter_certification*.rs` and supporting xtask modules
- Modify: `docs/EXTRACTION_CONTRACT.md`
- Modify: `docs/TREE_SITTER_QUALITY_BAR.md` only for quality-bar or ledger changes
- Regenerate: `docs/LANGUAGE_CERTIFICATION_REPORT.md`

**Tasks:**

### Task 1.1 - Schema Transition Without Mixed-Row Breakage

Introduce the new `CapabilityMatrix` shape with either:

- a one-commit migration of all rows, or
- a temporary deserialization enum that accepts legacy `CapabilityFlags` and new `CapabilityMatrix`, then normalizes to one public shape.

Do not migrate only Rust while other rows stay in an incompatible old shape unless the compatibility enum exists first.

Acceptance criteria:
- `capability_snapshot()` deserializes every row.
- Public downstream API compiles.
- `CapabilityFlags` remains as a compatibility alias only if tests prove downstream code still compiles.

### Task 1.2 - Per-Kind Evidence For Existing Kinds

Record per-kind coverage for current `SymbolKind`, `RelationshipKind`, and `IdentifierKind` variants only. Do not introduce new variants in this milestone.

Acceptance criteria:
- A supported per-kind claim must appear in fixture output.
- `NotApplicable` means the concept does not exist for that language.
- `OpenGap` means the concept exists but Julie does not emit it.
- Every `OpenGap` has a concrete planned closure reference in `capabilities.json`.

### Task 1.3 - Certification Report Integration

Extend `cargo xtask certify tree-sitter` so the existing `docs/LANGUAGE_CERTIFICATION_REPORT.md` carries depth coverage.

Acceptance criteria:
- No `docs/CAPABILITY_DEPTH.md`.
- `cargo xtask certify tree-sitter --check` fails when the checked-in report is stale.
- Report generation fails on malformed gap evidence.

### Task 1.4 - Downstream Canary

Broaden `crates/julie-extractors/tests/downstream_smoke.rs` to compile and run a tempdir crate that reads the capability matrix and basic extractor output. Add a lead-owned Eros canary command to the milestone ledger when `~/source/eros` is present.

Acceptance criteria:
- Downstream smoke passes as part of the extractors bucket.
- Eros canary result is recorded as passed, failed, or unavailable with reason; unavailable is not a substitute for Julie's in-repo downstream smoke.

## Milestone 2 - Additive Identity and Body Hash

**Goal:** Give Julie and Eros stable tracking data without breaking current `id` users.

**What changes:**
- Add optional `semantic_id`, `location_id`, `body_span`, and `body_hash` to `Symbol`.
- Keep current `Symbol.id` unchanged.
- Persist the new fields through Julie SQLite and row reads.
- Expose the fields through MCP/search output only where useful and documented.
- Prove `body_span` and `body_hash` through the same all-language capability contract used for existing extractor claims.
- Let Eros consume the new fields when ready, while old raw IDs still work.

**Files:**
- Modify: `crates/julie-extractors/src/base/types.rs`
- Create: `crates/julie-extractors/src/base/identity.rs`
- Modify: `crates/julie-extractors/src/base/creation_methods.rs`
- Modify: `crates/julie-extractors/src/base/results_normalization.rs`
- Modify: `src/database/schema.rs`
- Modify: `src/database/helpers.rs`
- Modify: `src/database/symbols/storage.rs`
- Modify: `src/database/bulk_operations.rs`
- Modify: `src/search/index.rs` only if a field is searched or displayed.
- Modify: `src/embeddings/metadata.rs` only if a field affects embedding text.
- Modify: `docs/EXTRACTION_CONTRACT.md`

**Tasks:**

### Body Span and Body Hash Contract

`body_span` is a source-coordinate range for the body-bearing part of a symbol. It uses the same coordinate rules as `NormalizedSpan`: 1-based lines, 0-based columns, and byte offsets into the original file content after embedded-language offsets are applied.

A valid `body_span` must:
- Belong to the same file as the symbol.
- Be contained by the symbol's declaration span.
- Exclude leading documentation, comments, decorators, attributes, annotations, and the symbol name/signature/header when the grammar exposes a separable body.
- Include the complete grammar body node when the body syntax has delimiters or indentation that are required to replace the body safely. For brace languages, this normally means the block node including braces. For indentation languages, this means the suite/block statements. For declarative languages, this means the member/declaration region represented by the grammar body node.
- Be absent for symbols without a body: variables, constants, imports, exports, enum members without bodies, fields, aliases, schema/table references, headings, and other leaf records.

`body_hash` is a deterministic digest of the normalized token stream inside `body_span`. The hash ignores whitespace-only formatting changes but includes non-whitespace token text, comments, literals, identifiers, operators, punctuation, and delimiters that fall inside `body_span`. If `body_span` is absent, `body_hash` must also be absent.

Capability rules:
- `capabilities.json` must gain a body-span/body-hash evidence domain for every language row.
- `supported` means fixture evidence proves every applicable emitted body-bearing symbol kind for that language has correct `body_span` and `body_hash`.
- `not_applicable` means the language row has no emitted body-bearing symbols for that concept, or the format is non-code and body replacement has no coherent meaning.
- `open_gaps` means the language emits at least one body-bearing symbol kind whose body span/hash is not proven. Public support is blocked while any applicable language has an open gap.
- Downstream consumers, including Eros, must read the capability matrix. They must not hardcode per-language allowlists.

### Task 2.1 - Add Fields Without Changing IDs

Add fields as optional data:

- `semantic_id: Option<String>`
- `location_id: Option<String>`
- `body_span: Option<BodySpan>`
- `body_hash: Option<String>`

`location_id` should equal today's location-based identity. `semantic_id` should be collision-resistant enough for same-name overloads before it is trusted by downstream code. If parameter types are not available, include a documented collision guard and keep consumers on `id`.

`body_span` means the executable/declarative body portion for body-bearing symbols. Non-body symbols and non-code formats must be recorded as `not_applicable`, not quietly omitted. Code languages must not receive public body-span/body-hash support until every applicable language has fixture-backed coverage or a documented exception in the capability matrix.

Acceptance criteria:
- Moving a symbol down by lines preserves `semantic_id` but changes `location_id`.
- Token changes change `body_hash`.
- Formatting-only changes do not change `body_hash` when tokens are unchanged.
- `Symbol.id` does not change.
- `cargo xtask certify tree-sitter --check` reports `0 open_gaps` for body-span/body-hash coverage across all applicable languages before any downstream consumer treats the fields as supported.

### Task 2.2 - Persist and Read Back

Add SQLite columns and update all write/read paths for the new fields.

Acceptance criteria:
- Full indexing stores fields.
- Incremental indexing stores fields.
- Row-to-symbol conversion returns fields.
- Schema migration/repair is covered by system tests when columns are added.

### Task 2.3 - Eros Compatibility

Update the plan evidence to prove Eros can either ignore the new fields or consume them explicitly.

Acceptance criteria:
- Eros existing extraction tests still pass when using the updated Julie extractors, or the Eros adapter changes are recorded as required and tested in Eros.
- Eros consumes Julie's capability matrix for body-span/body-hash support instead of hardcoding language allowlists.
- Eros does not depend on `semantic_id` until collision behavior is proven.

## Milestone 3 - Useful Symbol Richness

**Goal:** Add structured fields that directly improve product workflows: inspection, search, summaries, editing anchors, and test awareness without creating first-class and second-class language tiers.

**Language support rule:** New public extractor capabilities are all-language contracts. A language-specific implementation order is allowed internally, but the capability is not considered supported until the capability matrix records fixture-backed `supported`, `not_applicable`, or documented exception status for every language.

**What changes:**
- Add optional structured fields:
  - `parameters`
  - `returns`
  - `generic_params`
  - `modifiers`
  - `test_role`
- Keep `signature` populated as the readable derived view.
- Keep old metadata mirrors until Julie and Eros read the typed fields.

**Files:**
- Create: `crates/julie-extractors/src/base/symbol_richness.rs`
- Modify: `crates/julie-extractors/src/base/types.rs`
- Modify: `crates/julie-extractors/src/base/creation_methods.rs`
- Modify extractor files that already compute this information, then complete the capability matrix for every language before publishing the capability.
- Modify fixture expected JSON and generated evidence for every applicable language row.
- Modify persistence/read/search/embedding paths only for fields that Julie actually stores or uses.
- Modify `docs/EXTRACTION_CONTRACT.md`

**Tasks:**

### Task 3.1 - Define Typed Fields

Add common data types for parameters, generic params, modifiers, and test role. Keep type references as source strings for now; do not build a full `TypeRef` tree in this milestone.

Acceptance criteria:
- New fields serialize and deserialize.
- Public API re-exports are available from `julie_extractors::*`.
- Existing consumers compile without reading the new fields.

### Task 3.2 - Implement Through the Capability Matrix

Use one language as an internal worked example only when it reduces implementation risk. Do not publish or let downstream consumers rely on the new structured fields until every language row has a fixture-backed `supported`, `not_applicable`, or documented exception status.

Acceptance criteria:
- Typed fields match existing signature/metadata information.
- No metadata key is removed in the same milestone that introduces its typed replacement.
- Fixture assertions and generated certification evidence prove typed-field status for every language, with `0 open_gaps` before the fields are treated as supported.

### Task 3.3 - Search and Embedding Use

Only add fields to Tantivy or embeddings when they improve existing product behavior.

Acceptance criteria:
- Search still indexes a human-readable signature.
- Embedding text does not lose useful signature/doc/context vocabulary.
- Any new searchable text has a search-quality or dogfood proof.

## Milestone 4 - Product-Driven Graph Quality

**Goal:** Improve graph correctness where Julie and Eros need it, without broad new enum churn.

**What changes:**
- Improve existing calls/imports/references/identifiers precision.
- Improve pending relationship context where cross-file resolution needs it.
- Add embedded-language provenance only where host/embedded coordinate behavior is already proven.
- Add new relationship or identifier kinds only when an existing product workflow cannot be represented safely.

**Files:** selected per task after exact product workflow is named. Likely areas:
- `crates/julie-extractors/src/<language>/relationships.rs`
- `crates/julie-extractors/src/<language>/identifiers.rs`
- `crates/julie-extractors/src/base/relationship_resolution.rs`
- `crates/julie-extractors/src/base/embedded_span.rs`
- `src/tools/navigation/**`
- `src/tools/impact/**`
- `src/tools/get_context/**`
- `docs/EXTRACTION_CONTRACT.md`
- `fixtures/extraction/capabilities.json`

**Tasks:**

### Task 4.1 - Pick Product Workflows First

Before adding any relationship kind, name the workflow it improves:

- call path quality
- blast radius quality
- affected tests
- code inspection
- edit anchoring
- Eros retrieval

Acceptance criteria:
- Every graph change has a fixture showing a correct edge or a deliberately unresolved pending edge.
- Wrong confident edges are blockers.
- Existing filters keep working.

### Task 4.2 - Improve Existing Kinds Before Adding New Ones

Prefer better `Calls`, `Imports`, `References`, `Contains`, `Extends`, `Implements`, and `Overrides` behavior before introducing new enum variants.

Acceptance criteria:
- Existing MCP tools improve or stay stable.
- Compatibility aliases exist before any user-visible kind split.
- Dogfood runs when graph/search/navigation behavior changes.

### Task 4.3 - Embedded Provenance With Host Coordinates

If adding provenance for Vue/HTML/Razor/Markdown, primary spans remain host-file coordinates so navigation and edits work. Add embedded-local range separately if needed.

Acceptance criteria:
- Host coordinates are correct.
- Parent IDs and relationships are rekeyed after offsets.
- Fixtures cover duplicate embedded text and section offsets where relevant.

## Not In This Plan

- Moving `julie-extractors` to its own repo.
- Replacing `Symbol.id`.
- Full structured `TypeRef` parsers for every language.
- Mass `Function` to `Lambda` or `Macro` reclassification.
- Broad new relationship kinds such as `Throws`, `Catches`, `Yields`, `Awaits`, `MacroExpands`, `Specializes`, unless a product workflow demands one and compatibility is planned.
- SCIP/LSIF export.
- Range-incremental extraction API.
- Making every language emit every rich field.

## Worktree and Sequencing

Recommended sequence:

1. Milestone 1 - Trust Contract.
2. Milestone 2 - Additive Identity and Body Hash.
3. Milestone 3 - Useful Symbol Richness.
4. Milestone 4 - Product-Driven Graph Quality.

Milestones are intentionally sequential. Milestone 2 depends on Milestone 1's contract/versioning discipline. Milestone 3 depends on Milestone 2's persistence discipline. Milestone 4 depends on a trustworthy capability matrix and stable identity fields.

Recommended worktrees:

```bash
git worktree add .worktrees/extractor-trust-contract origin/main
git worktree add .worktrees/extractor-additive-identity origin/main
git worktree add .worktrees/extractor-useful-richness origin/main
git worktree add .worktrees/extractor-graph-quality origin/main
```

Estimated scope:

- Milestone 1: 2-4 days.
- Milestone 2: 2-4 days.
- Milestone 3: estimate after the all-language capability matrix is scoped; no priority-language carveout.
- Milestone 4: task-sized batches only after product workflows are named.

## Approval

Plan saved to `docs/plans/2026-05-11-julie-extractors-best-in-class-data-model.md`. Review for scope and correctness before implementation. Approval should mean: no new quality docs, no `id` replacement, existing docs/evidence are the authority, and Eros compatibility is a canary rather than an afterthought.
