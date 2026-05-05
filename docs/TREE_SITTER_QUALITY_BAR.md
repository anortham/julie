# Tree-Sitter Quality Bar

Updated: 2026-05-05

Julie calls the tree-sitter layer best-in-class only when extraction output is a truthful, tested product contract for every supported language entry. This document defines that bar. It is the release-readiness source of truth; implementation plans may link here, but they should not carry a separate final verdict.

## Current Verdict

Status: offline release-candidate gates and live restart dogfood pass for code commit `0b7a2f36`. Treat `0b7a2f36` as the dogfooded release-candidate code SHA; rerun the release gates and live dogfood if code changes after that commit.

The tree-sitter upgrade work has materially improved parser coverage, golden fixtures, parser-upgrade gates, relationship precision, live dogfood repair, and semantic index invalidation. The current working tree fixes the latest review findings:

- Semantic-version full repair now preserves the embedding contract by running embedding catch-up after destructive semantic repair.
- Semantic-version full repair now uses force-equivalent cancellation and watcher pause behavior.
- `super` calls no longer resolve through same-scope `self` handling.
- Relationship capability claims now require golden graph evidence, and `vue` and `regex` no longer claim relationship output while their extractors return none.
- Shared target-workspace integration fixtures now use one cross-module lock, so release gates cannot corrupt the fixture SQLite stores while running in parallel.
- The old plan evidence is historical only. Release evidence must be recorded here against the exact release commit.

## Definition

Best-in-class for Julie means:

1. Every tree-sitter registry entry is represented in the capability matrix.
2. Every parser-backed registry entry has at least one production-path golden fixture that uses `extract_canonical`.
3. Capability rows are truthful. If a language claims symbols, identifiers, relationships, pending relationships, or types, at least one golden fixture proves that capability unless the row records an explicit exception.
4. Wrong graph edges are treated as release blockers. Ambiguous or receiver-qualified calls should stay unresolved or pending instead of becoming confident edges to a local symbol.
5. Parser dependency upgrades are accepted only behind whole-inventory golden and real-world gates.
6. Startup, watcher, refresh, and force-reindex paths produce the same semantic extraction contract.
7. Semantic index engine changes automatically repair stale derived data, including embeddings when available.
8. Live dogfood must prove the release binary, daemon restart, health status, search, refs, and call path behavior on Julie itself.

## Language Inventory

The quality bar covers these registry entries:

| Capability group | Language entries |
| --- | --- |
| Full extraction | `rust`, `c`, `cpp`, `go`, `zig`, `typescript`, `tsx`, `javascript`, `jsx`, `python`, `java`, `csharp`, `vbnet`, `php`, `ruby`, `swift`, `kotlin`, `scala`, `dart`, `elixir`, `bash`, `powershell`, `gdscript` |
| Pending relationships, no type output | `lua`, `qml`, `r` |
| No pending relationships | `html`, `razor`, `sql` |
| Symbol/type extraction, no relationship output | `vue`, `regex` |
| Data-only extraction | `css`, `markdown`, `json`, `toml`, `yaml` |

`text` is not a tree-sitter language entry. It still matters for indexing policy parity, but it does not count toward parser-backed capability coverage.

## Golden Fixture Contract

Golden fixtures must compare normalized `extract_canonical` output. Normalization may remove derived noise, but it must not hide names, kinds, spans, parent IDs, relationship direction, relationship kind, containing symbol IDs, type values, identifiers, or parse diagnostics.

Required coverage:

- A registry entry without a capability row fails the matrix test.
- A registry entry without at least one golden fixture fails the matrix test.
- A capability row that claims relationships must have at least one fixture with `relationships`, `pending_relationships`, or `structured_pending_relationships`, unless an explicit exception explains why relationships are unavailable for that language group.
- A full-extraction language fixture should include at least one named definition, one parented or explicit flat-structure assertion, one identifier when identifiers are supported, one graph output when relationships are supported, and one type output when types are supported.
- A pending-relationship language fixture should include unresolved graph output and should assert the intended absence of type output when types are unsupported.
- A data-only language fixture should assert available symbols or identifiers and explicit absence of unsupported graph and type outputs.

## Real-World Contract

Real-world fixtures are not a substitute for small golden fixtures. They are parser-upgrade regression cases for stable high-value outputs that appear in realistic files.

Required coverage:

- Every supported language with an existing real-world fixture must have expected-output assertions.
- The parser-upgrade bucket must fail on missing expected symbols, identifiers, parent links, representative graph outputs, and type or doc-comment outputs where those are stable for the language.
- Real-world fixtures should prefer stable facts over exhaustive snapshots. They should catch parser drift without making unrelated fixture formatting expensive.

## Relationship Precision Contract

Relationship extraction must prefer no edge over a wrong edge.

Required behavior:

- Duplicate local names must not survive in legacy unique lookup maps.
- Unqualified calls resolve only when there is one credible local target or one concrete definition among declarations.
- Receiver-qualified calls to non-self receivers stay pending or receiver-qualified unless language-specific code can prove the target.
- `self`, `this`, and language-specific same-instance receivers may resolve to same-parent methods.
- `super` must not use same-parent resolution. It needs explicit inheritance-aware resolution, or it must stay pending.
- Cross-file or import-qualified calls should retain structured pending context so later resolvers can distinguish duplicate terminal names.

## Semantic Repair Contract

The semantic index engine version is part of the indexed-data contract. When it changes:

- Startup repair, explicit index, and refresh must detect stale stamps.
- A stale stamp must repair symbols, identifiers, relationships, type rows, search projection, and semantic vectors when embeddings are available.
- Any semantic-version repair that performs full persistence must use force-equivalent cancellation and watcher pause behavior.
- A non-force refresh after successful repair must report current state and must not loop a full reindex.

## Parser Upgrade Contract

Parser upgrades must follow [TREE_SITTER_UPGRADES.md](TREE_SITTER_UPGRADES.md).

Required evidence:

- Core tree-sitter version.
- Parser crate versions or git revisions.
- ABI support range.
- Fixture corpus revision.
- Parser-upgrade bucket result at the exact commit being released.

## Release Gates

A release can claim this quality bar only when these commands pass at the exact release commit:

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

## Verification Ledger

Record release evidence with the template in [verification-ledger-template.md](plans/verification-ledger-template.md). Evidence may be reused only when the scope label and commit SHA match the current HEAD exactly.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
| --- | --- | --- | --- | --- | --- | --- |
| Formatter | `cargo fmt --check` | formatter | `0b7a2f36` | Passed | 2026-05-05T16:35:22Z | No |
| Diff whitespace | `git diff --check` | diff-hygiene | `0b7a2f36` | Passed | 2026-05-05T16:35:22Z | No |
| Extractor golden and capability matrix | `cargo xtask test bucket extractors` | extractors | `0b7a2f36` | Passed 1 bucket in 1.1s | 2026-05-05T16:35:22Z | No |
| Parser upgrade contract | `cargo xtask test bucket parser-upgrade` | parser-upgrade | `0b7a2f36` | Passed 1 bucket in 1.7s | 2026-05-05T16:35:22Z | No |
| Changed-code regression tier | `cargo xtask test changed` | changed | `0b7a2f36` | Passed 22 buckets in 537.5s | 2026-05-05T16:35:22Z | No |
| Startup, workspace, daemon, integration tier | `cargo xtask test system` | system | `0b7a2f36` | Passed 6 buckets in 142.7s | 2026-05-05T16:35:22Z | No |
| Search and dogfood tier | `cargo xtask test dogfood` | dogfood | `0b7a2f36` | Passed 2 buckets in 206.7s | 2026-05-05T16:35:22Z | No |
| Full release-candidate tier | `cargo xtask test full` | full | `0b7a2f36` | Passed 30 buckets in 868.9s | 2026-05-05T16:35:22Z | No |
| Release binary build | `cargo build --release` | release-build | `0b7a2f36` | Passed in 2m 51s | 2026-05-05T16:35:22Z | No |
| Live MCP health after rebuild and restart | `manage_workspace health detailed=true` | live-health | `0b7a2f36` | READY: daemon serving, projection current 3970/3970, 34252 symbols, 32945 relationships, 7009 vectors | 2026-05-05T16:42:28Z | No |
| Live production call graph | `call_path extract_symbols_static extract_canonical` | live-call-path | `0b7a2f36` | Found one-hop production call edge through `src/tools/workspace/indexing/extractor.rs:24` to `crates/julie-extractors/src/pipeline.rs:8` | 2026-05-05T16:42:28Z | No |
| Live references for extraction API | `fast_refs extract_canonical` | live-fast-refs | `0b7a2f36` | Found definition plus 20 visible references, including public API projection and real-world contract callers | 2026-05-05T16:42:28Z | No |
| Live semantic state in SQLite | `sqlite3 ~/.julie/indexes/julie_528d4264/db/symbols.db` | live-sqlite-state | `0b7a2f36` | Schema version 24, semantic engine `2026-05-05.reference-identifier-v3`, Tantivy projection ready at 3970/3970, 7009 vector rowids | 2026-05-05T16:42:28Z | No |
| Live non-force refresh | `manage_workspace refresh workspace_id=julie_528d4264` | live-refresh | `0b7a2f36` | Already up-to-date at canonical revision 3970; no repeated full reindex | 2026-05-05T16:42:28Z | No |

## Exceptions

Exceptions are allowed only when they are explicit and tested. An exception must name the language, capability, reason, and the test that locks the exception in place.

Active exceptions:

- None.
