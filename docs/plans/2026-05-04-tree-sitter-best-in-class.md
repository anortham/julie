# Tree-Sitter Best-In-Class Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Make Julie's tree-sitter extraction layer defensible as best-in-class by hardening the data path, proving extraction correctness for every supported language entry, and upgrading parser dependencies only behind whole-language golden gates.

**Architecture:** Treat extracted data as a product contract, not an implementation detail. Build a registry-backed capability matrix and golden fixture harness that covers every language entry, then use that gate to fix watcher parity, relationship precision, metadata drift, parse diagnostics, performance, and dependency upgrades.

**Tech Stack:** Rust, tree-sitter, `julie-extractors`, SQLite relationship storage, Tantivy projection, `cargo nextest`, Julie xtask test buckets.

---

## Non-Negotiables

- No language subset counts as complete. Every registry entry must have a row in the capability matrix and at least one golden extraction case before parser dependency upgrades are accepted.
- Variants count. `tsx`, `jsx`, and `vue` are not covered merely because `typescript`, `javascript`, and `html` pass.
- Missing capability must be explicit and tested. A row may say a capability is intentionally unsupported, but the harness must assert that status so it cannot become accidental silence.
- Golden tests must use `extract_canonical`, because that is the production extraction path. Direct extractor tests can remain useful, but they do not satisfy the parser-upgrade gate.
- Wrong graph edges are worse than missing graph edges. Ambiguous local relationship resolution should emit pending relationships or lower-confidence unresolved data instead of a confident edge to the wrong symbol.
- Dependency upgrades are whole-inventory decisions. Each parser crate is either updated, verified current, or explicitly held with a reason and evidence.

## Audit Inputs

- Core tree-sitter is currently `0.25.10`, declared in [Cargo.toml](/Users/murphy/source/julie/Cargo.toml:42) and [crates/julie-extractors/Cargo.toml](/Users/murphy/source/julie/crates/julie-extractors/Cargo.toml:17).
- Latest upstream core is `0.26.8`. `tree-sitter 0.25.10` and `0.26.8` both support parser ABI `13..15`, so the main risk is grammar/API drift, not ABI panic.
- Watcher incremental indexing calls `extract_all` in [src/watcher/handlers.rs](/Users/murphy/source/julie/src/watcher/handlers.rs:128), but does not resolve `pending_relationships` or `structured_pending_relationships`.
- Full indexing collects and resolves pending relationships in [src/tools/workspace/indexing/pipeline.rs](/Users/murphy/source/julie/src/tools/workspace/indexing/pipeline.rs:112).
- Relationship extractors use name-only maps in several places, including [crates/julie-extractors/src/csharp/relationships.rs](/Users/murphy/source/julie/crates/julie-extractors/src/csharp/relationships.rs:354), which can create wrong edges for overloads or duplicate names.
- The extractor suite is broad, but golden production-path coverage is thin: `crates/julie-extractors/src/tests` has many tests, while only a small number call `extract_canonical`.
- Real-world validation currently checks symbol non-emptiness in [src/tests/integration/real_world_validation.rs](/Users/murphy/source/julie/src/tests/integration/real_world_validation.rs:279), which is smoke coverage, not extraction-contract coverage.

## Language Accounting

The plan covers these registry entries:

| Capability group | Language entries |
| --- | --- |
| Full extraction | `rust`, `c`, `cpp`, `go`, `zig`, `typescript`, `tsx`, `javascript`, `jsx`, `python`, `java`, `csharp`, `vbnet`, `php`, `ruby`, `swift`, `kotlin`, `scala`, `dart`, `elixir`, `bash`, `powershell`, `gdscript` |
| Pending relationships, no type output | `lua`, `qml`, `r` |
| No pending relationships | `html`, `vue`, `razor`, `sql`, `regex` |
| Data-only extraction | `css`, `markdown`, `json`, `toml`, `yaml` |

`text` is not tree-sitter-backed, but watcher/index parity must still cover text-only fallback files because the initial index and watcher currently disagree about unsupported and extensionless files.

## File Structure

### New files

- `fixtures/extraction/README.md`: fixture format, acceptance rules, and how to regenerate expected output intentionally.
- `fixtures/extraction/capabilities.json`: one row per registry entry, including aliases, extensions, expected outputs, parser crate, and dependency status.
- `fixtures/extraction/<language>/<case>/source.<ext>`: source fixture for each language and variant.
- `fixtures/extraction/<language>/<case>/expected.json`: normalized expected `extract_canonical` output.
- `crates/julie-extractors/src/tests/golden.rs`: fixture runner that loads all `fixtures/extraction/**/expected.json` cases and compares normalized extraction output.
- `crates/julie-extractors/src/tests/capability_matrix.rs`: tests that every registry entry has capability data and at least one golden case.
- `src/tools/workspace/indexing/file_policy.rs`: shared indexing policy for size limits, minified/generated detection, and text-only fallback classification used by batch indexing and watcher paths.
- `src/tests/tools/workspace/file_policy.rs`: policy tests covering parser-backed, text-only, oversized, minified, extensionless, and blacklisted files.
- `docs/TREE_SITTER_UPGRADES.md`: dependency upgrade process, parser inventory, ABI facts, and golden-gate commands.

### Existing files to modify

- `crates/julie-extractors/src/tests/mod.rs`: register `golden` and `capability_matrix`.
- `crates/julie-extractors/src/registry.rs`: expose enough registry metadata for matrix validation, without creating a second source of truth.
- `crates/julie-extractors/src/language.rs`: move toward one language spec table for names, aliases, extensions, parsers, and error messages.
- `crates/julie-extractors/src/base/extractor.rs`: replace substring language checks in doc comment detection with table-driven exact matching.
- `crates/julie-extractors/src/base/creation_methods.rs`: add helpers for span-based IDs and scoped relationship lookup.
- `crates/julie-extractors/src/*/relationships.rs`: replace ambiguous name-only relationship resolution across all relationship-capable languages.
- `crates/julie-extractors/src/*/identifiers.rs`: remove repeated per-identifier symbol-list cloning across all affected languages.
- `crates/julie-extractors/src/*/mod.rs`: move repeated pending relationship storage into a shared helper or `BaseExtractor` owned store.
- `crates/julie-extractors/src/pipeline.rs`: add parse diagnostics and parser reuse where it is cleanly supported.
- `src/watcher/handlers.rs`: resolve pending relationships for watcher updates, move extraction work off the async runtime, and use shared file policy.
- `src/watcher/filtering.rs`: align watcher inclusion/deletion behavior with initial indexing, including text-only fallback.
- `src/tools/workspace/indexing/processor.rs`: use shared file policy instead of private duplicated size/minified logic.
- `src/tools/workspace/indexing/pipeline.rs`: expose or reuse pending-relationship resolution logic for watcher updates.
- `src/tools/editing/rewrite_symbol.rs`: treat parse-error trees conservatively before rewriting.
- `src/tools/refactoring/mod.rs`: treat parse-error trees conservatively before AST-aware rename.
- `xtask/test_tiers.toml`: add extractor and parser-upgrade buckets.
- `xtask/src/changed.rs`: map `crates/julie-extractors/**`, `fixtures/extraction/**`, `Cargo.lock`, and tree-sitter manifest edits to the new buckets.
- `xtask/tests/changed_tests.rs`: add routing tests for extractor and parser-upgrade paths.
- `AGENTS.md` and `CLAUDE.md`: update quick reference with the new extractor gate after the implementation is in place.

## Task 1: Golden Extraction Harness And Capability Matrix

**Files:**
- Create: `fixtures/extraction/README.md`
- Create: `fixtures/extraction/capabilities.json`
- Create: `crates/julie-extractors/src/tests/golden.rs`
- Create: `crates/julie-extractors/src/tests/capability_matrix.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs`
- Modify: `crates/julie-extractors/src/registry.rs`

**What to build:** Add a production-path golden fixture harness driven by `extract_canonical`. The harness compares normalized output for symbols, spans, parent links, signatures, doc comments, annotations, relationships, pending relationships, structured pending relationships, identifiers, and type info.

**Approach:** Normalize unstable fields before comparison only when they are genuinely derived noise. Do not normalize away names, kinds, spans, parent IDs, relationship direction, relationship kind, containing symbol IDs, or type values. The capability matrix must be generated or validated against `registry::supported_languages()` and `capabilities_for_language`, so a new language entry fails tests until it has fixture coverage.

**Acceptance criteria:**
- [ ] A test fails if any registry entry lacks a `capabilities.json` row.
- [ ] A test fails if any registry entry lacks at least one golden fixture case.
- [ ] A test fails on missing or unexpected symbols, identifiers, relationships, pending relationships, structured pending relationships, type info, doc comments, or annotations for a fixture.
- [ ] Golden comparison uses `extract_canonical`, not direct language-specific extractor calls.
- [ ] Worker-scope verification passes: `cargo nextest run -p julie-extractors golden` and `cargo nextest run -p julie-extractors capability_matrix`.

## Task 2: Seed Golden Fixtures For Every Language Entry

**Files:**
- Create: `fixtures/extraction/<language>/<case>/source.<ext>` for every row in the language accounting table.
- Create: `fixtures/extraction/<language>/<case>/expected.json` for every row in the language accounting table.
- Modify: `fixtures/extraction/capabilities.json`

**What to build:** Add the first complete golden fixture inventory. This is not a subset rollout. Every parser-backed registry entry gets at least one case in the first pass.

**Approach:** Keep each fixture small but meaningful. Full extraction languages must include at least one named definition, one nested or parented symbol when the language supports it, one identifier, one local relationship or structured pending relationship, and one type/asserted absence based on capability. Data-only languages must assert their intended symbols and identifiers, plus explicit absence of unsupported outputs. Variants need variant-specific syntax: `tsx` must include JSX, `jsx` must include JSX, and `vue` must include single-file component structure.

**Acceptance criteria:**
- [ ] All `Full extraction` rows have symbol, span, parent or explicit flat-structure assertion, identifier, relationship or structured pending relationship, and type expectation.
- [ ] `lua`, `qml`, and `r` have pending relationship coverage and explicit no-type expectation.
- [ ] `html`, `vue`, `razor`, `sql`, and `regex` have no-pending expectation and meaningful available outputs.
- [ ] `css`, `markdown`, `json`, `toml`, and `yaml` have data-only expectations and explicit absence of unsupported outputs.
- [ ] Fixture count equals or exceeds registry entry count.
- [ ] Worker-scope verification passes: `cargo nextest run -p julie-extractors golden` and `cargo nextest run -p julie-extractors capability_matrix`.

## Task 3: Xtask Buckets And Changed-Path Routing

**Files:**
- Modify: `xtask/test_tiers.toml`
- Modify: `xtask/src/changed.rs`
- Modify: `xtask/tests/changed_tests.rs`
- Modify: `xtask/tests/manifest_contract_tests.rs`
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`

**What to build:** Add calibrated gates for extractor work and parser-upgrade work, then wire changed-file selection to those gates.

**Approach:** Add an `extractors` bucket for normal extractor changes and a `parser-upgrade` bucket for dependency and golden-corpus changes. Map `crates/julie-extractors/**`, `fixtures/extraction/**`, `Cargo.toml`, `crates/julie-extractors/Cargo.toml`, and `Cargo.lock` tree-sitter changes to the right bucket. Keep broad manifest edits from accidentally falling through to generic `dev` unless they truly touch shared infrastructure.

**Acceptance criteria:**
- [ ] `cargo xtask test bucket extractors` runs the extractor golden and capability tests.
- [ ] `cargo xtask test bucket parser-upgrade` runs extractor golden tests plus real-world golden tests.
- [ ] `cargo xtask test changed` selects extractor buckets for `crates/julie-extractors/**`.
- [ ] `cargo xtask test changed` selects parser-upgrade bucket for `fixtures/extraction/**` and tree-sitter dependency edits.
- [ ] Worker-scope verification passes: `cargo nextest run -p xtask changed_tests`.

## Task 4: Watcher And Batch Indexing Parity

**Files:**
- Create: `src/tools/workspace/indexing/file_policy.rs`
- Create: `src/tests/tools/workspace/file_policy.rs`
- Modify: `src/tests/mod.rs`
- Modify: `src/tools/workspace/indexing/processor.rs`
- Modify: `src/watcher/handlers.rs`
- Modify: `src/watcher/filtering.rs`
- Modify: `src/tools/workspace/indexing/pipeline.rs`
- Modify: `src/tests/integration/watcher_handlers.rs`
- Modify: `src/tests/integration/watcher.rs`

**What to build:** Make watcher incremental indexing honor the same extraction contract as full indexing.

**Approach:** Move size/minified/text-only policy into shared code. Make watcher extraction run in `spawn_blocking` like batch indexing. Reuse pending relationship resolution logic after watcher updates, including structured pending relationships. Align watcher filtering with initial discovery so text-only and extensionless files do not become stale.

**Acceptance criteria:**
- [ ] A watcher update that introduces a cross-file call stores or resolves the same relationship data as batch indexing.
- [ ] Oversized and minified parser-backed files take the same text-only or repair path in watcher and batch indexing.
- [ ] Extensionless or unsupported text-only files are either maintained by both initial indexing and watcher paths, or rejected by both. The chosen behavior is encoded in tests.
- [ ] Watcher extraction does not run CPU-heavy tree-sitter parsing on the async runtime.
- [ ] Worker-scope verification passes: `cargo nextest run --lib tests::integration::watcher_handlers`.

## Task 5: Parse Diagnostics And Rewrite Safety

**Files:**
- Modify: `crates/julie-extractors/src/pipeline.rs`
- Modify: `crates/julie-extractors/src/base/types.rs`
- Modify: `crates/julie-extractors/src/tests/golden.rs`
- Modify: `src/tools/editing/rewrite_symbol.rs`
- Modify: `src/tools/refactoring/mod.rs`
- Modify: `src/tests/tools/editing/mod.rs`
- Modify: `src/tests/tools/refactoring/mod.rs`

**What to build:** Record parse-error status without rejecting every recovered tree, and prevent rewrite/refactor operations from applying AST edits against unsafe parse trees.

**Approach:** Add parse diagnostics to extraction results or file metadata in a way that does not break existing extraction recovery. Parser-backed indexing can still store recovered data, but editing/refactoring tools must fail clearly when the live tree has parse errors near the target span.

**Acceptance criteria:**
- [ ] Golden fixtures can assert parse-error status for malformed recovery cases.
- [ ] Indexing records parse-error metadata without dropping useful recovered symbols.
- [ ] `rewrite_symbol` refuses edits when parse errors overlap or contain the target symbol span.
- [ ] `rename_symbol` refuses AST-aware edits on unsafe parse-error trees and reports a clear failure kind.
- [ ] Worker-scope verification passes: `cargo nextest run --lib tests::tools::editing::` and `cargo nextest run --lib tests::tools::refactoring::`.

## Task 6: Relationship Precision Across All Relationship-Capable Languages

**Files:**
- Modify: `crates/julie-extractors/src/base/creation_methods.rs`
- Modify: `crates/julie-extractors/src/base/relationship_resolution.rs`
- Modify: `crates/julie-extractors/src/*/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/relationship_precision.rs`
- Modify: `fixtures/extraction/**/expected.json`

**What to build:** Replace name-only local relationship resolution with scope-aware resolution across every relationship-capable language entry.

**Approach:** Add base helpers that resolve callers by containing span. Resolve callees by a scoped candidate set: name, kind, parent/scope, receiver context when available, and file path. If more than one candidate remains, emit a structured pending relationship or lower-confidence unresolved output instead of a confident wrong edge. Apply the rule across the full extraction, pending-no-types, and no-pending relationship-capable groups, not only C#, Python, and TypeScript.

**Acceptance criteria:**
- [ ] Duplicate method/function names in different classes/scopes do not produce wrong local edges.
- [ ] Overloads do not collapse to a single arbitrary target.
- [ ] Ambiguous calls are represented as unresolved or pending data with clear metadata.
- [ ] Golden fixtures include at least one duplicate-name case for every relationship-capable language where the grammar can express it.
- [ ] Worker-scope verification passes: `cargo nextest run -p julie-extractors relationship_precision` and `cargo nextest run -p julie-extractors golden`.

## Task 7: Single Language Spec And Shared Pending Store

**Files:**
- Modify: `crates/julie-extractors/src/language.rs`
- Modify: `crates/julie-extractors/src/registry.rs`
- Modify: `crates/julie-extractors/src/base/extractor.rs`
- Modify: `crates/julie-extractors/src/*/mod.rs`
- Modify: `crates/julie-extractors/src/tests/api_surface.rs`
- Modify: `crates/julie-extractors/src/tests/capability_matrix.rs`

**What to build:** Replace duplicated language metadata and repeated pending relationship storage with shared structures.

**Approach:** Introduce a `LanguageSpec` table that owns canonical language name, aliases, extensions, parser function, extractor registry entry, capability flags, doc comment policy, and display error text. Move `pending_relationships` and `structured_pending_relationships` storage into `BaseExtractor` or a small shared store so language modules do not copy the same vectors and accessors.

**Acceptance criteria:**
- [ ] `supported_languages`, `supported_extensions`, parser lookup, registry lookup, and error messages derive from one language spec source.
- [ ] `jsx`, `tsx`, `vue`, and `vbnet` are consistently represented in language support output and errors.
- [ ] JavaScript doc comments are no longer classified as Java because of substring matching.
- [ ] Pending relationship storage behavior is identical before and after the refactor, proven by golden tests.
- [ ] Worker-scope verification passes: `cargo nextest run -p julie-extractors api_surface`, `cargo nextest run -p julie-extractors capability_matrix`, and `cargo nextest run -p julie-extractors golden`.

## Task 8: Identifier Extraction Allocation Cleanup

**Files:**
- Modify: `crates/julie-extractors/src/base/creation_methods.rs`
- Modify: `crates/julie-extractors/src/*/identifiers.rs`
- Modify: `crates/julie-extractors/src/tests/identifier_semantics.rs`
- Modify: `fixtures/extraction/**/expected.json`

**What to build:** Remove repeated same-file symbol list cloning in identifier walkers across all affected languages.

**Approach:** Build the same-file symbol view once per extraction pass and pass borrowed references through walkers. Keep behavior identical, then let the golden harness prove no containing-symbol IDs drift.

**Acceptance criteria:**
- [ ] Identifier extraction does not rebuild cloned symbol vectors per identifier.
- [ ] Containing symbol IDs remain stable across every golden fixture.
- [ ] No new helper assumes Rust-style `src/` or language-specific project layout.
- [ ] Worker-scope verification passes: `cargo nextest run -p julie-extractors identifier_semantics` and `cargo nextest run -p julie-extractors golden`.

## Task 9: Parser Reuse And JSONL Efficiency

**Files:**
- Modify: `crates/julie-extractors/src/pipeline.rs`
- Modify: `crates/julie-extractors/src/tests/jsonl_pipeline.rs`
- Modify: `crates/julie-extractors/src/tests/jsonl_invariants.rs`

**What to build:** Avoid avoidable parser setup churn while keeping parser ownership simple and thread-safe.

**Approach:** Reuse a JSON parser inside JSONL extraction instead of creating a parser per non-empty line. Evaluate per-language parser reuse inside a blocking worker only if it does not complicate concurrency or lifetime ownership. The JSONL reuse is the required implementation; broader parser pools need measured benefit before adding machinery.

**Acceptance criteria:**
- [ ] JSONL extraction creates and configures one parser per JSONL file.
- [ ] JSONL record offsets and IDs remain correct.
- [ ] No parser instance crosses thread boundaries unsafely.
- [ ] Worker-scope verification passes: `cargo nextest run -p julie-extractors jsonl_pipeline` and `cargo nextest run -p julie-extractors jsonl_invariants`.

## Task 10: Whole-Inventory Tree-Sitter Dependency Upgrade

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/julie-extractors/Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `fixtures/extraction/capabilities.json`
- Modify: `docs/TREE_SITTER_UPGRADES.md`
- Modify: parser-specific extractor files as required by changed grammar node names or fields.

**What to build:** Upgrade tree-sitter core and parser crates behind the golden gate, with every parser crate accounted for.

**Approach:** Use two passes. First, produce a dependency ledger for every parser crate: current version, latest version, ABI, parser source, update decision, and evidence. Second, update all crates whose latest compatible release is accepted by the ledger. Crates with no newer release are marked current. Crates held back must include a concrete blocker, such as breaking grammar field changes requiring extractor updates already captured by failing golden tests.

**Acceptance criteria:**
- [ ] Every tree-sitter parser dependency has a ledger row in `fixtures/extraction/capabilities.json` or `docs/TREE_SITTER_UPGRADES.md`.
- [ ] Core `tree-sitter 0.26.8` is trialed with the full golden gate.
- [ ] Parser crates requiring core `0.26` are upgraded in the same parser-upgrade branch when their golden cases pass.
- [ ] Parser crates with breaking grammar changes are not skipped; their extractor fixes are included before the dependency task is accepted.
- [ ] Worker-scope verification passes for narrow failing golden tests during each parser fix.
- [ ] Lead parser-upgrade verification passes: `cargo xtask test bucket parser-upgrade`.

## Task 11: Promote Real-World Validation From Smoke To Golden

**Files:**
- Modify: `src/tests/integration/real_world_validation.rs`
- Create or modify: `fixtures/extraction/real-world/**`
- Modify: `xtask/test_tiers.toml`

**What to build:** Turn representative real-world fixtures into expected-output regression tests for parser upgrades.

**Approach:** Keep small per-language golden fixtures fast and mandatory. Add larger real-world cases to the parser-upgrade bucket, with expected data focused on stable high-value outputs: key symbols, parent links, representative relationships, identifiers, and type/doc-comment outputs where supported.

**Acceptance criteria:**
- [ ] Real-world validation fails on missing expected symbols and representative graph outputs, not only empty extraction.
- [ ] Real-world cases run in parser-upgrade scope, not every tiny worker loop.
- [ ] Every supported language with an existing real-world fixture has expected-output assertions.
- [ ] Worker-scope verification passes for the modified real-world test module.

## Task 12: Final Documentation And Dogfood Proof

**Files:**
- Modify: `docs/TREE_SITTER_UPGRADES.md`
- Modify: `docs/ADDING_NEW_LANGUAGES.md`
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`
- Save: `.memories/` checkpoint files created during execution.

**What to build:** Document the new extraction contract and make future language additions inherit it automatically.

**Approach:** Update docs so adding a language means adding a `LanguageSpec`, parser dependency ledger entry, capability matrix row, golden fixture, and xtask routing. Include the exact parser-upgrade process and verification ledger requirements.

**Acceptance criteria:**
- [ ] New language docs require golden fixture coverage before declaring support.
- [ ] Agent instructions point parser/extractor changes at the extractor and parser-upgrade gates.
- [ ] Goldfish checkpoints are included when created.
- [ ] Lead dogfood check succeeds with Julie tools against this repo after reindexing.

## Verification Strategy

**Project source of truth:** [AGENTS.md](/Users/murphy/source/julie/AGENTS.md), [RAZORBACK.md](/Users/murphy/source/julie/RAZORBACK.md), [docs/TESTING_GUIDE.md](/Users/murphy/source/julie/docs/TESTING_GUIDE.md), and [xtask/test_tiers.toml](/Users/murphy/source/julie/xtask/test_tiers.toml).

**Worker red/green scope:** Workers run exact tests they add or change, usually `cargo nextest run -p julie-extractors <test_name>` for extractor work, or `cargo nextest run --lib <exact_test_name>` for main crate watcher/tooling work.

**Worker ceiling:** Workers may run only their assigned exact test filters. Workers do not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test system`, or parser-upgrade buckets unless the lead explicitly assigns a diagnostic run and still owns acceptance.

**Worker gate invariant:** Each worker gate proves one behavior: fixture harness comparison, capability matrix completeness, watcher parity, parse-error rewrite safety, relationship precision, language metadata consistency, identifier stability, JSONL offset stability, or a parser-specific grammar adaptation.

**Lead affected-change scope:** After each coherent batch, run `cargo xtask test changed`. If changed-file routing selects only the new extractor bucket, run that bucket. If routing falls back to `dev`, accept the fallback and record why.

**Branch gate:** Run `cargo xtask test dev` once after the complete batch. Add `cargo xtask test system` for watcher/indexing lifecycle changes. Add `cargo xtask test dogfood` after search/ranking behavior is affected by graph output changes.

**Parser-upgrade gate:** Run `cargo xtask test bucket parser-upgrade` for any core tree-sitter or parser crate version change, any grammar-driven extractor adaptation, or any fixture expected-output update caused by dependency changes.

**Replay/metric evidence:** Golden fixture comparisons are hard gates. Real-world fixture expected outputs are hard gates. Parser dependency ledger rows are review evidence, not pass/fail by themselves. Performance timings for parser reuse are report-only unless a regression exceeds an agreed threshold in a test.

**Escalation triggers:** Escalate to strategy or gate-review tier for ambiguous relationship semantics, parser grammar changes that alter node shapes across many extractors, watcher/indexing repair semantics, dependency resolution conflicts, repeated golden fixture failures, or any change that makes the language matrix disagree with registry metadata.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. For parser-upgrade work, also record core version, parser crate versions, ABI range, and fixture corpus revision.

## Model Routing

**Project source of truth:** [RAZORBACK.md](/Users/murphy/source/julie/RAZORBACK.md).

**Strategy tier:** Planning, architecture, decomposition, lead review, dependency version selection, parser-upgrade decisions, and finding triage.
- Harness mapping: Codex `gpt-5.5` with medium or high reasoning.

**Implementation tier:** Bounded worker tasks with clear file ownership, such as golden harness, xtask routing, fixture seeding for assigned language groups, watcher parity tests, and isolated extractor fixes.
- Harness mapping: Codex `gpt-5.4-mini` with xhigh reasoning when the work is local and the invariant is clear.

**Coupled implementation tier:** Shared-invariant edits to watcher/indexing, language spec tables, relationship resolution helpers, parser upgrade fixes, and public extraction contracts.
- Harness mapping: Codex `gpt-5.3-codex` high by default, xhigh for terminal-heavy dependency or parser failures.

**Mechanical tier:** Fixture data entry, docs updates, manifest rows, and rote expected-output regeneration after a reviewer has approved the semantic change.
- Harness mapping: Codex `gpt-5.4-mini` low or medium.

**Gate-interpretation reviewer:** Review of golden fixture diffs, parser-upgrade failures, relationship precision semantics, or expected-output changes.
- Harness mapping: Codex `gpt-5.3-codex` high.

**Escalation tier:** Subtle correctness, weak tests, high blast radius, repeated failures, parser grammar incompatibilities, or cases where the plan no longer matches code reality.
- Harness mapping: Codex `gpt-5.5` high or xhigh.

**Worker eligibility:** Workers can own a task only when file ownership is narrow, acceptance criteria are explicit, verification is exact, and no hidden shared invariant is being interpreted. Fixture seeding can be parallelized by language group only after the harness and capability matrix contract are implemented.

**Escalation triggers:** Any worker that finds a parser grammar shape mismatch affecting multiple languages, a false positive relationship edge, a watcher repair-state contract issue, or a failing parser-upgrade gate must stop and report.

**Mechanical exclusion:** Mechanical workers cannot decide whether changed expected output is correct. They may update fixture JSON only from reviewed extraction output and must state the reviewer-approved semantic reason.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note that in the worker report, and continue.

## Execution Notes

- Start with Task 1 and Task 2. Without the whole-language golden gate, parser upgrades and relationship refactors are flying by vibes.
- Task 4 can proceed after Task 1 because watcher parity is a current correctness bug, but it must still add golden or integration coverage for any extractor data contract it touches.
- Task 10 must not start until every language entry has at least one golden fixture and the parser-upgrade bucket exists.
- Parallel fixture work is allowed, but the split is by ownership of fixture files, not by declaring only some languages in scope.

## Plan Self-Review

- Placeholder scan: no placeholder sections remain.
- Scope check: this is a master implementation plan for one theme, tree-sitter extraction quality. Tasks are separable and each produces testable behavior.
- Ambiguity check: the language coverage rule is explicit. Every registry entry must be accounted for before parser upgrades are accepted.
- Risk check: the highest-risk changes are watcher parity, relationship resolution, language metadata unification, and dependency upgrades. Those tasks have stricter gates and escalation triggers.
