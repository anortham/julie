# Julie extractor consumer migration implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Consume the reviewed newer julie-extractors release while preserving Julie's full indexing, embeddings, navigation, and safe AST editing behavior and retaining the new receiver-type facts.

**Architecture:** Upstream owns extraction facts and parser construction. Julie owns source-text projection, persistence, resolution policy, and edits. Introduce a Julie-owned symbol projection with the upstream symbol nested and flattened for serialization; retain `code_context` in Julie rather than restoring it upstream. Change the breaking dependency and every required consumer together in one atomic, compiling task.

**Tech Stack:** Rust workspace, julie-extractors pinned git dependency, tree-sitter via upstream's optional syntax API, SQLite and Tantivy.

**Architecture Quality:** Medium/high risk: changing a shared symbol type crosses all five consumer crates, and parser mistakes can corrupt edits. Keep one projection boundary in `julie-core`; never fork grammars or expose upstream internals. Root Codex is the final implementation reviewer. A worker does not accept its own migration.

## Global Constraints

- Plan-authoring scope only: the user requested this document in the current checkout, no new worktree, no implementation, no commit. These restrictions describe this writing session; approved future execution follows the coordinator's recorded branch and commit policy.
- Released target: `v2.41.0`, peeled commit `a71e18c1a6fae67b15d2d0aaa20793330fda901f`. Use this tag consistently in all six manifests and verify Cargo.lock resolves that commit. Do not substitute current upstream main or a moving branch. Execution still follows the owner's worker assignment.
- Upstream prerequisite: `/home/murphy/source/julie-extractors/docs/plans/2026-09-07-julie-consumer-syntax-api.md`; coordinator must verify its approved contract before dispatch.
- Preserve full functionality; no deletion of semantics, regex replacement for parser-backed code, placeholders, weakened tests, or silent reduction in language support.
- Keep full extraction for indexing. `ExtractionLevel::Symbols` is not an indexing substitute: identifiers, types, pending and resolved relationships, source regions, structural facts, literals, annotations, type arguments, complexity, parse diagnostics, and body spans/hashes must survive.
- Preserve relative Unix-style paths, source byte offsets, Unicode boundaries, CRLF, files with BOM, scope-sensitive identities, and existing full/lightweight query behavior.
- Run one test command at a time. Workers run exact tests only; the coordinator owns regression tiers. Code changes first get `cargo check`; use package selectors after the crate split.
- New implementation files ≤500 lines, tests ≤1000 lines. Split touched oversized files by responsibility, without unrelated cleanup. Tests contain no comments.
- No push, publication, release, destructive Git action, or external messages without explicit authorization. `parallel-lead-commit` means workers never commit; coordinator may commit only when authorized.
- Runtime rearchitecture and semantic-sidecar migration belong to their separate plans. This plan must continue to work with the current Python provider.

## Released upstream target

GitHub published [v2.41.0](https://github.com/anortham/julie-extractors/releases/tag/v2.41.0) on 2026-09-07 at 20:16:20Z. Its annotated tag object is `b8b9f5498e37397e54ab8aac1e68e2d8936188f1`; the dependency commit is `a71e18c1a6fae67b15d2d0aaa20793330fda901f`. Local upstream main `5912bafe185ce2653ce22f86ec962aef615a27f8` differs from the tag only in publication evidence/docs/memory, not source.

The release contains `syntax-api`, both parsing entry points, bounded SyntaxOptions, all six typed errors, and root PendingSpan/UnresolvedTarget exports. Extraction identity epoch stays 9; extraction contract is unchanged from v2.40.6. That does not remove Julie's v2.34.3-to-v2.41.0 migration/reindex requirement. Enable `syntax-api` only in the consumer that needs syntax and keep a single resolved extractor/tree-sitter identity.

Publication and interface inspection establish the dependency target. Review/verification scope is recorded in [release readiness evidence](../findings/2026-09-07-julie-extractors-v2.41.0-readiness.md); do not treat its tests as Julie migration verification.

## Recorded source state and exact contract

Research used `/home/murphy/source/julie`, `main`, `0432158c173c30ae2f76d92773380c744ddc6542`. The initial tree was clean; concurrent lead work subsequently added memory/findings documents. Leave those alone. This is an evidence anchor, not permission to reset the checkout.

Current pins are `v2.34.3` in all six manifests: `Cargo.toml`, `crates/julie-core/Cargo.toml`, `crates/julie-index/Cargo.toml`, `crates/julie-pipeline/Cargo.toml`, `crates/julie-runtime/Cargo.toml`, `crates/julie-tools/Cargo.toml`. Root package is `julie`; package test ownership follows each named crate.

Verified current newer public API:

```rust
julie_extractors::extract_canonical(file_path: &str, content: &str, workspace_root: &std::path::Path)
    -> anyhow::Result<julie_extractors::ExtractionResults>;
julie_extractors::detect_language_for_path(file_path: &std::path::Path, content: &str)
    -> Option<&'static str>;
julie_extractors::capability_snapshot().languages();
```

`languages()` yields `&CapabilityRow`; `language: String` and `extensions: Vec<String>` are public. Use this for extension coverage; do not call private `language_specs`, `language::supported_extensions`, or extension-only language helpers. Root `extract_canonical` still returns the complete facts; there is no promised parse-once extraction interface.

Published upstream API verified in v2.41.0:

```rust
pub fn parse_source(file_path: &std::path::Path, source: &str)
    -> Result<ParsedSource, SyntaxError>;
pub struct ParsedSource {
    pub language: &'static str,
    pub tree: tree_sitter::Tree,
    pub diagnostics: Vec<julie_extractors::ParseDiagnostic>,
}
pub struct SyntaxOptions<'a> {
    pub deadline: Option<std::time::Instant>,
    pub cancelled: Option<&'a std::sync::atomic::AtomicBool>,
    pub max_source_bytes: usize,
}
pub fn parse_source_with_options(
    file_path: &std::path::Path,
    source: &str,
    options: &SyntaxOptions<'_>,
) -> Result<ParsedSource, SyntaxError>;
```

Path is `julie_extractors::syntax`, gated by Cargo feature `syntax-api`. Recovery trees return successfully with diagnostics. The agreed error variants are `UnsupportedLanguage { path: PathBuf }`, `UnsupportedContainer { path: PathBuf }`, `InputTooLarge { bytes: usize }`, `ParseFailed { source: anyhow::Error }`, `Cancelled`, and `DeadlineExceeded`; verify them against the accepted upstream implementation. `parse_source` remains a convenience entry point; Julie request processing uses `parse_source_with_options`. `.jsonl` is an unsupported *syntax container*, not unsupported canonical extraction. Vue/Markdown return the host AST, not an injected-tree forest. Root exports must include `UnresolvedTarget` and `PendingSpan` required by Julie's resolver, with their existing actual definitions; do not restore a public `base` module.

The coordinator records the tag, SHA, `EXTRACTION_CONTRACT_VERSION`, `EXTRACTION_IDENTITY_EPOCH`, syntax error variants, root type exports, and `cargo metadata` tree-sitter resolution in the verification ledger before editing pins. A missing reviewed release or mismatching API is a real prerequisite blocker, not a reason to invent an adapter against private modules.

## File structure and ownership

Create these implementation files:

- `crates/julie-core/src/symbol.rs`: Julie symbol projection and checked extraction-to-source conversion.
- `crates/julie-core/src/database/migrations/receiver_type.rs`: additive identifier receiver-type migration, using the next free migration version after inspection (current maximum is 30).
- `crates/julie-tools/src/editing/syntax.rs`: consume the public syntax API and map typed errors into existing edit diagnostics.
- `crates/julie-pipeline/src/resolver/receiver_type.rs`: conservative candidate restriction using receiver facts, scoped owner identity, and inheritance evidence.

Create tests and fixtures:

- `crates/julie-core/src/tests/extractor_projection.rs` and `crates/julie-core/src/tests/receiver_type_storage.rs`.
- `crates/julie-pipeline/src/tests/extractor_migration.rs` and `crates/julie-pipeline/src/tests/receiver_type_resolution.rs`.
- `crates/julie-tools/src/tests/extractor_migration_editing.rs`.
- `fixtures/extractor-migration/` with Unicode/CRLF Rust, C++ header, F#, QML directory, JSONL, Vue script, and Markdown code-fence source/control pairs.
- `docs/findings/2026-09-07-julie-extractor-migration-ledger.md` for release identity, API import inventory, language ledger, before/after edit results, and verification evidence.

Mandatory modified files:

- All six manifests above and `Cargo.lock`.
- `src/extractors/mod.rs`, `src/language.rs` if present in the preflight inventory: remove private-module compatibility exports; provide explicit Julie aliases only for Julie-owned types.
- `crates/julie-core/src/lib.rs`, `crates/julie-core/src/file_policy.rs`, `crates/julie-core/src/tests/mod.rs`, `crates/julie-core/src/test_support/db/rows.rs`.
- `crates/julie-core/src/database/helpers.rs`, `crates/julie-core/src/database/symbols/storage.rs`, `crates/julie-core/src/database/symbols/bulk.rs`, `crates/julie-core/src/database/bulk/atomic.rs`, `crates/julie-core/src/database/bulk/identifiers.rs`, `crates/julie-core/src/database/identifiers.rs`, `crates/julie-core/src/database/schema.rs`, `crates/julie-core/src/database/migrations.rs` and the file declaring `LATEST_SCHEMA_VERSION`, discovered by Miller before edit.
- `crates/julie-pipeline/src/indexing_core/{normalized,extraction,batch}.rs`, `crates/julie-pipeline/src/resolver.rs`, `crates/julie-pipeline/src/resolver/scoring.rs`, `crates/julie-pipeline/src/resolver/namespace.rs`, `crates/julie-pipeline/src/finalize.rs`, `crates/julie-pipeline/src/tests/mod.rs`.
- `crates/julie-runtime/src/watcher/handlers.rs` and all manager plumbing proven by its reference inventory: `crates/julie-runtime/src/watcher/{mod,runtime}.rs`, `crates/julie-runtime/src/watcher/runtime/{processing,repairs}.rs`, `crates/julie-runtime/src/workspace/mod.rs`.
- `crates/julie-tools/src/editing/{mod,rewrite_symbol}.rs`, `crates/julie-tools/src/refactoring/mod.rs`, `crates/julie-tools/src/impact/ranking.rs`, `crates/julie-tools/src/tests/mod.rs`.
- `src/tools/workspace/indexing/engine_version.rs`, `src/tests/core/engine_version.rs`, `src/tests/integration/{real_world_contract,real_world_validation/real_world_tests}.rs`.
- `crates/julie-index/src/search/projection/apply.rs`, embedding and analysis consumers, plus exact import/type-construction call sites found in the mandatory inventory below.
- `docs/ADDING_NEW_LANGUAGES.md`, `docs/TREE_SITTER_UPGRADES.md`, `docs/DEPENDENCIES.md`; update current supported-language statements in project docs only after the evidence ledger is complete.

The source has hundreds of symbol constructors and internal API imports. Before dispatch, the coordinator uses Miller search/trace to record the exhaustive *exact paths* into the ledger for `julie_extractors`, `crate::extractors`, `Symbol {`, `code_context`, `ExtractorManager`, `get_tree_sitter_language`, `parse_diagnostics_for_tree`, `UnresolvedTarget`, `PendingSpan`, and `Visibility`. This closed inventory becomes the atomic worker's additional exact ownership. Do not infer unlisted paths from a glob, and do not treat a truncated MCP result as exhaustive. Update the inventory if a compiler diagnostic identifies an omitted consumer. This is a prerequisite ownership inventory, not permission for unrelated edits.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, `docs/TREE_SITTER_UPGRADES.md`, crate manifests, and current xtask bucket definitions.

**Worker red/green scope:** `cargo check` then `cargo nextest run -p <owning-package> --lib <exact_test_name>`. Each exact behavior test gets one RED and one GREEN run; diagnose failures rather than repeating unchanged commands. Root tests use `-p julie`; core/pipeline/runtime/tools tests use their actual package names.

**Worker ceiling:** Exact tests listed in the task only, serialized. No `dev`, broad filters, or parallel nextest invocations. Coordinator runs package `cargo check` as needed to establish the atomic dependency state compiles.

**Worker gate invariant:** Source projection remains valid after upstream drops text; all canonical facts survive normalization/storage; receiver hints do not invent exact references; AST rename/rewrite retain successful existing behavior and safe refusal.

**Lead affected-change scope:** `cargo xtask test changed`. If OverBudget, record the mapped scope and use `cargo xtask test changed --scale` deliberately; do not misreport OverBudget as a test failure or a passing gate.

**Branch gate:** `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo xtask test bucket extractor-dep-integration`, and once per completed batch `cargo xtask test dev`. Add `cargo xtask test system` for index rebuild/watcher changes and `cargo xtask test dogfood` because source projection/resolution changes affect retrieval. Confirm the calibrated bucket actually selects tests in the split packages; repair runner mapping if it omits this migration's regression coverage, with exact xtask tests and review.

**Security scope:** none declared for this plan. No external data transfer or release is part of the task.

**Replay/metric evidence:** Hard gates are behavior parity, valid spans, canonical-fact round trips, no incorrect exact edges, complete applicable-language coverage, single dependency identity, and successful invalidation/rebuild. Report-only metrics are extraction duration, index size, embedding input size, and lexical/semantic result changes on recorded fixtures. Existing golden expectations are not rewritten simply because a new extractor produces different output; review each changed expectation against source evidence.

**Escalation triggers:** Parser crash, missing facts, source-range drift, unsafe edits, or a supported embedded operation becoming unsupported block acceptance and require diagnosis. New upstream parser defects are fixed upstream under its plan rather than worked around silently. Windows/NTFS verification is coordinator-owned when applicable, using the win-test skill.

**Assigned verification failure:** A worker diagnoses and fixes in-scope failures, then reports the cause and additional exact verification needed; it must not weaken the gate or accept a failing task.

**Verification ledger:** Use `docs/plans/verification-ledger-template.md` if present at execution. If absent, the new findings ledger must contain invariant, command, package, scope label, HEAD SHA, dirty diff identity, UTC timestamp, exit status, selected test count, and result. Reuse only matching HEAD, scope, and unchanged task content; same HEAD with a different dirty diff is not reusable. Record upstream identity separately. Test listing is diagnostic, never a passing test run.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Atomic extractor consumer migration | None - serial | All files in File structure and ownership, plus the coordinator's closed exact import inventory | Not applicable - single task. | Not applicable - single task. |

One implementer owns the atomic migration. Independent reviewers may inspect source/evidence concurrently, but may not edit shared files or run test commands. The root Codex coordinator reviews spec compliance and code quality, reconciles all state, and owns broad verification. Commit mode is `parallel-lead-commit`; the worker hands off a verified diff and does not commit.

## Task 1: Atomic extractor consumer migration

**Files:** Exact create/modify/test ownership above, including the preflight import inventory.

**Interfaces:** Consumes the reviewed upstream root extraction API and optional syntax API, complete `ExtractionResults`, existing SQLite symbol/identifier schemas, and source content from the same read used to extract. Produces `julie_core::symbol::Symbol`, checked normalization accepting source text, persisted optional receiver facts, AST edits through supported upstream syntax, and one new composed semantic engine identity.

**Contract inputs:** Reviewed upstream tag/SHA, verified public exports, existing source/control fixtures, full language capability snapshot, current schema maximum, and behavior baseline collected before pin changes.

**File ownership:** All files in File structure and ownership, plus the coordinator's closed exact import inventory.

**Serialization required:** Not applicable - single task.

**Dependency reason:** Not applicable - single task.

### Step 1: Record the compiling baseline and write behavioral tests

Before changing pins, record existing exact tests and characterize each public edit operation on Rust, TypeScript, Vue script, Markdown heading/fence, and JSONL. `rewrite_symbol` already uses a host AST and separately extracted symbols; that does not prove every embedded operation is unsupported. Record actual success/output or safe refusal per operation. Preserve successful cases exactly; do not replace them with new refusals. Include whole-symbol, body-only, signature, and identifier rename cases where the current tool exposes them. Use current tool constructors from Miller, not guessed JSON parameters.

Add the projection tests below in `crates/julie-core/src/tests/extractor_projection.rs` and register the module. The new projection API is deliberately Julie-owned; missing import/function is valid RED before implementation. These tests compile after Step 3.

```rust
use crate::symbol::Symbol;
use std::path::Path;

#[test]
fn projected_symbol_keeps_exact_unicode_source_and_extraction_facts() {
    let source = "// π\r\npub fn café() -> &'static str { \"雪\" }\r\n";
    let extracted = julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let fact = extracted.symbols.into_iter().find(|s| s.name == "café").unwrap();
    let expected = source.get(fact.start_byte as usize..fact.end_byte as usize).unwrap().to_owned();
    let original = serde_json::to_value(&fact).unwrap();
    let projected = Symbol::from_extracted(fact, source).unwrap();
    assert_eq!(projected.code_context.as_deref(), Some(expected.as_str()));
    assert_eq!(serde_json::to_value(&projected.extracted).unwrap(), original);
    let serialized = serde_json::to_value(&projected).unwrap();
    assert!(serialized.get("extracted").is_none());
    assert_eq!(serialized["code_context"], expected);
}

#[test]
fn projected_symbol_rejects_out_of_bounds_and_non_utf8_ranges() {
    let source = "pub fn café() {}";
    let extracted = julie_extractors::extract_canonical("sample.rs", source, Path::new(".")).unwrap();
    let mut fact = extracted.symbols.into_iter().find(|s| s.name == "café").unwrap();
    fact.end_byte = u32::MAX;
    assert!(Symbol::from_extracted(fact.clone(), source).is_err());
    fact.start_byte = (source.find('é').unwrap() + 1) as u32;
    fact.end_byte = source.len() as u32;
    assert!(Symbol::from_extracted(fact, source).is_err());
}
```

Add the registry integration test in `crates/julie-pipeline/src/tests/extractor_migration.rs`:

```rust
#[test]
fn consumer_extension_registry_covers_every_upstream_extension() {
    let actual = julie_core::file_policy::supported_extensions_for_indexing();
    for language in julie_extractors::capability_snapshot().languages() {
        for extension in &language.extensions {
            assert!(actual.contains(&extension.to_lowercase()), "{} {}", language.language, extension);
        }
    }
    assert_eq!(julie_core::file_policy::detect_language_for_indexing_with_content(
        std::path::Path::new("sample.fs"), "module Sample\nlet answer = 42\n"), "fsharp");
}
```

In `crates/julie-tools/src/tests/extractor_migration_editing.rs`, add:

```rust
#[test]
fn migrated_parser_renames_identifiers_without_touching_literals() {
    let source = "class Example { run() { return \"Example\"; } }";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool.smart_text_replace(source, "Example", "Renamed", "sample.ts", false).unwrap();
    assert_eq!(output, "class Renamed { run() { return \"Example\"; } }");
}
```

This last test may already pass before migration: retain it as a characterization guard, not a fabricated RED. New projection and receiver storage tests below must demonstrate their real missing behavior before their implementation. Add exact fixtures for invalid spans, zero-length synthetic symbols, source regions embedded in host files, and full versus lightweight DB reads. For zero-length source ranges, preserve valid facts and represent absent text explicitly; never fabricate a body or use line slicing as an unchecked fallback.

### Step 2: Run only the exact RED scopes

```sh
cargo nextest run -p julie-core --lib projected_symbol_keeps_exact_unicode_source_and_extraction_facts
cargo nextest run -p julie-core --lib projected_symbol_rejects_out_of_bounds_and_non_utf8_ranges
cargo nextest run -p julie-pipeline --lib consumer_extension_registry_covers_every_upstream_extension
```

Run commands sequentially. Missing `crate::symbol` is the expected new-contract RED; the registry test must fail only for an actual missing registry/language behavior, or be recorded as an existing preservation guard. Do not count a dependency download failure as RED. No task is accepted in this intermediate state.

### Step 3: Introduce the owned projection and migrate the dependency atomically

Implement `crates/julie-core/src/symbol.rs`:

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Symbol {
    #[serde(flatten)]
    pub extracted: julie_extractors::Symbol,
    pub code_context: Option<String>,
}

impl Symbol {
    pub fn from_extracted(extracted: julie_extractors::Symbol, source: &str) -> Result<Self> {
        let start = extracted.start_byte as usize;
        let end = extracted.end_byte as usize;
        let text = source.get(start..end).with_context(|| {
            format!("invalid symbol span {}:{}..{}", extracted.file_path, start, end)
        })?;
        let code_context = (!text.is_empty()).then(|| text.to_owned());
        Ok(Self { extracted, code_context })
    }
}

impl Deref for Symbol {
    type Target = julie_extractors::Symbol;
    fn deref(&self) -> &Self::Target { &self.extracted }
}

impl DerefMut for Symbol {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.extracted }
}
```

Keep serialization flat, retain every upstream field, and keep DB column names stable. For stored and lightweight rows, move the complete current upstream-field mapping from `row_to_symbol` and `row_to_symbol_lightweight` into the nested `extracted` value; keep only `code_context` on Julie's wrapper. Preserve the exact nullable/lightweight omissions already present. Existing body-span and body-hash columns already exist; do not create duplicate columns. Constructors in test helpers must retain supplied context and move remaining fields into `extracted`. Replace upstream Symbol imports in Julie-owned storage/search/embedding consumers with `julie_core::symbol::Symbol`. Pass `&symbol.extracted` only at a verified upstream API boundary. Do not rely on Deref for moving owned fields; explicitly destructure `extracted` where needed.

Change `normalize_extraction_results` to consume `source: &str` and return `anyhow::Result<NormalizedExtractionData>`. Map symbols with `Symbol::from_extracted(symbol, source)` and `collect::<Result<Vec<_>>>()?`, then run Julie's `classify_symbols_by_role` on the projected symbols. Literal classification retains its current position and types. Migrate `julie-index` analysis APIs to the owned Symbol consistently so the normalization boundary compiles. Preserve every other extraction-results field and its existing enrichment. Update all three actual call sites: text-only and parser-backed paths in `indexing_core/extraction.rs`, and watcher `handlers.rs`. Text-only uses its actual source and empty canonical results. The source must be the same content supplied to extraction, never a second file read after an edit. Projection error is an extractor failure routed through existing repair handling; do not persist a partial success or silently drop the invalid symbol.

All six manifests must resolve the same reviewed upstream source. Enable `syntax-api` in the tools dependency that actually calls it and ensure one compatible tree-sitter version. Update `Cargo.lock` deliberately and inspect `cargo tree -i julie-extractors` and `cargo tree -i tree-sitter` for duplicate incompatible identities. Replace private root shim exports in `src/extractors/mod.rs` and every inventory import with actual root APIs. Do not rebuild compatibility modules named after removed private modules.

Replace `ExtractorManager` extraction calls with canonical extraction and the appropriate explicit projection. Remove its now-useless watcher/session plumbing in the same atomic task; update exact callers/tests from the inventory. Migrate direct per-language extractor tests in `real_world_tests.rs` to canonical extraction while preserving the source assertions, not reducing them to symbol-count-only smoke.

Use capability metadata for extensions:

```rust
julie_extractors::capability_snapshot()
    .languages()
    .flat_map(|row| row.extensions.iter())
    .map(|extension| extension.to_lowercase())
    .collect::<std::collections::HashSet<_>>()
```

Use root `detect_language_for_path(path, content)` for source-sensitive routing and the same function with empty content only when source is unavailable. Keep Julie's explicit text-only policies and extensionless filename handling. Test `.h` source-aware C/C++, `qmldir`, F# `.fs/.fsi/.fsx`, aliases JSX/TSX, uppercase extensions, and JSONL. Never pretend a parser exists by checking whether a path has an extension.

### Step 4: Preserve receiver facts through storage and conservative resolution

Add nullable `receiver_type TEXT` to identifiers via the next available migration and fresh schema. Extend the actual insert columns/bindings in `database/bulk/identifiers.rs`, `IdentifierRef` and row projection in `database/identifiers.rs`, and all affected fixture constructors. Preserve `Identifier.code_context` as its existing optional field; its upstream removal/population policy is separate from Symbol text ownership. `StructuredPendingRelationship.receiver_type` remains attached through normalization, finalization, and resolver dispatch; initialize `None` only when upgrading an old synthetic pending fact with no receiver evidence.

Add this storage RED test to `crates/julie-core/src/tests/receiver_type_storage.rs` before the migration implementation:

```rust
#[test]
fn receiver_type_column_is_present_after_open_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("symbols.db");
    for _ in 0..2 {
        let db = crate::database::SymbolDatabase::new(&path).unwrap();
        assert!(db.has_column("identifiers", "receiver_type").unwrap());
    }
}
```

Run `cargo nextest run -p julie-core --lib receiver_type_column_is_present_after_open_and_reopen` once RED, implement the migration, then once GREEN after `cargo check`. Also add a round-trip extraction test using two classes with the same method name and a typed self call: extract canonical identifiers, store with existing `bulk_store_identifiers`, reopen, and assert the exact receiver value survives both full and reference queries. Include null old rows and atomic replacement. Use actual foreign-key fixture helpers after inspecting them; do not disable constraints.

Extend resolver candidate evaluation with `receiver_type: Option<&str>` from structured pending facts. Resolve the enclosing owner using caller scope, namespace, language, and recorded inheritance facts before selecting member candidates. A bare owner-name match across namespaces is insufficient. For a self call restrict to the verified owner; for a base/super call use verified base relationships; an unresolved owner or ambiguous inherited target remains unresolved instead of promoting name-only evidence. Keep `reference_site_is_exact` and span provenance untouched: receiver evidence alone does not create an exact target token. Avoid arbitrary new ranking constants that can overpower scope evidence.

Add `receiver_type_resolution.rs` tests for same-name members on unrelated classes, same-name owners in separate namespaces, base calls, absent hint, aliases, and ambiguous multiple candidates. Build each fixture through `extract_canonical` and normalized storage, then call the real `resolve_structured_batch`, asserting target symbol IDs and exact-site provenance. Inspect existing `batch_resolver.rs` for real DB builders before writing them. The positive and negative cases are hard gates; no language-specific name heuristic qualifies as a shared resolver implementation.

### Step 5: Migrate safe editing through the approved syntax API

Enable the optional feature and replace local parser construction in `rewrite_symbol::parse_live_tree` and `SmartRefactorTool::smart_text_replace_with_line_filter` with the bounded entry point. The adapter receives a configured maximum source size and budget, validated before parse admission. The following adapter body uses its caller-supplied `max_source_bytes`, `deadline`, and shared `cancelled` flag:

```rust
let options = julie_extractors::syntax::SyntaxOptions {
    deadline: Some(deadline),
    cancelled: Some(cancelled),
    max_source_bytes,
};
let parsed = julie_extractors::syntax::parse_source_with_options(
    std::path::Path::new(file_path),
    content,
    &options,
)?;
let tree = parsed.tree;
let parse_diagnostics = parsed.diagnostics;
```

For this migration's current handlers, supply explicit configurable size and time budgets from the adapter's validated configuration rather than calling the unlimited convenience API. Reject zero or overflowing configuration values. The request-engine plan, `2026-09-07-julie-request-engine-mcp-cli.md`, and lifecycle plan, `2026-09-07-julie-workspace-lifecycle.md`, subsequently thread the real remaining request deadline and cancellation token into this same adapter. Do not create a second independent timer that extends an existing request deadline.

The upstream options apply to both C/C++ header probes and the diagnostic walk, not just the final parser call. Cancellation is cooperative. Run parsing in a bounded blocking worker and retain its admission permit until that worker actually finishes and is joined; dropping or aborting a Tokio `spawn_blocking` waiter does not terminate parsing. Retain responsibility for cancelled workers during shutdown and check cancellation again before an edit commits. Do not promise hard preemption of a pathological native scanner. Add exact adapter tests for pre-cancelled input, expired deadline, oversized source, cancellation during diagnostics, and permit retention until a cancelled worker joins.

At the adapter boundary, map typed `SyntaxError` variants to existing tool diagnostics; match the reviewed enum, not formatted strings. `Cancelled` and `DeadlineExceeded` propagate as failed requests, never as clean syntax or plain-text fallback. Rename currently refuses any parse diagnostics. Rewrite currently refuses diagnostics touching the selected range, including zero-width diagnostics at boundaries; preserve these distinctions. Replace private `pipeline::parse_diagnostics_for_tree` with returned diagnostics. Continue canonical extraction for live symbol identity and use its original-source byte spans. Do not imply two calls parse only once.

Preserve existing unsupported text-file behavior separately from parser failures. The current rename implementation falls back to plain text only when no parser can be obtained. Do not route a parser-backed file, an oversize error, a parser failure, or `UnsupportedContainer` into that fallback. Characterize `.jsonl` before switching: canonical extraction still works; any formerly successful semantic edit is a migration prerequisite for upstream-supported syntax rather than an excuse to rename string contents.

Run exact existing guards after adaptation:

```sh
cargo nextest run -p julie-tools --lib migrated_parser_renames_identifiers_without_touching_literals
cargo nextest run -p julie-tools --lib test_ast_aware_rename_rejects_parse_error_tree
cargo nextest run -p julie-tools --lib test_smart_text_replace_unknown_language_falls_back_to_plain_text
```

Add source/control tests for each characterized embedded operation and for unsupported embedded subspans. No successful baseline operation may become a blanket safe refusal. If host-only trees cannot reproduce a previously successful operation, coordinator must extend/review the upstream syntax contract and release before this task can pass. Do not copy parser internals or hand-slice injected languages locally.

New post-edit syntax validation is assigned to [workspace lifecycle plan Task 3](2026-09-07-julie-workspace-lifecycle.md), with its own failing tests for edits that introduce parse errors. This migration preserves current pre-edit validation and transaction safety as the baseline; it does not falsely claim post-edit parsing already exists. Do not weaken current checks before the lifecycle task lands.

### Step 6: Complete language coverage and index invalidation

Record a row for every registry language and alias. Seed names are Rust, TypeScript, JavaScript, JSX, TSX, Python, Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Scala, C, C++, Go, Lua, Zig, Elixir, Erlang, GDScript, Vue, Razor, QML, qmldir, R, SQL, HTML, CSS, Regex, Bash, PowerShell, Dart, Markdown, JSON, TOML, YAML, XML, F#. Reconcile against the reviewed release, not the old “36 languages” prose; aliases and container formats are identified separately rather than inflating the language count.

For each row record canonical fixture path, detection, symbol/source projection, supported canonical fact preservation, receiver-type applicability/evidence, and edit operation applicability. All applicable rows need a fixture-derived pass; verified-not-applicable requires upstream grammar/extractor/capability evidence. No “later” category is accepted. This is consumer coverage, not duplicating all upstream grammar unit tests.

Handle all `Visibility` variants explicitly: `Public`, `Private`, `Protected`, `Internal`, `FilePrivate`, `Open`. Preserve storage strings using upstream methods; rank `Open` as externally accessible and `FilePrivate` as local, then review `Internal`/`Protected` against existing policy. Exercise serialization, DB read/write, impact ranking, and analytics queries that currently compare only `visibility = 'public'`; changes must reflect the product definition of externally accessible symbols, not mechanical string replacement.

Update `SEMANTIC_INDEX_ENGINE_VERSION` to include the reviewed extraction contract, exact release identity, extraction identity epoch, and a new consumer projection/receiver schema component. Its current literal includes `+extractors-tag=v2.34.3`; synchronize tests with the reviewed tag or explicit SHA policy. Never bump only the package pin. Verify a workspace with unchanged file hashes and an old engine identity re-extracts all derived facts, refreshes Tantivy and embeddings from the new projection, and records the new identity only after successful completion. A failed rebuild must remain visibly stale and retryable. Keep existing source data and schema migrations intact; do not ask users to delete `.julie` as the migration mechanism.

### Step 7: GREEN, coordinator review, and handoff

Run `cargo check` and each newly written exact test in its owning package. Record exact selected counts. The coordinator then runs the verification strategy once against the coherent final diff, reviews all changed expectations and source/control outputs, and verifies direct CLI dogfood indexing/search/navigation/edit preview with the debug binary. Do not run a release rebuild or require restarting the user's MCP session for ordinary verification.

Worker report must contain path, branch, HEAD, dirty state, exact owned files, reviewed upstream identity, API inventory closure, RED/GREEN evidence, all language and embedded-operation rows, and unresolved findings. The coordinator runs Miller impact after edits and resolves every actionable in-scope finding before acceptance. Save a checkpoint before any authorized commit; include `.memories/` deliberately and preserve unrelated memory work. This document's creation does not authorize committing or execution.

**Acceptance criteria:**

- [x] Reviewed release identity and minimal public syntax/relationship contracts are recorded; no moving or invented dependency target.
- [x] All six manifests and lockfile use one extractor identity and compatible tree-sitter; the whole workspace compiles at the accepted task boundary.
- [x] No private upstream module imports or compatibility extractors/managers remain in the closed inventory.
- [x] Julie-owned source projection preserves all upstream fields, exact valid source text, persisted body spans/hashes, embedding inputs, and lightweight reads; invalid ranges fail visibly.
- [x] Full canonical facts survive full indexing and watcher updates; no symbols-only downgrade or partial-success concealment.
- [x] Receiver facts survive migration and round trips and improve owner-bound resolution without false exact edges or namespace conflation.
- [x] Every current successful edit operation, including characterized embedded targets, retains behavior; parser errors never fall through to unsafe text replacement.
- [x] Every registry language/alias is covered with implemented or positively verified-not-applicable evidence; new F#/qmldir routing and container handling are exercised.
- [x] All visibility variants have reviewed persistence/ranking behavior, and stale unchanged-source workspaces rebuild via composed engine identity.
- [x] Exact worker tests and coordinator regression/dogfood gates pass with matching source identities and scope labels.
- [x] Root Codex completes final implementation review and records all fixes; worker hands off under `parallel-lead-commit` without committing.

## Verification Ledger

Fingerprint methodologies:
- **Code & Test Implementation Fingerprint** (excludes mutating ledger docs): `git diff --binary -- ':!docs/findings/*' ':!docs/plans/*' | sha256sum` -> `185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb`
- **Full Working Tree Diff Fingerprint** (all 230 tracked files): `git diff --binary | sha256sum` -> recorded in final review packet

| Invariant | Command | Package | Scope Label | Commit SHA | Implementation Diff Identity | Result / Selected Count | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|---|---|
| R1: Projected symbol keeps exact unicode source and extraction facts | `cargo nextest run -p julie-core --lib extractor_projection` | julie-core | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (7 passed) | 2026-09-08T02:08:00Z | no |
| R2: Migration 31 and schema store optional receiver_type | `cargo nextest run -p julie-core --lib receiver_type_storage` | julie-core | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (4 passed) | 2026-09-08T02:08:00Z | no |
| R2: Resolver filters candidates by receiver_type match | `cargo nextest run -p julie-pipeline --lib extractor_migration` | julie-pipeline | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (1 passed) | 2026-09-08T02:08:00Z | no |
| R2: Receiver disambiguation, inheritance hierarchy, and namespace boundary resolution | `cargo nextest run -p julie-pipeline --lib receiver_type_resolution` | julie-pipeline | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (10 passed) | 2026-09-08T02:08:00Z | no |
| R3: Syntax API adapter validates bounded execution, worker join, cancellation, and fixtures | `cargo test -p julie-tools --lib tests::extractor_migration_editing` | julie-tools | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (29 passed) | 2026-09-08T02:35:00Z | no |
| R4/R5: Semantic index engine version includes extraction contract | `cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract` | julie | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (1 passed) | 2026-09-08T02:08:00Z | no |
| R4/R5: Semantic index engine version names pinned extractors tag v2.41.0 | `cargo nextest run --lib test_semantic_index_engine_version_names_pinned_extractors_tag` | julie | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (1 passed) | 2026-09-08T02:08:00Z | no |
| BaseExtractor removal: UTF-8 string truncation utility | `cargo test --lib test_truncate_string_with_utf8` | julie | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (1 passed) | 2026-09-08T02:08:00Z | no |
| Extractor dependency integration bucket | `cargo xtask test bucket extractor-dep-integration` | xtask | extractor-dep | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (4 commands in 1.8s) | 2026-09-08T02:08:00Z | no |
| Tools editing test bucket | `cargo xtask test bucket tools-editing` | xtask | tooling | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (9 commands in 74.4s) | 2026-09-08T02:08:00Z | no |
| Dev test tier (batch regression gate) | `cargo xtask test dev` | xtask | dev | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (27 buckets in 198.6s) | 2026-09-08T02:08:00Z | no |
| System test tier (system lifecycle & integration) | `cargo xtask test system` | xtask | system | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (5 buckets in 84.1s) | 2026-09-08T02:08:00Z | no |
| Dogfood test tier (Tantivy & 100MB SQLite search quality) | `cargo xtask test dogfood` | xtask | dogfood | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (2 buckets in 447.6s) | 2026-09-08T02:08:00Z | no |
| Codebase formatting compliance | `cargo fmt --all -- --check` | workspace | format | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (0 diffs) | 2026-09-08T02:08:00Z | no |
| Workspace compilation across all targets | `cargo check --workspace --all-targets` | workspace | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (0 errors) | 2026-09-08T02:08:00Z | no |

Record upstream release identity and language/edit capability evidence in the findings ledger described above. This migration verifies preservation against the actual reviewed extractor capability registry; it does not promise a new concept for a language whose extractor does not implement it, nor use an unsupported feature probe as a passing test.
