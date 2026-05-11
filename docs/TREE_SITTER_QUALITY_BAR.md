# Tree-Sitter Quality Bar

Updated: 2026-05-11

This document defines the fixed best-in-class target for Julie's tree-sitter layer. It is not allowed to move down to match the current implementation. Passing the current extractor gates proves the current contract; it does not, by itself, prove this target.

Implementation plans and evidence ledgers may link here, but they do not get to redefine the bar. If Julie cannot meet a requirement yet, that requirement stays here and the implementation is marked incomplete.

## Current Verdict

Status: **closure work landed; release-profile evidence regenerated; daemon-mode live dogfood passed; PR open; merge pending**.

The 2026-05-10 autonomous run and Codex follow-up drove the best-in-class plan (`docs/plans/2026-05-10-best-in-class-tree-sitter-plan.md`) through release-gate and daemon-mode dogfood evidence against this rubric:

- Phase 4a-d: every relationship-emitting language ships a `cross_file` fixture and locking test. Cross-file pending now emits StructuredPendingRelationship with `target.terminal_name` + `import_context` for 24 languages (Rust, C, C++, Go, Zig, TypeScript, JavaScript, TSX, JSX, Python, Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Scala, Dart, Elixir, Lua, R, GDScript, SQL, Vue, HTML, QML). Recipe-B no-pending classifications (CSS, regex, Markdown, YAML, Razor, TOML, JSON) carry locking tests as evidence.
- Phase 5: Pillar-3 hardening landed. `capability_snapshot()` + `EXTRACTION_CONTRACT_VERSION` exported, `SEMANTIC_INDEX_ENGINE_VERSION` embeds the contract version and is checked by regression tests, downstream-consumer integration test spawns a tempdir consumer crate and runs the public API end-to-end. Extractors bucket now runs the downstream-smoke gate.
- Phase 6: release-profile real-world evidence now covers 22 repos, including the VB.NET `samples` corpus, with 110 representative specs enforced by the hard-failure gate.
- Phase 7: historical verification docs removed; canonical sources of truth are `fixtures/extraction/capabilities.json` (typed evidence schema, machine-checked) and the regenerated `docs/LANGUAGE_CERTIFICATION_REPORT.md` + `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json`.
- Phase 5.4 follow-up: `cargo doc -p julie-extractors --no-deps` is warning-free after cleaning broken doc links and HTML-like text in public rustdoc.

Live daemon-mode dogfood passed after a release rebuild and restart; see the verification ledger. The Codex-hosted `mcp__julie__` connector in this session still returned `Transport closed`, so the live rows were verified through `julie-server tool ...` against the daemon HTTP transport and the connector issue is tracked as a separate transport concern. PR #20 is open; merge back to `main` is still pending.

## Current Documentation Validation

The restored verification docs were historical evidence, not current certification output. They were removed in Phase 7.

2026-05-11 validation facts:

- `fixtures/extraction/capabilities.json` tracks 36 registry rows: 34 user-facing language rows plus `tsx` and `jsx`.
- `fixtures/extraction/capabilities.json` records 17 capability-gap rows, all `status: "exception"`; there are no open capability gaps.
- `fixtures/extraction/capabilities.json` now records fixture-backed per-kind coverage for symbols, relationships, identifiers, and body spans. The generated certification report carries the current depth totals.
- `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` records release-profile evidence for 22 repos and 0 skipped repos.
- `fixtures/extraction/tree-sitter-real-world-corpus.toml` includes 110 representative specs, 5 per release-profile repo.
- `cargo xtask certify tree-sitter --check` verifies [LANGUAGE_CERTIFICATION_REPORT.md](LANGUAGE_CERTIFICATION_REPORT.md) is current for the checked-in capability, fixture, historical-doc, and real-world evidence state.

Release claims must come from generated evidence plus verification ledger rows tied to the checked commit, not from manually edited historical docs. Generated docs do not embed a self-referential commit hash.

## Fixed Target

Julie can claim best-in-class tree-sitter support only when extraction output is a truthful, tested product contract for every supported language entry and every language gets the semantics that are native to that language.

The target is not "every language has identical output." That would be silly. The target is:

1. Every supported tree-sitter language has a target capability row that says what Julie should extract when the implementation is complete.
2. Every supported tree-sitter language has an implementation capability row that says what Julie extracts today.
3. A mismatch between target and implementation is an open gap unless the missing capability is intrinsically not meaningful for that language, or blocked by a documented parser limitation.
4. "We have not implemented it yet" is never an exception.
5. Wrong graph edges are release blockers. Missing edges are gaps, but wrong confident edges poison every downstream tool.
6. Golden fixtures use `extract_canonical`, the production extraction path.
7. Parser dependency upgrades are accepted only behind whole-inventory golden and real-world gates.
8. Startup, watcher, refresh, force reindex, and semantic repair paths produce the same extraction contract.
9. Live dogfood proves the release binary, daemon restart, health status, search, refs, and call-path behavior on Julie itself.

## Language Target Model

The target is grouped by language semantics, not by current implementation convenience.

| Target group | Language entries | Required target |
| --- | --- | --- |
| General programming languages | `rust`, `c`, `cpp`, `go`, `zig`, `typescript`, `tsx`, `javascript`, `jsx`, `python`, `java`, `csharp`, `vbnet`, `php`, `ruby`, `swift`, `kotlin`, `scala`, `dart`, `elixir`, `bash`, `powershell`, `gdscript`, `lua`, `r` | Symbols, parent structure, identifiers, local relationships, structured pending relationships for cross-file or unresolved references, doc comments where syntax supports them, type output where syntax or stable inference supports it. |
| Component and template languages | `vue`, `razor`, `qml`, `html` | Structural symbols plus relationships for component use, template references, event handlers, links, resources, bindings, embedded code calls, and structured pending output when the target is outside the file. |
| Query and declarative languages | `sql`, `css`, `regex` | Domain symbols plus domain references: SQL table/view/CTE/function dependencies, CSS custom property/keyframe/selector relationships where syntactically provable, regex capture group and backreference relationships. |
| Documentation and data languages | `markdown`, `json`, `toml`, `yaml` | Stable document or data structure symbols, meaningful identifiers or keys, links/anchors or aliases where the language has references, explicit absence of call/type outputs when they are not meaningful. |

Variants count separately. `tsx`, `jsx`, and `vue` are not covered merely because `typescript`, `javascript`, and `html` pass.

`text` is not a tree-sitter language entry. It still matters for indexing policy parity, but it does not count toward parser-backed target coverage.

## Capability Contract

The fixture capability file must stop acting as the target. It should distinguish:

- **Target capability:** what best-in-class requires for the language.
- **Implemented capability:** what the extractor currently emits.
- **Gap status:** open, met, or exception.
- **Exception reason:** only intrinsic non-applicability or documented parser limitation.
- **Evidence:** golden fixture paths and tests that prove the implemented behavior.
- **Kind coverage:** per-kind `supported`, `not_applicable`, and `open_gaps` entries for current symbol, relationship, identifier, and body-span/body-hash domains.

A target capability may be false only when the concept does not apply to the language. Examples:

- Function-call relationships are false for JSON because JSON has no functions.
- Type output is false for Markdown because Markdown has no language-level type system.
- Cross-file pending calls are false for standalone regex patterns unless the extractor adds host-language integration.

A target capability must not be false because the implementation is currently thin. Examples:

- Vue relationship target is not false just because the Vue extractor currently emits no graph output.
- Regex relationship target is not false just because relationship extraction is currently stubbed.
- CSS relationship target is not false if the fixture includes `var(--name)` or `animation: keyframes` references that can be linked to local definitions.

## Golden Fixture Contract

Golden fixtures must compare normalized `extract_canonical` output. Normalization may remove derived noise, but it must not hide names, kinds, spans, parent IDs, relationship direction, relationship kind, containing symbol IDs, type values, identifiers, parse diagnostics, signatures, or doc comments.

Required coverage:

- A registry entry without a target capability row fails the matrix test.
- A registry entry without an implementation capability row fails the matrix test.
- A registry entry without at least one golden fixture fails the matrix test.
- A target capability marked implemented must have at least one golden fixture proving it.
- A supported per-kind claim must appear in golden fixture output.
- A supported body-span claim must appear in golden fixture output with both `body_span` and `body_hash`.
- A target capability marked open must have at least one failing or pending plan item tied to it. It cannot disappear into prose.
- A full programming-language fixture includes at least one named definition, one parented or explicit flat-structure assertion, one identifier, one graph output or structured pending output, and one type assertion or explicit no-type target reason.
- A component/template fixture includes component or element structure, embedded code or binding syntax when supported, graph output for local references, and structured pending output for external references.
- A query/declarative fixture includes at least one domain definition and one domain reference when the language has references.
- A documentation/data fixture includes stable structure, identifiers or keys, and link/anchor/alias references when the language has them.

## Relationship Precision Contract

Relationship extraction must prefer no edge over a wrong edge.

Required behavior:

- Duplicate local names must not survive in legacy unique lookup maps.
- Unqualified calls resolve only when there is one credible local target or one concrete definition among declarations.
- Receiver-qualified calls to non-self receivers stay pending or receiver-qualified unless language-specific code can prove the target.
- `self`, `this`, and language-specific same-instance receivers may resolve to same-parent methods.
- `super` must not use same-parent resolution. It needs explicit inheritance-aware resolution, or it must stay pending.
- Cross-file or import-qualified calls retain structured pending context so later resolvers can distinguish duplicate terminal names.
- Domain-specific references, such as SQL table use, CSS variable use, Markdown links, YAML aliases, and regex backreferences, follow the same rule: unresolved is acceptable, wrong is not.

## Real-World Contract

Real-world fixtures are not a substitute for small golden fixtures. They are parser-upgrade regression cases for stable high-value outputs that appear in realistic files.

Required coverage:

- Every supported language with an existing real-world fixture has expected-output assertions.
- The parser-upgrade bucket fails on missing expected symbols, identifiers, parent links, representative graph outputs, and type or doc-comment outputs where those are stable for the language.
- Real-world fixtures prefer stable facts over exhaustive snapshots. They should catch parser drift without making unrelated fixture formatting expensive.

## Semantic Repair Contract

The semantic index engine version is part of the indexed-data contract. When it changes:

- Startup repair, explicit index, and refresh detect stale stamps.
- A stale stamp repairs symbols, identifiers, relationships, type rows, search projection, and semantic vectors when embeddings are available.
- Any semantic-version repair that performs full persistence uses force-equivalent cancellation and watcher pause behavior.
- A non-force refresh after successful repair reports current state and does not loop a full reindex.

## Parser Upgrade Contract

Parser upgrades must follow [TREE_SITTER_UPGRADES.md](TREE_SITTER_UPGRADES.md).

Required evidence:

- Core tree-sitter version.
- Parser crate versions or git revisions.
- ABI support range.
- Fixture corpus revision.
- Parser-upgrade bucket result at the exact commit being released.

## Release Gates

A release can claim this quality bar only when there are no open target gaps and these commands pass at the exact release commit:

| Gate | Command | Required when |
| --- | --- | --- |
| Formatter | `cargo fmt --check` | Always |
| Tree-sitter certification report | `cargo xtask certify tree-sitter --check` | Always for tree-sitter claims |
| Tree-sitter real-world evidence | `cargo xtask certify tree-sitter --real-world --profile release --out docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` | Before updating checked-in release-profile real-world evidence |
| Extractor bucket | `cargo xtask test bucket extractors` | Always |
| Parser-upgrade bucket | `cargo xtask test bucket parser-upgrade` | Always for parser, fixture, or extractor contract changes |
| Changed tier | `cargo xtask test changed` | Always after localized implementation changes |
| Dev tier | `cargo xtask test dev` | Always before release handoff |
| System tier | `cargo xtask test system` | Startup, watcher, workspace, daemon, or repair changes |
| Dogfood tier | `cargo xtask test dogfood` | Graph, search, refs, ranking, or navigation changes |
| Full tier | `cargo xtask test full` | Final release candidate |
| Release build | `cargo build --release` | Final release candidate |

Live dogfood must also pass after a release rebuild and MCP restart:

- `manage_workspace health` reports ready status for Julie.
- `call_path extract_symbols_static extract_canonical` finds the production extraction edge.
- `fast_refs` finds newly indexed semantic-version references after repair.
- SQLite records the current schema and semantic index engine version.
- Non-force refresh reports current state without repeating full reindex.

## Current Open Gaps

These are known gaps against the fixed target. This list is allowed to grow as the target capability split exposes more missing work.

| Gap | Status | Closing reference |
| --- | --- | --- |
| Cross-file pending shape for every relationship-emitting language | Closed 2026-05-10 | Per-language `cross_file` fixtures + `tests::<lang>::cross_file_pending` locking tests across 24 languages (commits 740af24 → 24564d0f). |
| Recipe-B no-pending classifications | Closed 2026-05-10 (exception) | Locking tests for CSS, regex, Markdown, YAML, Razor wired into `capability_gaps.evidence` as `kind: test`. TOML/JSON exception rows reference domain commits. |
| Pillar-3 downstream-consumer usability | Closed 2026-05-10 | `crates/julie-extractors/tests/downstream_smoke.rs::julie_extractors_works_as_path_dependency_in_downstream_crate` proves the crate consumable via a path dependency. |
| Capability snapshot public API + extraction-contract version | Closed 2026-05-10 | `julie_extractors::capability_snapshot()` + `julie_extractors::EXTRACTION_CONTRACT_VERSION`; `SEMANTIC_INDEX_ENGINE_VERSION` embeds the contract version and regression tests enforce the link. |
| `capability_matrix_negative_cases_emit_no_wrong_edges` activated | Closed 2026-05-10 | De-ignored in `crates/julie-extractors/src/tests/capability_matrix.rs`; accepted fixtures broadened to `negative|cross_file`. |
| Full-corpus real-world evidence with raised `min_relationships` | Closed 2026-05-11 | `cargo xtask certify tree-sitter --real-world --profile release --out docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` wrote 22 verified + 0 skipped repos. `samples` supplies VB.NET evidence. `tree-sitter-real-world-corpus.toml` now has 110 representative specs. Relationship floors are 5x language-file count except `blazor-samples` and `riverpod`, which use high actual-output floors because 5x exceeds truthful current relationship output. |
| Doc-comment audit on every public item in `julie-extractors` | Closed 2026-05-11 | `cargo doc -p julie-extractors --no-deps` emits no warnings after fixing broken intra-doc links, HTML-like generic text, and the ambiguous `capability_snapshot()` rustdoc link. |
| Fixed-target release evidence for integration branch | Closed 2026-05-11 | Phase 8.1 release gates recorded against `94b7f5a3`; broad regression tiers (dev/system/dogfood/full) recorded against `61a27e42` after the workspace_isolation_smoke matcher fix; closure and certification evidence recorded against `235bd37c` / `0e8f1357`; current health and daemon-mode live dogfood recorded against `88998e69`. |

## Verification Ledger

Record release evidence with the template in [verification-ledger-template.md](plans/verification-ledger-template.md). Evidence may be reused only when the scope label and commit SHA match the current HEAD exactly, and only when there are no open target gaps for the claimed release scope.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
| --- | --- | --- | --- | --- | --- | --- |
| Formatter | `cargo fmt --check` | formatter | `0b7a2f36` | Passed | 2026-05-05T16:35:22Z | No |
| Diff whitespace | `git diff --check` | diff-hygiene | `0b7a2f36` | Passed | 2026-05-05T16:35:22Z | No |
| Extractor golden and current capability matrix | `cargo xtask test bucket extractors` | extractors-current-contract | `0b7a2f36` | Passed 1 bucket in 1.1s | 2026-05-05T16:35:22Z | No |
| Parser upgrade current contract | `cargo xtask test bucket parser-upgrade` | parser-upgrade-current-contract | `0b7a2f36` | Passed 1 bucket in 1.7s | 2026-05-05T16:35:22Z | No |
| Changed-code regression tier | `cargo xtask test changed` | changed-current-contract | `0b7a2f36` | Passed 22 buckets in 537.5s | 2026-05-05T16:35:22Z | No |
| Startup, workspace, daemon, integration tier | `cargo xtask test system` | system-current-contract | `0b7a2f36` | Passed 6 buckets in 142.7s | 2026-05-05T16:35:22Z | No |
| Search and dogfood tier | `cargo xtask test dogfood` | dogfood-current-contract | `0b7a2f36` | Passed 2 buckets in 206.7s | 2026-05-05T16:35:22Z | No |
| Full release-candidate tier for current contract | `cargo xtask test full` | full-current-contract | `0b7a2f36` | Passed 30 buckets in 868.9s | 2026-05-05T16:35:22Z | No |
| Release binary build | `cargo build --release` | release-build-current-contract | `0b7a2f36` | Passed in 2m 51s | 2026-05-05T16:35:22Z | No |
| Live MCP health after rebuild and restart | `manage_workspace health detailed=true` | live-health-current-contract | `0b7a2f36` | READY: daemon serving, projection current 3970/3970, 34252 symbols, 32945 relationships, 7009 vectors | 2026-05-05T16:42:28Z | No |
| Live production call graph | `call_path extract_symbols_static extract_canonical` | live-call-path-current-contract | `0b7a2f36` | Found one-hop production call edge through `src/tools/workspace/indexing/extractor.rs:24` to `crates/julie-extractors/src/pipeline.rs:8` | 2026-05-05T16:42:28Z | No |
| Live references for extraction API | `fast_refs extract_canonical` | live-fast-refs-current-contract | `0b7a2f36` | Found definition plus 20 visible references, including public API projection and real-world contract callers | 2026-05-05T16:42:28Z | No |
| Live semantic state in SQLite | `sqlite3 ~/.julie/indexes/julie_528d4264/db/symbols.db` | live-sqlite-state-current-contract | `0b7a2f36` | Schema version 24, semantic engine `2026-05-05.reference-identifier-v3`, Tantivy projection ready at 3970/3970, 7009 vector rowids | 2026-05-05T16:42:28Z | No |
| Live non-force refresh | `manage_workspace refresh workspace_id=julie_528d4264` | live-refresh-current-contract | `0b7a2f36` | Already up-to-date at canonical revision 3970; no repeated full reindex | 2026-05-05T16:42:28Z | No |
| Formatter | `cargo fmt --check` | formatter | `94b7f5a3` | Passed | 2026-05-10T23:16:49Z | No |
| Diff whitespace | `git diff --check` | diff-hygiene | `94b7f5a3` | Passed | 2026-05-10T23:16:49Z | No |
| Tree-sitter certification freshness | `cargo xtask certify tree-sitter --check` | tree-sitter-cert | `94b7f5a3` | Passed (report current) | 2026-05-10T23:16:49Z | No |
| Extractor bucket (golden + capability_matrix + cert + downstream-smoke) | `cargo xtask test bucket extractors` | extractors-current-contract | `94b7f5a3` | Passed 4 commands in 27.1s | 2026-05-10T23:16:49Z | No |
| Parser upgrade current contract | `cargo xtask test bucket parser-upgrade` | parser-upgrade-current-contract | `94b7f5a3` | Passed 2 commands in 1.6s | 2026-05-10T23:16:49Z | No |
| Changed-code regression tier | `cargo xtask test changed` | changed-current-contract | `94b7f5a3` | No code/test buckets matched (clean working tree) | 2026-05-10T23:16:49Z | No |
| Release binary build | `cargo build --release` | release-build-current-contract | `94b7f5a3` | Passed in 3m 10s | 2026-05-10T23:16:49Z | No |
| Example consumer build | `cargo build --examples -p julie-extractors` | example-build-current-contract | `94b7f5a3` | Passed | 2026-05-10T23:16:49Z | No |
| Crate doctests | `cargo test -p julie-extractors --doc` | doctest-current-contract | `94b7f5a3` | Passed 1 test | 2026-05-10T23:16:49Z | No |
| Crate rustdoc | `cargo doc -p julie-extractors --no-deps` | rustdoc-current-contract | `94b7f5a3` | Generated (6 missing-docs warnings, expected — Phase 5.4 follow-up) | 2026-05-10T23:16:49Z | No |
| Pillar-3 downstream-consumer gate | `cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate` | downstream-smoke-current-contract | `94b7f5a3` | Passed in 17.0s | 2026-05-10T23:16:49Z | No |
| Dev regression tier | `cargo xtask test dev` | live-dev-current-contract | `61a27e42` | Passed 32 buckets in 354.1s | 2026-05-11T00:12:55Z | No |
| System regression tier | `cargo xtask test system` | live-system-current-contract | `61a27e42` | Passed 6 buckets in 86.5s (after restoring workspace_isolation_smoke matcher to the new line-grouped fast_search output) | 2026-05-11T00:12:55Z | No |
| Dogfood regression tier | `cargo xtask test dogfood` | live-dogfood-current-contract | `61a27e42` | Passed 2 buckets in 225.3s (tools-dogfood-repo-index + search-quality) | 2026-05-11T00:12:55Z | No |
| Full regression tier | `cargo xtask test full` | live-full-current-contract | `61a27e42` | Passed 40 buckets in 664.4s | 2026-05-11T00:12:55Z | No |
| Release-profile real-world evidence | `cargo xtask certify tree-sitter --real-world --profile release --out docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` | realworld-release-current-contract | `235bd37c` | Passed: 22 verified, 0 skipped, 0 hard failures; VB.NET `samples` included; 110 representative specs enforced | 2026-05-11T01:35:52Z | No |
| Tree-sitter certification freshness | `cargo xtask certify tree-sitter --check` | tree-sitter-cert-current-contract | `235bd37c` | Passed (report current) | 2026-05-11T01:35:52Z | No |
| Crate rustdoc | `cargo doc -p julie-extractors --no-deps` | rustdoc-current-contract | `235bd37c` | Passed with no warnings | 2026-05-11T01:35:52Z | No |
| Changed-code regression tier | `cargo xtask test changed` | changed-current-contract | `235bd37c` | Passed 4 buckets in 92.2s (`xtask-runner`, `extractors`, `parser-upgrade`, `integration`) | 2026-05-11T01:35:52Z | No |
| Dogfood regression tier | `cargo xtask test dogfood` | dogfood-current-contract | `235bd37c` | Passed 2 buckets in 228.5s | 2026-05-11T01:35:52Z | No |
| Formatter | `cargo fmt --check` | formatter | `235bd37c` | Passed | 2026-05-11T01:59:38Z | No |
| Diff whitespace | `git diff --check` | diff-hygiene | `235bd37c` | Passed | 2026-05-11T01:59:38Z | No |
| Extractor bucket (golden + capability_matrix + cert + downstream-smoke) | `cargo xtask test bucket extractors` | extractors-current-contract | `235bd37c` | Passed 4 commands in 21.8s | 2026-05-11T01:59:38Z | No |
| Parser upgrade current contract | `cargo xtask test bucket parser-upgrade` | parser-upgrade-current-contract | `235bd37c` | Passed 2 commands in 1.6s | 2026-05-11T01:59:38Z | No |
| Dev regression tier | `cargo xtask test dev` | dev-current-contract | `235bd37c` | Passed 32 buckets in 353.4s | 2026-05-11T01:59:38Z | No |
| System regression tier | `cargo xtask test system` | system-current-contract | `235bd37c` | Passed 6 buckets in 88.2s | 2026-05-11T01:59:38Z | No |
| Full regression tier | `cargo xtask test full` | full-current-contract | `235bd37c` | Passed 40 buckets in 669.1s | 2026-05-11T01:59:38Z | No |
| Release binary build | `cargo build --release` | release-build-current-contract | `235bd37c` | Passed in 2m44s | 2026-05-11T01:59:38Z | No |
| Example consumer build | `cargo build --examples -p julie-extractors` | example-build-current-contract | `235bd37c` | Passed | 2026-05-11T01:59:38Z | No |
| Crate doctests | `cargo test -p julie-extractors --doc` | doctest-current-contract | `235bd37c` | Passed 1 test | 2026-05-11T01:59:38Z | No |
| Pillar-3 downstream-consumer gate | `cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate` | downstream-smoke-current-contract | `235bd37c` | Passed in 16.6s | 2026-05-11T01:59:38Z | No |
| Startup repair planning regression | `cargo nextest run --lib test_startup_noop_repair_does_not_mark_catchup_active_while_planning` | startup-health-noop-regression | `88998e69` | Passed: no-op startup repair does not report catch-up active while only planning | 2026-05-11T03:31:43Z | No |
| Workspace startup/health focused regression | `cargo nextest run --lib tests::tools::workspace::mod_tests` | workspace-mod-tests | `88998e69` | Passed 41 tests; nextest reported 1 leaky test, exit 0 | 2026-05-11T03:31:43Z | No |
| System regression tier after startup-health fix | `cargo xtask test system` | system-current-contract | `88998e69` | Passed 6 buckets in 86.5s | 2026-05-11T03:31:43Z | No |
| Release binary after startup-health fix | `cargo build --release` | release-build-current-contract | `88998e69` | Passed in 2m 29s | 2026-05-11T03:31:43Z | No |
| Live daemon health after rebuild and restart | `julie-server --workspace . --json tool manage_workspace --params '{"operation":"health"}'` | live-health-current-contract | `88998e69` | READY / FULLY READY: daemon serving, SQLite healthy, 46843 symbols, 56538 relationships, projection current 409/409, embeddings initialized | 2026-05-11T03:31:43Z | No |
| Live daemon call graph | `julie-server --workspace . --json tool call_path --params '{"from":"extract_symbols_static","to":"extract_canonical","max_hops":6}'` | live-call-path-current-contract | `88998e69` | Found one-hop edge from `extract_symbols_static` to `extract_canonical` at `src/tools/workspace/indexing/extractor.rs:24` -> `crates/julie-extractors/src/pipeline.rs:8` | 2026-05-11T03:31:43Z | No |
| Live daemon references for extraction API | `julie-server --workspace . --json tool fast_refs --params '{"symbol":"extract_canonical","limit":20}'` | live-fast-refs-current-contract | `88998e69` | Found definition plus 20 visible references, including public API projection and cross-file contract callers | 2026-05-11T03:31:43Z | No |
| Live semantic state in SQLite | `sqlite3 ~/.julie/indexes/best-in-class-treesitter_2ad7e041/db/symbols.db "SELECT workspace_id, component, version FROM index_engine_state WHERE component='semantic_index_engine';"` | live-sqlite-state-current-contract | `88998e69` | `best-in-class-treesitter_2ad7e041|semantic_index_engine|extractors=2026-05-10.tree-sitter-best-in-class-v1+schema=2026-05-05.reference-identifier-v3` | 2026-05-11T03:31:43Z | No |
| Live daemon non-force refresh | `julie-server --workspace . --json tool manage_workspace --params '{"operation":"refresh","workspace_id":"best-in-class-treesitter_2ad7e041"}'` | live-refresh-current-contract | `88998e69` | Already up-to-date at canonical revision 409; 1588 files, 46843 symbols, 56538 relationships | 2026-05-11T03:31:43Z | No |

## Exceptions

Exceptions are allowed only when they are explicit and tested. An exception must name the language, capability, reason, and the test that locks the exception in place.

Active exceptions:

- None.
