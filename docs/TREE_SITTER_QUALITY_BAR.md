# Tree-Sitter Quality Bar

Updated: 2026-05-05

This document defines the fixed best-in-class target for Julie's tree-sitter layer. It is not allowed to move down to match the current implementation. Passing the current extractor gates proves the current contract; it does not, by itself, prove this target.

Implementation plans and evidence ledgers may link here, but they do not get to redefine the bar. If Julie cannot meet a requirement yet, that requirement stays here and the implementation is marked incomplete.

## Current Verdict

Status: **not achieved as a release claim**.

The 2026-05-05 code at `0b7a2f36` passed the offline release-candidate gates and live restart dogfood for the corrected review findings. That is good evidence for the fixes in that batch, but it is not enough to call the tree-sitter layer best-in-class.

The current working branch closes the implementation gaps that made this document say "not achieved": capability accounting now has separate target and implemented rows, and native relationship output exists for Vue SFCs, regex backreferences, CSS references, Markdown local heading links, and YAML aliases.

This still cannot be called achieved until the fixed-target release gates below pass at the current commit SHA, followed by live MCP dogfood after a release rebuild and restart. The historical plan evidence proves specific older commits and scopes. It does not prove this quality bar unless the commit SHA, scope label, and target requirements all match.

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

| Gap | Status | Required closure |
| --- | --- | --- |
| Fixed-target release evidence has not been recorded at the current commit SHA | Open | Run and record the fixed-target gates for the current HEAD: formatter, extractor bucket, parser-upgrade bucket, changed tier, dev tier, dogfood tier, full tier, release build, and live MCP restart dogfood. |

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

## Exceptions

Exceptions are allowed only when they are explicit and tested. An exception must name the language, capability, reason, and the test that locks the exception in place.

Active exceptions:

- None.
