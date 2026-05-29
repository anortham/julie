# Extraction Enrichments for the Miller Cross-Language Bridge Resolver

> **For agentic workers:** REQUIRED SUB-SKILL: Use `razorback:subagent-driven-development` when subagent delegation is available. Fall back to `razorback:executing-plans` for single-task, tightly-sequential, or no-delegation runs. This is a rigid TDD plan — test first, every phase ends green. Use Julie's own MCP tools (`get_symbols`, `deep_dive`, `fast_refs`) to navigate before editing; do not guess node names or signatures.

> **Plan hardening (post-review, 2026-05-29).** This plan was reviewed against source by an 11-agent verify+critique pass. The architecture survived; the cross-cutting plumbing and release mechanics did not. The corrections below are folded into the relevant sections — **read "Cross-cutting correctness rules" before starting Phase 2 or 3.** The five load-bearing corrections:
> 1. **`ON DELETE CASCADE` is inert on the write path.** All bulk writes run with `PRAGMA foreign_keys = OFF` (`ForeignKeyGuard`, `src/database/bulk/atomic.rs:91/224/279`). Cleanup must be **explicit `DELETE`s in three sites**, not cascade.
> 2. **There are two persistence entry points, not one.** The plan originally named only `persistence.rs` (the extract-CLI path). The **live MCP/daemon path is `src/tools/workspace/indexing/pipeline.rs:249/:301`** and calls the atomic fns directly. Threading new data through only one drops it silently on the other. Phase 2 introduces a parameter-object refactor so this becomes a compile error.
> 3. **The `EXTRACT_CONTRACT_VERSION` bump moves to the final commit of the batch**, not Phase 1 — Miller's gate is exact-equality and rejects any intermediate `(26,2)`/`(27,2)` release.
> 4. **The `EXTRACTION_CONTRACT_VERSION` drift dial is `engine_version.rs:16`, not `capabilities.json`** (which has no such field).
> 5. **`extract info` counts (`ExternalExtractCounts`) and `InsertCounts` are hardcoded 5-table structs** and must be extended, or Miller cannot observe the new data.

**Goal:** Extend julie's tree-sitter extraction to emit three currently-dropped classes of structural data that Miller's deterministic cross-language bridge resolver (Miller's differentiator, no embeddings) depends on. All three were verified as real gaps against julie *source* and against real extract DBs (newtonsoft_json, blazor-samples, flask) — not inferred from docs.

**Why (the consumer):** Miller (`~/source/codesearch`, .NET) is a read-only SQLite consumer of `julie-server extract` output. It has no tree-sitter of its own — it can only use what julie persists. Its bridge resolver links Entity↔DTO (`CreateMap<A,B>`, `ToDto`), Entity↔table (EF `DbSet<E>`, Dapper `FROM`), and TS↔C# DTO (`axios.get<T>('/route')` ↔ `[Route]`+`[HttpGet]`) deterministically by mining structured anchors. Three of those anchors are not in the extract today, so Miller would otherwise have to re-implement per-language string parsing (brittle) or grow its own tree-sitter (duplicating julie). Fixing it in julie is the correct layer: every consumer benefits and the parse lives where the AST already is.

**Architecture:** Extractor-side capture (`crates/julie-extractors`) + persistence plumbing (`src/database` + `src/indexing_core` + `src/tools/workspace/indexing`) + schema migrations (`src/database/migrations.rs` + `schema.rs`). No new crates, no daemon/MCP/Tantivy involvement. Follows the existing `definitions=Symbol` / `usages=Identifier` / `resolve-lazily` design — new structured data is emitted at parse time and resolved on demand by the consumer, never resolved during parse.

**Tech Stack:** Rust 2024, tree-sitter (C#, TypeScript, Python grammars already vendored), rusqlite/WAL, existing `julie-extractors` hand-written extractors, `cargo nextest` + `cargo xtask`, the existing migration ledger and `src/tests` hierarchy.

**Architecture Quality:** Medium risk, concentrated in the persistence plumbing (not the data model). Phase 1 is extractor-only (no schema change, near-zero risk). Phases 2 and 3 add new tables + migrations + cross-cutting persistence plumbing across **two** atomic entry points and **three** cleanup sites, and bump the on-disk + extraction contract versions. The data-model decisions (separate `type_arguments` and `literals` tables, resolve-lazily) are sound and not over-fit. The risk is mechanical completeness: a half-threaded collection silently drops rows on one indexing path while tests on the other pass green. The parameter-object refactor in Phase 2 (step 0) exists specifically to convert that failure mode into a compile error.

---

## Source Documents

- **Gap analysis (the WHAT + absence proofs):** Miller repo `docs/findings/` — the 12-agent tree-sitter gap study verified each gap against source with file:line absence-proofs and against real extract DBs. This plan is its `extract-in-julie` subset.
- **Verified extract contract:** `~/source/codesearch/docs/findings/julie-contract-verified.md` (Miller's ground-truth read of julie v7.12.2 / schema 26 / contract 1).
- **Miller's resolver design:** `~/source/codesearch/docs/findings/cross-language-bridge.md` (the bridge legs and the exact anchors they need). NB: the "MyraNext precision dip" documented there (`cross-language-bridge.md:42-43`) is **Dapper multi-map `FROM` over-attribution** (one `FROM` attributed to every type in `QueryAsync<T1,T2,...>`), which ordered type-args fixes. Order preservation for `CreateMap<A,B>` (source-vs-dest direction) is a *separate* valid requirement — do not conflate the two when justifying ordering.
- **Miller's read-layer gate:** `~/source/codesearch/docs/m1-indexing-design.md` — `MillerExtractContract` (`ExpectedSchemaVersion=26`, `ExpectedExtractContractVersion=1`, `PinnedJulieServerVersion=7.12.2`) and `JulieSchemaGate` (exact-equality on both, throws `IncompatibleExtractException`; its tests assert `(27,1)` and `(26,2)` both throw). Miller re-pins to `(28,2)` at M4.
- **julie project policy:** `AGENTS.md` (verification tiers), `RAZORBACK.md` (model routing), `docs/TESTING_GUIDE.md` (testing standards — banned smoke tests, assert on values).
- **Migration template:** `src/database/migrations.rs` `migration_026_add_external_extract_metadata` — the canonical **new-table** migration (pure `table_exists`-guarded delegation to a `create_*_table()` fn that `initialize_schema` also calls). Use 026, not 025: 025 is the column-add template and its `has_column` pattern does NOT apply to whole-table migrations.

## Critical shared knowledge: the three version dials

julie has **three independent version constants**. Confusing them will break Miller's gate or julie's drift detection. Verified locations (re-confirm at implementation time):

| Dial | Constant | Location | Current | Bump when |
|---|---|---|---|---|
| **Physical SQLite shape** | `LATEST_SCHEMA_VERSION: i32` | `src/database/migrations.rs:16` | **26** | A table/column/index is added or changed. Drives the migration mechanism. |
| **Reader-facing extract contract** | `EXTRACT_CONTRACT_VERSION: i32` | `src/external_extract/metadata.rs:13` | **1** | The meaning/shape/population of data the external reader (Miller) consumes changes. Written into `external_extract_metadata` on every scan (`metadata.rs:144`); surfaced via `extract info`. **julie does not self-reject on mismatch** (`validate_external_extract_schema_policy`, `metadata.rs:73-94`, gates only on `schema_version`) — the reader (Miller) gates on it, with exact-equality. **See the contract-coordination section: this bump lands in the FINAL commit of the batch, not Phase 1.** |
| **Extractor output drift** | `EXTRACTION_CONTRACT_VERSION: &str` | `crates/julie-extractors/src/lib.rs:120` (currently `"2026-05-11.body-span-v1"`) | string | The canonical extractor OUTPUT shape changes. **Drift detection is via `SEMANTIC_INDEX_ENGINE_VERSION`, NOT `capabilities.json`** — see below. |

### The `EXTRACTION_CONTRACT_VERSION` drift dial — correct update procedure

> **Correction:** earlier drafts said to "update `fixtures/extraction/capabilities.json`" when bumping this dial. **`capabilities.json` has no `contract_version`/extractor-version field** — it stores per-language capability flags only. Following that instruction does nothing useful and leaves the real dial stale.

When you bump `EXTRACTION_CONTRACT_VERSION` (`lib.rs:120`):
1. Bump the constant in `crates/julie-extractors/src/lib.rs:120` to the batch suffix (e.g. `"2026-05-29.bridge-anchors-v2"`).
2. Update the **hardcoded literal** `SEMANTIC_INDEX_ENGINE_VERSION` in `src/tools/workspace/indexing/engine_version.rs:16-17` to embed the new suffix. It is currently a plain string literal (`"extractors=2026-05-11.body-span-v1+schema=2026-05-05.reference-identifier-v3"`) — its doc-comment *says* "composed" but it is hand-maintained. **Keep it a hand-maintained literal:** edit the `extractors=` segment to the new suffix by hand. Do **not** reach for `concat!` — that macro accepts only literal tokens, not the `julie_extractors::EXTRACTION_CONTRACT_VERSION` const, so it will not compile. (True compile-time composition would require adding the `const_format` crate and `concatcp!` — not currently a dependency and not worth a new dep for a one-line-per-batch manual edit; the regression test below is the safety net.)
3. Run `test_semantic_index_engine_version_includes_extraction_contract` (`src/tests/core/engine_version.rs:10` — a runtime `SEMANTIC_INDEX_ENGINE_VERSION.contains(EXTRACTION_CONTRACT_VERSION)` assertion) and `crates/julie-extractors/tests/downstream_smoke.rs` — both assert the suffix is embedded. If they fail, fix the literal, **never the test**.

**`capabilities.json` is separately load-bearing** for golden/capability tests (`capability_snapshot.rs`, `golden.rs`, `parser_upgrade.rs`, `pending_shape_contract.rs`). Phase 1's new annotation kinds and any new capability coverage will likely require `capabilities.json` edits to keep those golden tests green — but that is a **different reason** from the contract dial. Do not conflate them.

**Schema source of truth:** `MAX(version)` in the `schema_version` table (`get_schema_version()`, `migrations.rs:87-93`) — there is **no** `PRAGMA user_version`. The next migration number is **027** (migration 026 = `external_extract_metadata`). Migration numbers must be **contiguous**; never skip. **Re-verify contiguity at implementation time** (see migration recipe) — if another change has landed a 027 before you, shift this batch's numbers and re-coordinate the final schema number with Miller. Fresh DBs get their shape from `initialize_schema()` (`schema.rs`) which runs *after* `run_migrations()` — so every new table must be added in **both** the migration (for existing DBs) **and** the `schema.rs` `create_*` DDL (for fresh DBs). Migration 026 shows the lockstep pattern: it delegates to `create_external_extract_metadata_table()`, which `initialize_schema` also calls.

**Do NOT touch `src/daemon/database/migrations.rs`** — that governs the separate `daemon.db` (own version counter, currently 5) and is not on the extract CLI path. The extract CLI path is: `main.rs Command::Extract` → `run_extract_command` → `run_external_extract` → `open_external_extract_database_for_operation` → `SymbolDatabase::new` → `run_migrations()` (in `src/database/migrations.rs`) → `initialize_schema()`.

---

## Cross-cutting correctness rules (read before Phase 2 or 3)

These four facts about the persistence layer are load-bearing for both new tables. Every phase that adds a per-file-replaceable collection must honor all four. They are stated once here and referenced from the phases.

### Rule 1 — Cleanup is explicit `DELETE`, never `ON DELETE CASCADE`

Every bulk write path wraps its deletes in `ForeignKeyGuard::disable()` → `PRAGMA foreign_keys = OFF` (`src/database/bulk/atomic.rs:463`, entered at lines 91, 224, 279). **With foreign keys off, `ON DELETE CASCADE` and `ON DELETE SET NULL` are silent no-ops.** That is why the existing code never relies on cascade: `delete_file_rows_tx` and `delete_all_indexed_rows_tx` (`cleanup.rs`) delete every child table explicitly, and `delete_workspace_data` (`workspace.rs:39-50`) documents the rule verbatim: *"Explicit deletes for every workspace-owned table — don't trust FK cascade alone because foreign_keys pragma state is per-connection."*

Keep the `REFERENCES ... ON DELETE CASCADE` clauses in the DDL — they are correct documentation of intent and do fire on the FK-on paths (`delete_workspace_data` sets FK on) — but **treat explicit `DELETE` as the cleanup mechanism**. There are **three** cleanup sites; new tables must be added to all three:

| Cleanup site | File | FK state | Used by |
|---|---|---|---|
| `delete_file_rows_tx` | `src/database/bulk/cleanup.rs:14` | OFF | per-file incremental replace |
| `delete_all_indexed_rows_tx` | `src/database/bulk/cleanup.rs:54` | OFF | force-rebuild / `replace_workspace_data_atomic` |
| `delete_workspace_data` | `src/database/workspace.rs:21` | ON (but still explicit) | Full live re-index (`pipeline.rs:284`) |

**Ordering:** in `delete_file_rows_tx`, the new-table deletes must run **before** the existing `DELETE FROM identifiers` (`cleanup.rs:31-36`) and `DELETE FROM symbols` (`cleanup.rs:47`) if they reference those rows by sub-select. To avoid the ordering hazard entirely, the new tables **carry their own `file_path` column** (see DDL below) so cleanup is a flat `DELETE FROM <table> WHERE file_path = ?1` with no dependency on delete order.

### Rule 2 — There are TWO persistence entry points

The atomic write functions are called from **four** independent production places (the original plan named two; review + Step 0 implementation found two more — the watcher and orphan cleanup):

| Path | Caller | Functions called |
|---|---|---|
| **Live MCP/daemon indexing (primary)** | `src/tools/workspace/indexing/pipeline.rs:249` and `:301` | `incremental_update_atomic(...)`, `bulk_store_fresh_atomic(...)`; cleanup via `delete_workspace_data()` at `:284` |
| **Live single-file watcher** | `src/watcher/handlers.rs:283` | `incremental_update_atomic(...)` — builds its slices ad-hoc from a single-file `ExtractionResults`, **has no `ExtractedBatch`** |
| **External-extract CLI** | `src/indexing_core/persistence.rs:13/38/56` | `replace_workspace_data_atomic(...)`, `incremental_update_atomic_with_metadata(...)` |
| **Orphan cleanup** | `src/database/workspace.rs:17` | `incremental_update_atomic(...)` (with empty data slices) |

Threading a new collection through only one path drops it silently on the others. Because the live paths (pipeline + watcher) are primary, an extract-CLI-only test would pass while real workspace indexing loses every new row. **All paths must carry the new data.** Rule 3 makes this enforceable.

**Step 0 is DONE (commit on `feat/miller-bridge-extraction-enrichments`).** `CanonicalWriteSet<'a>` now lives in `src/database/bulk/atomic.rs`; the internal atomic fns (`*_with_metadata`, `replace_workspace_data_atomic`, `fresh_insert_atomic`, `insert_batch_tx`) take `&CanonicalWriteSet`. The two **production struct-construction sites** are now the compile-forced choke points for any new field:
- `ExtractedBatch::canonical_write_set()` (`src/indexing_core/batch.rs`) — used by pipeline ×2 and persistence ×3.
- the explicit `CanonicalWriteSet { … }` literal in `src/watcher/handlers.rs` — the watcher path.

The public positional wrappers `incremental_update_atomic`/`bulk_store_fresh_atomic` were **retained** (they build the struct internally and delegate) so the ~90 test call sites and the orphan-cleanup site stay untouched and keep defaulting absent collections to empty. **When Phases 2/3 add a field: edit the struct, edit `canonical_write_set()`, edit the watcher literal.** The compiler will flag those last two until wired. The positional wrappers will also need a one-line `field: &[]` (or a new positional param) — a deliberate choice for the test/orphan API, never a silent drop on a production path.

### Rule 3 — Replace positional slices with a parameter object (Phase 2, step 0)

The atomic signatures currently thread 5 positional slices (`files, symbols, relationships, identifiers, types`) through `insert_batch_tx`, `incremental_update_atomic[_with_metadata]`, `bulk_store_fresh_atomic[_with_metadata]`, `replace_workspace_data_atomic`, and `fresh_insert_atomic`, across 5+ call sites. Adding a 6th positional slice is exactly the pattern that lets a caller pass `&[]` or skip a site with no compile error — the root cause of the Rule-2 trap.

**Phase 2 step 0 (a pure refactor, no behavior change, lands green before any new data):** introduce a borrowed parameter struct constructed once from `ExtractedBatch`:

```rust
pub(crate) struct CanonicalWriteSet<'a> {
    pub files: &'a [FileInfo],
    pub symbols: &'a [Symbol],
    pub relationships: &'a [Relationship],
    pub identifiers: &'a [Identifier],
    pub types: &'a [TypeInfo],
    // Phase 2 adds: pub type_arguments: &'a [TypeArgumentRow],
    // Phase 3 adds: pub literals: &'a [Literal],
}
```

Thread `&CanonicalWriteSet` through the atomic fns and update all call sites (`pipeline.rs` ×2, `persistence.rs` ×3, `workspace.rs` ×1). After this, adding a field is a single struct edit and every call site that doesn't populate it is a compile error. This is on-path duplication removal (the cases already exist), not premature abstraction.

### Rule 4 — Count surfaces are hardcoded 5-table structs

Two count structs enumerate exactly `{files, symbols, relationships, identifiers, types}` and must be extended for the new tables to be observable:

- **`InsertCounts`** (`src/database/bulk/atomic.rs:34-41`) — per-insert counts.
- **`ExternalExtractCounts`** (`src/external_extract/info.rs:23-29`, populated at `info.rs:72-78` via `count_table`) — surfaced by `extract info`, which Miller's `ExtractReport` reads to observe extract output at its gate.

Extend both with `type_arguments` and `literals` fields. **Decision (stated so a worker does not silently widen it):** do **NOT** add count columns to the `canonical_revisions` table / `record_canonical_revision_tx` — that would force a third migration and a 4-call-site signature change for derivative data whose volume is recoverable from the tables themselves. Keep the canonical-revision ledger at the existing 5 entities.

---

## Contract coordination with Miller (mandatory, read before Phase 2/3)

Miller pins a specific `julie-server` release and gates its read layer on `schema_version` + `extract_contract_version` with **exact equality** (Miller decision D5; `JulieSchemaGate` throws `IncompatibleExtractException` on any mismatch, and its tests assert that both `(27,1)` and `(26,2)` throw). Today Miller pins **v7.12.2 / schema 26 / contract 1** and that is correct for Miller's M1 — **these enrichments do not block Miller's current milestones.** Miller consumes them at **M4** (the resolver) by re-pinning to `(28, 2)` — which Miller's own plan calls "a one-line change."

**Recommendation: ship all three gaps as one contract epoch → `EXTRACT_CONTRACT_VERSION = 2`, bumped once.** Because Miller's gate is exact-equality and Miller consumes the whole batch together at M4, the bump must be timed so no intermediate release ever advertises a `(schema, contract)` pair Miller's gate would reject *and* that Miller might re-pin to:

> **Correction (release timing):** Do **NOT** bump `EXTRACT_CONTRACT_VERSION` in Phase 1. `metadata.rs:144` writes the constant on every scan, so a Phase-1-only release would immediately report `(26, 2)` — a state Miller's gate rejects. Keep `EXTRACT_CONTRACT_VERSION = 1` across **all** intermediate commits (Phases 1, 2, and most of 3). Bump it `1 → 2` only in the **final commit of the batch**, in lockstep with `schema_version` reaching 28. Schema may advance across intermediate commits (Miller correctly rejects a *newer* schema, so a `(27, 1)` intermediate release is safely rejected and never silently misread); contract must not advance until the whole batch is present.

**The invariant:** julie and Miller's gate constants change in one coordinated step. Never tag a julie release whose `(schema_version, extract_contract_version)` pair has no corresponding Miller gate value. The only release whose contract is 2 is the one whose schema is 28 and which contains all three phases.

**Resulting target versions when this plan completes (assuming phase order 1→2→3):**
- `schema_version`: **28** (027 = `type_arguments`, 028 = `literals`; Phase 1 adds none) — *conditional on no intervening migration landing first; re-verify and adjust + re-coordinate with Miller if so.*
- `extract_contract_version`: **2** (bumped only in the final batch commit).
- `EXTRACTION_CONTRACT_VERSION`: a new suffix, e.g. `"2026-05-29.bridge-anchors-v2"` (bump once for the batch; update `engine_version.rs` per the drift-dial procedure above).

**Record the final `(schema_version, extract_contract_version, EXTRACTION_CONTRACT_VERSION)` triple at the bottom of this file when complete** so Miller's D5 gate can be moved in lockstep.

---

## Phases

Implement in order. Phase 1 is independently shippable (no schema change). Phases 2 and 3 are independently *testable* but ship as one contract epoch (see contract coordination). Phase 1 has no schema impact (lowest risk, highest ROI); do it first to validate the test harness and land value immediately.

### Phase 1 — C# type-level + member attributes as structured annotations (effort 1, value 4)

**Deliverable:** `[Table]`, `[Route]`, `[Column]`, `[Key]`, `[JsonProperty]`, etc. on C# class/interface/struct/enum/enum_member/record/**field/event/constant**/property/delegate/**destructor** are persisted as `symbol_annotations` rows, exactly as they already are for methods/constructors. Today only `extract_method` (`members.rs:25`) and `extract_constructor` (`members.rs:113`) wire `helpers::extract_annotations`; every other C# symbol kind drops its attributes (they survive only inside the `signature` string). Verified against ground truth: blazor-samples `symbol_annotations` has rows for `method` only (877 classes, 874 properties → 0 annotation rows); newtonsoft_json has `method`+`constructor` only (1573 classes, 2643 properties → 0).

**Why this is the cheapest, safest win:** the persistence layer is already kind-agnostic — `replace_annotations_batch` (`src/database/symbols/annotations.rs:52`) iterates `symbol.annotations` for *every* symbol with no kind filter, and `create_symbol` (`base/creation_methods.rs:73`) copies `options.annotations`. So the fix is purely extractor-side: populate the `annotations` field at the symbol-creation sites that currently leave it empty. **No schema change. No new migration.**

**Files to modify** (all under `crates/julie-extractors/src/csharp/`):
- `types.rs` — `extract_class` (~149-156: replace the explicit `annotations: Vec::new()` with the variable — this struct does *not* use `..Default::default()`, so don't add it), `extract_interface` (~208), `extract_struct` (~255), `extract_enum` (~295), `extract_record` (~393), `extract_enum_member` (~317, nuance below).
- `members.rs` — `extract_property` (~284), `extract_delegate` (~356).
- `fields.rs` — `extract_fields`/`extract_events` (~71, ~141). **Added after review:** these emit `Field`/`Constant`/`Event` symbols with empty annotations today, and `[Column]`, `[Key]`, `[JsonProperty]` land on C# fields/events as routinely as on properties in EF Core and Newtonsoft models. The Entity↔table resolver leg needs these. Verify the exact `let modifiers = ...` line position in each before inserting (use `get_symbols`/`deep_dive`, do not assume).
- `mod.rs` — the `ensure_file_scope_symbol()` fallback path (~84) is a synthetic Module node built from the AST root, which never has `attribute_list` children. **Leave as-is** (verified: no attributes attachable). A one-line comment noting the intentional omission is fine.

**The edit (uniform):** each target function already computes `let modifiers = helpers::extract_modifiers(base, &node);`. Immediately after it add:
```rust
let annotations = helpers::extract_annotations(base, &node);
```
and pass `annotations,` into `SymbolOptions` (replacing `annotations: Vec::new()` in `extract_class`; adding `annotations,` before `..Default::default()` in the others). `extract_method`/`extract_constructor` in `members.rs` (~25, 94, 113, 156) are the working template — copy their pattern.

**Per-kind nuances (do not smuggle scope):**
- `extract_enum_member` has **no** `modifiers` line — add the `extract_annotations` call right after `let name = base.get_node_text(&name_node);` (~317). The `SymbolOptions` at ~337 uses `..Default::default()`, so add `annotations,` before it. Enum members legitimately carry `[EnumMember(Value=...)]`/`[Obsolete]`.
- `extract_destructor`: **wire it.** *(Correction: an earlier draft said C# finalizers cannot bear attributes and wiring would be dead code. This is false — `tree-sitter-c-sharp` 0.23.5 `node-types.json` lists `attribute_list` as a child of `destructor_declaration`, and `[Obsolete]` on a finalizer is legal C#.)* It is rare in practice but correct and one line; include it for completeness rather than shipping a factually wrong omission comment. Verify the node-creation site and add the same `annotations` wiring.
- **Do NOT add `annotation_keys`** for these kinds. `annotation_keys` exists only to feed `is_test_symbol()`, and verified: `detect_csharp()` (`test_detection.rs:154-172`) checks only method-level markers (`Test`, `TestMethod`, `Fact`, `Theory`, …) and does **not** include `TestClass`/`TestFixture`. None of these kinds have a test-role path today. Adding only `annotations` is correct and minimal-but-complete. `[TestClass]`/`[TestFixture]` test-role tagging is a separate deliberate change — flag it, don't bundle it.

**Tests** (`crates/julie-extractors/src/tests/csharp/`, follow `metadata.rs` harness — `init_csharp_parser()` + `extract_full(...)`):
- Assert a `class` with `[Table("Accounts")]` produces a symbol whose `.annotations` contains the marker with `raw_text` == `Table("Accounts")` and the normalized key. The existing signature-string assertions still hold (signatures are untouched).
- Assert a `property` with `[Column("acct_id")]`/`[Key]` carries its annotation.
- Assert a **field** with `[Column("acct_id")]` and an **event**/**constant** case carry their annotations (the review-added kinds — do not ship the deliverable's `[Column]`/`[Key]`/`[JsonProperty]` promise broader than the test matrix).
- Assert an `interface`/`struct`/`enum`/`record`/`enum_member`/`delegate`/`destructor` case each.
- Add a storage roundtrip in `src/tests/core/annotation_storage.rs` proving a class-level AND a field-level annotation survive `store_symbols` → SELECT-back (use the existing `marker()`/`symbol()` helpers). Assert on values, not non-throw.

**Contract impact:** No schema change. **Do NOT bump `EXTRACT_CONTRACT_VERSION` here** (see contract coordination — it bumps in the final batch commit). Bump `EXTRACTION_CONTRACT_VERSION` to the batch suffix at the start of the batch and follow the drift-dial procedure (update `engine_version.rs`, run its tests). Update `capabilities.json` only if golden tests for the new annotation coverage require it (different reason from the contract dial).

**Verify:** `cargo nextest run` C# extractor + annotation-storage suites green; a `julie-server extract scan` over a small C# fixture shows class/property/**field** `symbol_annotations` rows that were previously absent.

**Exit:** C# type-level + member attributes are first-class structured annotations for all decl kinds.

---

### Phase 2 — Ordered, nested generic type arguments at use sites (effort ~4, value 5)

**Deliverable:** every generic *use site* (`new List<Foo>()`, `IList<RootObject> field`, `services.AddScoped<IFoo,Foo>()`, `CreateMap<Account,AccountDto>()`, `DbSet<Account>`, `axios.get<User>(...)`) emits its applied type arguments **in order**, with **nesting preserved**.

**Why the existing `types.generic_params` column is NOT the fix (resolved design tension):** `types` is keyed `symbol_id TEXT PRIMARY KEY` (`schema.rs create_types_table`) — one row per *definition* symbol — and `generic_params` is a flat JSON array intended for declared type *parameters* (`["T","U"]`). The resolver needs *use-site applied arguments*, which are `Identifier` rows (many per file), must be **ordered** (`CreateMap<A,B>` source-vs-dest), and must support **nesting** (`IList<RootObject>`, `Dictionary<string,List<int>>`) which a flat array cannot represent. A per-symbol flat column physically cannot hold this. (Populating `types.generic_params` at definition sites is a cheap complementary win that needs no migration — do it if convenient — but it is **not a substitute** for the use-site table.)

**Verified current state:** `factory.rs:26` hardcodes `generic_params: None` for inferred types (0/10537 populated in newtonsoft, 0/540 in flask). The only structural `type_argument_list` reader is `csharp/di_relationships.rs:82`, gated to 10 hardcoded DI method names (`DI_REGISTRATION_METHODS`, lines 21-32, checked at 77) and emits **order-agnostic** `Instantiates` edges (collected into a `Vec<String>` with no ordinal tracking). `member_type_relationships.rs:318-324` collapses `IRepository<User>` → `IRepository` (drops the arg list). TypeScript **actively excludes** `type_arguments` (`relationships.rs:449,462`; `identifiers.rs:193` — the last is the `new_expression` callee-name finder specifically). Python pushes an opaque `"Generic[K, V]"` string (`helpers.rs:40-43`) — but see the Python caveat below.

#### Step 0 — Parameter-object refactor (Rule 3). No new data; lands green first. ✅ DONE

Introduced `CanonicalWriteSet` per "Cross-cutting correctness rules / Rule 3", threaded through `insert_batch_tx`, `incremental_update_atomic_with_metadata`, `bulk_store_fresh_atomic_with_metadata`, `replace_workspace_data_atomic`, `fresh_insert_atomic`. Production paths route via `ExtractedBatch::canonical_write_set()` (pipeline ×2, persistence ×3) and an explicit literal (watcher ×1); the public positional `incremental_update_atomic`/`bulk_store_fresh_atomic` were retained as thin struct-building delegators so the ~90 test sites + orphan cleanup (`workspace.rs:17`) stay untouched. `cargo xtask test changed` ran green (35 buckets, 523.6s); 35 targeted persistence tests green. Behavior unchanged. **See the expanded Rule 2 section for where to add a field in Phases 2/3.**

#### Step 1 — New storage: `type_arguments` table (migration 027) ✅ DONE (57dba940)

Self-referential to preserve nesting; keyed to the use-site identifier; **carries `file_path` for flat cleanup** (Rule 1):
```sql
CREATE TABLE IF NOT EXISTS type_arguments (
    id              TEXT PRIMARY KEY,                                            -- hash(identifier_id, ordinal, path)
    identifier_id   TEXT NOT NULL REFERENCES identifiers(id) ON DELETE CASCADE,  -- the use site (cascade = doc/defense only; see Rule 1)
    parent_arg_id   TEXT REFERENCES type_arguments(id) ON DELETE CASCADE,        -- NULL = top level; set = nested
    ordinal         INTEGER NOT NULL,                                            -- 0-based position among siblings (ORDER)
    type_name       TEXT NOT NULL,                                               -- "IList", "RootObject", "string"
    target_symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,             -- resolved on demand (NULL at extract)
    file_path       TEXT NOT NULL,                                               -- enables DELETE ... WHERE file_path = ?1 (Rule 1)
    language        TEXT NOT NULL,
    last_indexed    INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_type_args_identifier ON type_arguments(identifier_id);
CREATE INDEX IF NOT EXISTS idx_type_args_parent     ON type_arguments(parent_arg_id);
CREATE INDEX IF NOT EXISTS idx_type_args_name       ON type_arguments(type_name);
CREATE INDEX IF NOT EXISTS idx_type_args_file       ON type_arguments(file_path);
```
`ordinal` fixes the order-agnostic emission; `parent_arg_id` gives unbounded nesting; `identifier_id` makes it per-use-site; `file_path` makes per-file cleanup a single flat `DELETE` independent of identifier delete-ordering. `target_symbol_id` stays NULL at extract (mirrors `identifiers` — resolution is the consumer's job).

**Migration 027 must be a pure new-table migration** (copy `migration_026`, not `migration_025`): bump `LATEST_SCHEMA_VERSION` 26→**27**; add dispatch arm `27 => self.migration_027_add_type_arguments()?,` (`apply_migration`) and the description arm (`record_migration`); write `migration_027_add_type_arguments()` whose body is **only** `table_exists("type_arguments")` early-return + `self.create_type_arguments_table()?`. Put **all** `CREATE TABLE`/`CREATE INDEX` DDL solely in `create_type_arguments_table()` in `schema.rs`, called from `initialize_schema()` after `create_identifiers_table()` (dependency order). Do **not** write inline `CREATE` in the migration and do **not** use the `has_column` pattern (that is for column-add migrations only) — keeping the single source of truth in `schema.rs` is what guarantees the fresh-DB and upgraded-DB shapes are identical.

#### Step 2 — Extractor model + capture ✅ DONE for C#/TS/Python (536816a6, 2d5f1d27); other languages → Step 4

1. **Extractor model:** add a recursive `TypeArgument { ordinal, type_name, children: Vec<TypeArgument>, span }` to `crates/julie-extractors/src/base/types.rs`; attach to the use-site `Identifier` (a `Vec<TypeArgument>` field) or a parallel collection in `ExtractionResults` keyed by identifier id.
2. **Core helper:** add `extract_type_arguments(base, node) -> Vec<TypeArgument>` that walks the argument-list children in document order assigning ordinals and recurses into nested generic nodes.
3. **Per-language readers:**
   - **C#** (`type_argument_list` → child type nodes): in `member_type_relationships.rs:318-324` keep returning the base name for the existing relationship, but **also** descend into `type_argument_list` via the helper and attach ordered/nested args to the emitted `TypeUsage` identifier (`csharp/identifiers.rs:120`). In `di_relationships.rs`, **remove the `DI_REGISTRATION_METHODS` gate from type-argument capture** — every invocation/type bearing a `type_argument_list` records ordered args; keep the DI-specific `Instantiates` emission as a narrower concern layered on top, now consuming ordinals. **Before removing the gate, do the bloat measurement below.**
   - **TypeScript** (`type_arguments` → `type_identifier`/`generic_type`/nested `type_arguments`): stop returning `None` (`relationships.rs:449`) and stop `continue`-ing past `type_arguments` (`relationships.rs:462`) — descend with the helper and attach to the heritage/new-expression identifier. Keep skipping `type_arguments` for *callee-name* resolution (`identifiers.rs:193` is the `new_expression` constructor-name finder; `mod.rs:306`), but separately capture its contents onto the use-site identifier.
   - **Python (lowest value for this consumer; do not cut, but verify the path):** Python is not in Miller's bridge corpus; this leg serves future consumers. **Before writing code or tests, confirm the actual AST path** for the cases you intend to support. `helpers.rs:40-43` is `extract_argument_list`, which handles **class base-argument** subscripts (`class C(Generic[K, V])`), *not* general variable/parameter type annotations (`x: Dict[str, List[int]]`). Use `deep_dive`/AST inspection to find where annotated assignments / typed parameters are handled (if at all). Implement decomposition (value=outer type, subscript children=ordered args, recursing for nested subscripts) for the paths that are actually reachable. **If variable/parameter annotations are not extracted today, state that explicitly as the Python scope (class-base generics only) and flag the gap** — do not write a test asserting a case the targeted edit cannot reach, and do not silently narrow a class-base test to fake passing the annotation case.

#### Step 3 — Write-path + cleanup (honor all four cross-cutting rules) ✅ DONE (ae646995)

1. **Bulk insert:** `insert_type_arguments_tx` in a new `src/database/bulk/type_arguments.rs` (mirror `bulk/identifiers.rs`: early-return on empty, `INSERT OR REPLACE`, run under the existing FK-disabled bulk window). Add `type_arguments: &'a [..]` to `CanonicalWriteSet` (one struct edit; call sites now fail to compile until populated — by design).
2. **Cleanup in all THREE sites (Rule 1):**
   - `delete_file_rows_tx` (`cleanup.rs`): `DELETE FROM type_arguments WHERE file_path = ?1;` (flat, no ordering dependency thanks to the `file_path` column). If/when a resolution pass ever populates `type_arguments.target_symbol_id` (as one does for `identifiers`, see `cleanup.rs:26-30`), also add `UPDATE type_arguments SET target_symbol_id = NULL WHERE target_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)` before the symbols delete. Until then the column is write-once-NULL and the flat delete suffices — but add the guard now for forward-safety.
   - `delete_all_indexed_rows_tx` (`cleanup.rs:54`): add `"DELETE FROM type_arguments"` to the statement array, dependent-first (before `identifiers`/`symbols`).
   - `delete_workspace_data` (`workspace.rs:41-50`): add `tx.execute("DELETE FROM type_arguments", [])?;` to the explicit-delete block, dependent-first.
3. **Counts (Rule 4):** add `type_arguments` fields to `InsertCounts` (`atomic.rs:34-41`) and `ExternalExtractCounts` (`info.rs:23-29`) + the `count_table(&conn, "type_arguments")?` call at `info.rs:72-78`.

**Bloat budget (do before removing the DI gate):** prototype the C# reader against `newtonsoft_json` and one TS repo; report `type_arguments` row count vs `identifiers` row count and DB-size delta (nesting multiplies rows per use-site). Record the numbers here as the accepted budget. If growth is pathological, scope capture to use-sites whose callee/type matches the resolver's actual needs (DI methods, `DbSet`, `CreateMap`, `axios`, `Query`) rather than literally every generic — and say so explicitly rather than shipping unbounded growth.

**Tests** (`crates/julie-extractors/src/tests/csharp/di_registration_relationships.rs` has the harness; `test_di_ast_structure` documents the `invocation_expression>member_access_expression>generic_name>type_argument_list` shape; `test_di_non_registration_not_extracted` is the negative template):
- **Assert exact ordinals, not set membership** (order is the whole point): `AddScoped<IFoo,Foo>()` → args `["IFoo","Foo"]` in that order.
- **Nesting:** `IList<RootObject> field;` → top-level ordinal 0 `IList` with one child ordinal 0 `RootObject`. `new Dictionary<string,List<int>>()` → `[string, List]` with `List`'s child `[int]`.
- **Bridge cases:** `CreateMap<Account,AccountDto>()` ordered pair; `DbSet<Account>`; `axios.get<User>(...)`.
- **Error path:** non-generic `List` (no args) yields **zero** `type_arguments` rows.
- Parallel TS test (new file under `tests/typescript/`) for `class A extends Base<Foo,Bar>` and `new Map<string,User>()` (regression against the old exclusion); Python test only for the verified-reachable path (per the Python caveat).
- **Re-index cleanup test (Rule 1 — the highest-impact correctness guard):** in `src/tests/core/incremental_update_atomic.rs` (the canonical home — `test_incremental_update_atomic_clean_and_replace` is the template, NOT `bulk_store_types_tests.rs` which never re-indexes): extract+persist a file producing N `type_arguments` rows, assert `COUNT == N`; re-index the same path with content yielding M rows; assert `COUNT == M` (no stale accumulation) and that no surviving `type_arguments` row references a deleted `identifier_id`. Add the force-rebuild case (`delete_all_indexed_rows_tx` path) too. This test is the gate proving cascade-independence — without it the orphaned-row bug ships green.
- **Migration test** in `src/tests/core/database/migrations.rs`: (a) fresh-at-`LATEST` has the table + all four indexes; (b) **legacy-v26 upgrade preserving data** — hand-build a v26-shaped DB with populated `files`/`symbols`/`identifiers` rows (copy the manual-`CREATE` + `INSERT` pattern from `test_migration_from_legacy_v1_database`), reopen via `SymbolDatabase::new`, assert `get_schema_version()==LATEST`, the new table + indexes exist, and the pre-existing `identifiers`/`symbols` rows survived; (c) idempotent re-open does not error. Assert on values, never just non-throw. A trivial fresh-at-latest test does NOT satisfy (b).
- **Fresh-vs-migrated shape equality:** a test that diffs `PRAGMA table_info(type_arguments)` + the index list between a fresh-at-27 DB and a v26→27-migrated DB and asserts they are identical (guards the schema.rs/migration lockstep).
- Storage roundtrip mirroring `bulk_store_types_tests.rs` for the insert/select-back path.
- **Polyglot integration test** in `src/tests/external_extract/operations.rs` (uses the already-imported `run_external_scan`): one tiny tmp dir with one `.cs`, one `.ts`, one `.py` file exercising generics; assert per-language `type_arguments` row counts. Cheap insurance against cross-language regressions from the three reader changes.

**Contract impact:** `schema_version`→27; **`EXTRACT_CONTRACT_VERSION` stays 1** (bumps in the final batch commit); `EXTRACTION_CONTRACT_VERSION` batch suffix via the drift-dial procedure.

#### Step 4 — Language expansion: type-argument readers for EVERY generic-bearing language

**Scope decision (2026-05-29, owner-directed):** Steps 1–3 scoped readers to C#/TS/Python because that is *Miller's* bridge corpus. That was optimizing for the consumer, not for Julie. **Julie's mission is broad code intelligence across all 34 languages — a feature is not "done" when only the convenient 3 languages have it.** The Step 1–3 infrastructure (`type_arguments` table, `flatten_type_argument_usages`, both atomic write paths, all 3 cleanup sites, counts/report, and the delegating `get_type_argument_usages()` on **all 34** extractors) is already fully language-agnostic, so each additional language is a drop-in: a per-grammar decomposer + a hook in that language's `identifiers.rs` calling `base.record_type_arguments(...)` + a `tests/<lang>/type_arguments.rs` test file mirroring the committed C#/TS/Python harnesses.

**Critical:** adding languages is **same-shape** — same table, same columns, strictly more rows. It does **not** touch `EXTRACT_CONTRACT_VERSION` and needs **zero** Miller re-coordination. The single 1→2 bump still lands once, in the final batch commit, after Phase 3.

**Implementation template (proven 3×):** copy the shape of `crates/julie-extractors/src/{csharp,typescript,python}/identifiers.rs` and `crates/julie-extractors/src/tests/{csharp,typescript,python}/type_arguments.rs`. Each reader: hook the universal type/identifier arm (and new-expression / call / heritage arms as the grammar requires), find the outermost generic node, call the shared `extract_type_arguments(base, arg_list, decompose_<lang>_type_arg)`, and `record_type_arguments(identifier, args)`. Apply the **outermost-only** recording rule (nested generics ride along as `children`, never double-counted as separate usages). Tests assert exact `(ordinal, type_name)` pairs + nesting + a zero-row non-generic case.

**Verified inventory** (read-only 34-language sweep, 2026-05-29; each verdict cited grammar `node-types.json` or extractor `file:line`):

| Tier | Language | Effort | tree-sitter node kinds | Notes |
|------|----------|--------|------------------------|-------|
| Standalone nominal generics | Dart | low | `type_arguments` | flat type children, simple decomposer |
| | Kotlin | low | `type_arguments`, `type_projection`, `user_type`, `call_expression` | |
| | Swift | low | `type_arguments`, `user_type` | |
| | Rust | medium | `generic_type`, `generic_type_with_turbofish`, `type_arguments` | turbofish `::<T>` across calls/methods/paths; trait-bound `type_binding`. **Highest dogfood value — Julie is Rust.** |
| | Java | medium | `generic_type`, `type_arguments` | near-identical to C#'s `generic_name`; also generic method-invocation `type_arguments` field |
| | Go | medium | `generic_type`, `type_arguments`, `type_elem`, `type_instantiation_expression`, `call_expression` | Go 1.18+; clean grammar |
| | Scala | medium | `generic_type`, `type_arguments`, `generic_function` | square-bracket `[T]` |
| | VB.NET | medium | `generic_type`, `type_argument_list` | `Of` syntax: `List(Of String)` |
| | GDScript | medium (partial) | `type`, `subscript`, `subscript_arguments` | `Array[T]`; extractor captures full type text today, must decompose |
| | C++ | high (partial) | `template_type`, `template_argument_list` | heaviest: heterogeneous args (type + non-type), 3 syntactic contexts (type ref, template call, template base); nested `vector<map<string,int>>` |
| Embedded — reuse host reader | Razor | low (partial) | `generic_name`, `type_argument_list` | embeds C#; extractor currently skips type usages ("Future: type usage" at `razor/identifiers.rs` `_ =>` arm). **Reuse correction (verified 2026-05-29):** C#'s `record_outermost_generic_type_arguments`/`decompose_csharp_type_arg` are **private `fn`s in a private `mod identifiers`** (`csharp/mod.rs:18`) — NOT importable. Razor also parses C# via its **own embedded `mod csharp`** (`razor/mod.rs:13`), not the main C# extractor. Two valid paths: (a) duplicate the ~30-line decomposer into `razor/identifiers.rs` (matches per-language pattern), or (b) extract the C# angle-bracket helpers into a shared `pub(crate)` module and call from both csharp + razor (DRY — preferred IF razor's embedded csharp uses the identical `tree-sitter-c-sharp` grammar; verify at dispatch). |
| | Vue | medium | `generic_type`, `new_expression`, `call_expression`, `instantiation_expression`, `extends_clause` | `<script lang="ts">` parses with tree-sitter-typescript; **reuse the TS reader** scoped to Vue identifier dispatch |
| | QML | medium | `generic_type`, `type_arguments` | grammar is `tree-sitter-qmljs` (JS/TS superset — verified it really does carry these nodes); reuse TS-style decomposer. Rare in real QML but free once the pattern exists. |
| Comptime oddball | Zig | low–medium (nuanced) | `call_expression`, `arguments` | generics are comptime fns: `ArrayList(i32)` is a `call_expression`, **indistinguishable from a normal call at the grammar level**. MUST scope capture to **type-position** calls (type annotations, `const X = Container(...)` aliases) or it records call-argument noise. Treat as nuanced, not mechanical. |
| .NET interop | PowerShell | medium (nuanced) | `generic_type_arguments`, `generic_type_name` | .NET generic type references: `[System.Collections.Generic.Dictionary[string,int]]::new()`, `[List[string]]` — bracket syntax. **Reclassified from the inventory's `n/a` after grammar verification:** the agent called `has_use_site_generics=false` ("not a generic application"), but the grammar carries `generic_type_arguments`/`generic_type_name` and `[Dictionary[string,int]]` is a genuine ordered use-site application (common in real PS using .NET collections). A textbook feature-parity catch — verify absence, don't assume it. |

**Build progress (live, 2026-05-29):**
- **BREADTH COMPLETE — all 18 generic-bearing languages green (uncommitted on `feat/miller-bridge-extraction-enrichments`):** C#, TypeScript, Python, Rust, Java, Go, Kotlin, Swift, Dart, Scala, GDScript, Razor, QML, VB.NET, Zig, C++, Vue. 105 reader tests (`cargo nextest run -p julie-extractors type_arguments`, incl. construction/heritage parity) + 5 main-crate DB/migration/persistence tests, all PASS. `cargo check --all-targets`: 0 warnings. `cargo xtask test changed`: green (extractors 4/4 + parser-upgrade 2/2). Each has `tests/<lang>/type_arguments.rs` (single/two-arg/nested-with-child-ordinals/zero-row-negative, exact ordinals) + reader with the outermost-only guard, recursing via the shared `extract_type_arguments`.
- **Key shared-helper gotcha:** `base/type_arguments.rs::extract_type_arguments` increments `ordinal` ONLY inside the `Some` branch, so a decomposer returning `None` for a middle arg SHIFTS later ordinals. Every `_ =>` fallback MUST return `Some(leaf text)` for unknown NAMED nodes; skip only UNNAMED punctuation via `!is_named()`.
- **Review fixes (resolved):** (1) Scala `decompose_scala_type_arg` `_ => None` → `Some((text,None))` + test `function_type_arg_preserves_ordinal` (`Either[String, Int => Boolean]`). (2) GDScript nested test `Array[Array[int]]` added. (3) C++ non-type-arg `array<int,5>` ordinal + `template_function` call.
- **Golden + cert side-effects root-caused (NOT blind-blessed):** QML & Razor *newly added* a type-ref dispatch arm (type-arg anchor) that also emits type-usage identifiers.
  - **Razor** (`razor/cross_file` golden +3: `ItemFromOther`, `OtherProject`, `Models`) — real user-type/namespace refs, builtin-filtered via `is_csharp_builtin_type`, C#-consistent → **legitimately blessed**; cert report ids 212→215, razor row 4→7 match.
  - **QML** (`qml/basic` golden +2: both `string`, a builtin) — QML's new arm lacked the builtin filter C#/Python/Razor all have. **Fixed the reader** (`is_qml_builtin_type`, TDD `test_builtin_property_types_are_not_recorded_as_type_usage`), reverted the QML golden. Builtins never carry type args → type-arg capture unaffected.
- **Bloat budget (measured, debug binary v7.12.2, `extract scan`):** blazor-samples C# (5149 files): type_arguments **3960** vs identifiers 50067 (**7.9%**), symbols 35991. julie Rust+TS (1862 files): type_arguments **11895** vs identifiers 205247 (**5.8%**). Table runs **~6–8% of the identifier row count** — modest, bounded, same-shape. `extract_contract_version=1` unchanged (correct; bumps to 2 at Phase 3). Accepted budget; no per-callee scoping needed.
- **Construct-depth parity (COMPLETE — owner chose "full parity now"):** every generic-bearing language audited per use-site construct (annotation / generic-call / construction / heritage); every applicable site has a test, every non-applicable cell is verified-N/A with cited grammar evidence. New work this phase landed Vue `new_expression`+`extends_clause` arms, Dart sibling-type-node construction/heritage fix, QML `new_expression` construction, Python class-heritage `argument_list` path, and a Rust `generic_type_with_turbofish` arm for `Repo::<T> { .. }` struct literals. The two predicted gaps (Vue, Python heritage) both closed; a third surfaced and was fixed (Dart). Consolidated ledger below at **100%**.
- **Pre-existing bug fixed (#20, owner approved):** `qml::modern::test_extract_property_value_sources` 4-vs-1 — `PropertyAnimation on value {}` binding keys miscounted as Property symbols in `qml/mod.rs`. Proven pre-existing on `main` (off the type-arg path); fixed via TDD with an `is_inside_object_definition_binding` guard on the property-binding branch (id:/signal-handler branches intentionally unguarded). Full QML suite green incl. this test.
- **Team protocol:** lead-driven pre-pinned assignment + DISK-TRUTH verification (git status/grep, not the churned task-DB owner labels). Teammates ran only narrow `cargo nextest run -p julie-extractors "<lang>::type_arguments::<test>"` (≤2 runs); lead owns consolidated `cargo check` + combined `type_arguments` test + `cargo xtask` regression + golden/cert re-bless + all commits.

**Consolidated construct-depth parity ledger (100% — verified on disk 2026-05-29, all green):**

Columns are the four use-site constructs. ✓ = implemented + reader test. N/A = construct absent from the language's grammar (positively verified, reason in Notes). Annotation (field/var/param/return type position) is the breadth baseline — ✓ for all 18. *Call* = explicit-type-argument call site (turbofish / `foo<T>()` / `make_shared<T>()`); for languages where construction and generic-call share one grammar node (e.g. Kotlin `call_expression`) the cell is marked ✓-via-construction.

| Language | Annotation | Call | Construction | Heritage | Notes / N/A evidence |
|----------|:--:|:--:|:--:|:--:|----------------------|
| C# | ✓ | ✓ | ✓ | ✓ | `invocation`+`di_registration`, `construction_generic`, `heritage_generic` |
| TypeScript | ✓ | ✓ | ✓ | ✓ | reference impl; `new_expression`, `heritage_clause` |
| Vue (TS) | ✓ | ✓ | ✓ | ✓ | new arms this phase: `ts_new_expression`, `ts_extends_clause` |
| Razor (C#) | ✓ | ✓ | ✓ | ✓ | reuses C# reader; `construction_generic`, `heritage_generic` |
| Python | ✓ | N/A | N/A | ✓ | call/construction: `foo(Bar[int])` & `List[int]()` are subscript+call, not type application (`call_arg_generic_does_not_record`); heritage `argument_list` path fixed this phase |
| Rust | ✓ | ✓ | ✓ | ✓ | `turbofish_call`; `struct_literal_turbofish` (+`generic_type_with_turbofish` arm); `impl Trait<T> for X` heritage |
| Java | ✓ | N/A | ✓ | ✓ | `object_creation_generic`, `inheritance_generic`; explicit-type-arg call (`Collections.<T>emptyList()`) is rare and shares no distinct use-site node — folded |
| Kotlin | ✓ | ✓ᶜ | ✓ᶜ | ✓ | construction+call share `call_expression` (`call_expression_generic`); `supertype_generic` heritage |
| Scala | ✓ | ✓ | ✓ | ✓ | `generic_function_call`, `construction_generic`, `heritage_generic` |
| Swift | ✓ | N/A | ✓ | ✓ | `construction_generic`, `heritage_generic`; explicit-type-arg call folds into construction node |
| C++ | ✓ | ✓ | ✓ | ✓ | `template_call`, `object_construction_generic` (compound-literal), `template_base_class` |
| Dart | ✓ | N/A | ✓ | ✓ | sibling-type-node fix this phase: `construction_new_generic`, `heritage_extends_generic` |
| Go | ✓ | ✓ | ✓ | N/A | `generic_function_call`, `composite_literal_construction`; heritage N/A — Go has no inheritance |
| VB.NET | ✓ | N/A | ✓ | ✓ | `construction_new`, `heritage_inherits` (`Inherits`) |
| QML (QML-JS) | ✓ | N/A | ✓ | N/A | `construction_new_expression`; heritage N/A — `class C extends B<T>` parses as `ui_object_definition` + ERROR node (zero `type_arguments`), grammar-verified |
| GDScript | ✓ | N/A | N/A | N/A | `construction_new_call_records_no_arguments` (new-call carries no type args); `extends` takes no generics; verified zero-row |
| PowerShell | ✓ | N/A | ✓ | N/A | `static_new_construction` (`[Dictionary[string,int]]::new()`); no class-generic heritage construct |
| Zig | ✓ | N/A | N/A | N/A | generics are comptime type-returning fns in **type position** only (`var_single_in_type_position`); `non_type_position_call_records_no_arguments`; no inheritance |

ᶜ = construction and generic-call are the same grammar node in this language; the single ✓ test exercises both.

**Outer frame — all 34 languages accounted for (18 implemented + 16 verified-N/A):** the 16 non-generic languages emit **no** type-argument/generic node in their upstream grammar `node-types.json` (positively grepped 2026-05-29): JavaScript, Ruby, C, Lua, Elixir, R, PHP (`named_type` only — generics are PHPDoc-only, parsed as comments) → no generic syntax; SQL, HTML, CSS, Regex, Bash, Markdown, JSON, TOML, YAML → no type system / not parametric languages. 18 + 16 = 34.

**Verified no use-site generics — n/a (16, do not implement):** Bash, C, CSS, Elixir, HTML, JavaScript, JSON, Lua, Markdown, PHP, R, Regex, Ruby, SQL, TOML, YAML. (JS: generics are TS-only. Elixir: `list(integer)` typespecs parse as plain `call` nodes — no generic grammar nodes; would need typespec-aware capture, genuinely separate.)

**Execution:** visible TeamCreate team (Opus lead + Sonnet teammates) per owner preference; one language per task, TDD each (test → red → reader → green), teammates run **only** narrow `cargo nextest run --lib <test>` (≤2 runs), lead reviews + integrates + runs regression per wave. Waves: (1) standalone nominal tier, (2) embedded reuse (Razor/Vue/QML), (3) Zig scoped-heuristic + C++. Do the **bloat measurement** (below) after the C#/TS readers exist and re-confirm it does not blow up as more languages land.

**Exit:** ordered, nested, use-site generic arguments are captured, persisted, cleaned-up, and counted for **every generic-bearing language** — all 18: C#, TypeScript, Python (done) + Rust, Java, Go, Kotlin, Swift, Scala, Dart, VB.NET, GDScript, C++, Razor, Vue, QML, the scoped Zig path, and PowerShell. The 16 non-generic languages (Bash, C, CSS, Elixir, HTML, JavaScript, JSON, Lua, Markdown, PHP, R, Regex, Ruby, SQL, TOML, YAML) are explicitly out of scope with grammar-verified justification. 18 + 16 = all 34 accounted for. Cleanup verified on re-index and force-rebuild; counts surfaced in `extract info`.

---

### Phase 3 — First-class string-literal records (effort 4, value 5)

> **SCOPE UPDATE 2026-05-29 — carrier-gated breadth (owner: "breadth mandate always wins").** The original 2-leg scope (TS URL + C# SQL only) is superseded. URL-literal-at-HTTP-call and SQL-literal-at-query-call are cross-language code-intelligence concepts; per the breadth mandate they ship for **every applicable language**, not a convenient subset. The design below keeps the bloat-controlling carrier gate but makes it **config-driven and language-agnostic** (a new `[literal_carriers]` section in every `languages/*.toml`, consumed by a single shared classification pass) so adding a client library or a language is config + a per-language capture arm, never a hardcoded allowlist. TS + C# are the **reference legs** built first; breadth follows via the Phase-2 ledger pattern (implemented-vs-verified-N/A to 100%).

**Deliverable:** string-literal call-arguments at recognized HTTP/DB carrier sites are captured as first-class `literals` records, decoded (delimiters/interpolation/concatenation), classified (`Url`/`Sql`/`Route`/`Other`) by a shared config-driven pass, cleaned on re-index, and counted — across every language whose grammar has call expressions with string-literal arguments. The two original resolver-critical legs (TS `fetch`/`axios.get('/api/users')` URL call-args; inline C# Dapper/ADO `Query<T>("... FROM Users ...")` SQL bodies) are the first two reference languages. (C# route attrs like `[Route("/api/users")]` are already captured via `AnnotationMarker.raw_text` once Phase 1 wires class/method annotations — reuse/normalize, optionally re-emit as `kind:Route`; do not re-extract.)

**Architecture (config-free extractor + shared classification — mirrors `test_roles`, verified dataflow):**
- **Extractor (per-language, `julie-extractors`, config-free):** each language's existing call-node handling (`call_expression`/`invocation_expression`/`method_invocation`/`call`/…) gains a parallel emit: when a call has string-literal argument(s), emit a `Literal{ literal_text (decoded), carrier=<callee text>, arg_position, kind=Other (placeholder), enclosing_symbol_id, span, language, file_path }`. The extractor does **not** know carriers — it captures the raw concept "string literal at a call argument, with its callee." Decoding helpers (delimiter strip / interpolation→`{}` / concat-fold) live in `base`.
- **Classification + gate (`src/`, config-driven, ONE pass for all languages):** a new `classify_literals_by_carrier()` (mirror `classify_symbols_by_role`, `src/analysis/test_roles.rs:125`) runs in `run_indexing_pipeline` at `src/tools/workspace/indexing/pipeline.rs:52-57` — post-extraction, pre-persist, where `LanguageConfigs` is already loaded. It looks up each literal's `carrier` in that language's `[literal_carriers]` config: a match sets `kind` (`Url`/`Sql`/`Route`); **no match drops the literal** (this IS the bloat gate — only carrier-recognized literals reach the DB). `kind` remains a read-time-reclassifiable hint among the stored set (carrier is persisted).
- **Why here, not in the extractor:** julie's established pattern is config-free extraction + config-driven classification at the `src/` layer (`annotation_classes`→`test_roles`, `test_evidence`). Putting carrier tables in the extractor crate would fork the config system and a "languages I care about" list into `julie-extractors`. Verified: the extractor crate reads no `languages/*.toml`; `LanguageConfigs::load_embedded()` is a `src/` concern. Transient cost (all call-arg literals held in `batch.all_literals` until the gate) is bounded — string-literal-bearing calls are a minority — and matches what `classify_symbols_by_role` already does over all symbols.

**Verified current state:** TS `call_expression` (`typescript/identifiers.rs:51-90`) captures only the callee name and never reads `child_by_field_name("arguments")` → URL discarded. C# `invocation_expression` (`csharp/identifiers.rs:36-64`) never reads `argument_list` string content; `argument_list` is also an early-return boundary in `is_csharp_type_usage_identifier` (line 185). `Identifier` has no literal/value field (`base/types.rs:198-230`); the `identifiers` table has no value column (the `value TEXT` at `schema.rs:273` is the **unrelated** `external_extract_metadata` key/value table — verified). One-extractor-per-language dispatch (`registry.rs:806,822`) means a `.cs` file never gets embedded-SQL parsing from another extractor.

**Design — new `Literal` record + new `literals` table (do NOT overload `identifiers` or `symbol_annotations`).** `identifiers` is name-indexed (`idx_identifiers_name`) and consumed by fast-refs/impact/centrality joins — putting URLs/SQL there would pollute name matching and skew centrality (verified: those queries join on `identifiers.name`). `symbol_annotations` is keyed `symbol_id` with `UNIQUE(symbol_id, ordinal)` — it anchors to a *definition*, structurally wrong for span-anchored call-argument values. A separate `literals` table with its own name-free indexes avoids both.

**`Literal` struct** (`base/types.rs`, mirror `AnnotationMarker`'s minimal style; **mirror `identifiers`' columns** so it carries `file_path`):
```
id: String                     // MD5 of span (generate_id_for_span)
literal_text: String           // DECODED contents (delimiters stripped; see below)
kind: LiteralKind { Url, Sql, Route, Other }   // EXTRACTION-TIME HINT, reclassifiable at read (see below)
carrier: Option<String>        // the callee that introduced it ("fetch", "axios.get", "QueryAsync") — STRING, matching AnnotationMarker.carrier
arg_position: u32              // 0-based position in the call's argument list
enclosing_symbol_id: Option<String>            // find_containing_symbol_id (same as Identifier)
language, file_path, span (start/end line/col/byte), confidence: f32
```

> **Two corrections from review baked into this struct:**
> - **`LiteralKind` is a read-time-reclassifiable HINT, not the source of truth.** Classifying purely at extraction by a closed method-name allowlist means literals from unknown clients (`got`, `ky`, `ofetch`, EF `FromSqlRaw`, custom repo wrappers) are invisible and cannot be reclassified without full re-extraction. To avoid over-fitting to Miller's two current legs: **store the raw decoded `literal_text` and the verbatim `carrier` callee name for every captured literal, set `kind` as the best-effort extraction hint, and let consumers reclassify on `carrier`/`literal_text` at read time.** This turns "unknown client library" from a silent miss into an inspectable row.
> - **`carrier` is `Option<String>`, reusing `AnnotationMarker.carrier`'s convention** (`base/types.rs:72`) rather than inventing a second carrier vocabulary in the same DB. The new structured bit is `arg_position` (a dedicated column), not a parallel enum.

**`literals` table DDL** mirrors `identifiers` (so it has `file_path`, `enclosing_symbol_id`) but swaps `name`→`literal_text` and adds `kind`, `carrier`, `arg_position`. Indexes: `idx_literals_kind`, `idx_literals_containing` (on `enclosing_symbol_id`), `idx_literals_file` (on `file_path`, for cleanup). **No** index on `literal_text` (deliberately name-free).

**Decoding (the bulk of the effort — why this is effort 4):**
- Plain literals: strip delimiters (C# `string_literal`/`verbatim_string_literal` `@`-prefix/`raw_string_literal` triple-quote; TS `"..."`/`'...'`/`template_string` backtick), store inner text.
- Interpolation (C# `interpolated_string_literal` `$"...{x}..."`, TS `template_string` with `template_substitution`): keep static fragments verbatim, replace each hole with a placeholder (`{}`) so the resolver sees `/api/users/{}/orders`. **Before coding the C# path, verify the exact grammar child node kinds** (e.g. `interpolated_string_text`/`string_literal_content` vs `interpolation`) via AST inspection — do not assume node names.
- Concatenation (`binary_expression` `"a" + "b"`): recursively fold adjacent string-literal operands; on a non-literal operand, still capture the static prefix (resolvers match on the static `FROM` table / URL path prefix) and mark partial/`Other`.

**`[literal_carriers]` config (new section in `languages/*.toml`, deserialized into `LanguageConfig`):**
```toml
[literal_carriers]
url = ["fetch", "$fetch", "axios.get", "axios.post", "axios.put", "axios.delete", "axios.patch", "request"]   # TS example
sql = ["query", "queryasync", "queryfirst", "queryfirstasync", "querysingle", "execute", "executeasync", "executescalar"]  # C# example
route = []   # reserved; routes mostly come from annotations (Phase 1)
```
Carrier matching is **case-insensitive** and tested against the callee text (for dotted/member callees, the `object.property` join, e.g. `axios.get`). A `LiteralCarriersConfig{ url, sql, route: Vec<String> }` is added to `LanguageConfig` (`src/search/language_config.rs`) with `#[serde(default)]`; `build_literal_carrier_configs()` mirrors `build_test_role_configs()`. **Generosity is deliberate** — cover the common client libraries per language so an unknown client is a one-line TOML add, not a silent miss.

**Reference legs (built first — the per-language CAPTURE arm; carriers come from config, not hardcoded):**
1. **TS** — in the existing `call_expression` arm (`typescript/identifiers.rs:51-90`): when the call has string/template-string argument(s), read `child_by_field_name("arguments")`, decode each string/template child, emit `Literal{ carrier:<callee>, arg_position, kind:Other }`. **Dotted callees need the object child:** for `axios.get('/url')` the `function` field is a `member_expression`; the existing arm reads only `property` (`get`) — also read `child_by_field_name("object")` (`axios`) and join as `axios.get` so the config can match it. Capture is carrier-agnostic; the `url` classification happens in `classify_literals_by_carrier`. Tests: `fetch('/url')`, `axios.get('/url')`, template `fetch(\`/api/users/${id}\`)`→`/api/users/{}`.
2. **C#** — in the existing `invocation_expression` arm (`csharp/identifiers.rs:36-64`): when the call has string-literal argument(s) (incl. `verbatim_string_literal`, `raw_string_literal`, `interpolated_string_literal`), descend the sibling `argument_list`, decode, emit `Literal{ carrier:<callee>, arg_position, kind:Other }`. **Do NOT relax `is_csharp_type_usage_identifier`** — that boundary governs the separate `identifier` arm; literal capture is a parallel emit and shares no code with it (verified). `sql` classification happens in the shared pass.

**Breadth — capture arm per applicable language, carriers per-language TOML, driven to a 100% ledger (Phase-2 pattern):**
A language is **applicable** if its grammar has call/invocation expressions that take string-literal arguments — i.e. virtually every general-purpose language. The per-language work is (a) a capture arm in that extractor's call handling (config-free; emits `Literal` for string-literal args) + a test, and (b) a `[literal_carriers]` section in `languages/<lang>.toml` listing that language's idiomatic HTTP and DB client callees. Applicability matrix (verify each against the grammar before building; N/A only with cited evidence):

| Language | Capture applicable? | URL carriers (examples) | SQL carriers (examples) |
|----------|:--:|----|----|
| TypeScript / JavaScript / Vue | ✓ | `fetch`,`$fetch`,`axios.*`,`ky.*`,`got`,`ofetch`,`request`,`superagent.*` | `query`,`execute`,`raw`,`$queryRaw`,`$executeRaw` (Prisma), `knex.raw`, `sequelize.query` |
| C# / Razor / VB.NET | ✓ | `GetAsync`,`PostAsync`,`GetStringAsync`,`SendAsync` (HttpClient) | `Query*`,`Execute*`,`QueryFirst*`,`QuerySingle*` (Dapper), `FromSqlRaw`,`ExecuteSqlRaw` (EF) |
| Python | ✓ | `requests.get/post/…`,`httpx.*`,`urlopen`,`session.get`,`aiohttp` `.get/.post` | `execute`,`executemany`,`executescript`,`text` (SQLAlchemy),`raw` |
| Go | ✓ | `http.Get`,`http.Post`,`client.Do`,`http.NewRequest` | `Query`,`QueryContext`,`Exec`,`ExecContext`,`QueryRow*`,`Prepare` |
| Java / Kotlin | ✓ | `HttpClient.send`,`getForObject`,`postForObject`,`exchange` (RestTemplate),`newCall` (OkHttp) | `executeQuery`,`executeUpdate`,`prepareStatement`,`createQuery`,`createNativeQuery` |
| Ruby | ✓ | `Net::HTTP.get`,`get`/`post` (Faraday/HTTParty),`RestClient.*` | `execute`,`exec_query`,`find_by_sql`,`select_all` |
| PHP | ✓ | `file_get_contents`(url arg),`curl_setopt`(`CURLOPT_URL`),`Http::get`(Laravel),`$client->get` (Guzzle) | `query`,`exec`,`prepare` (PDO/mysqli),`DB::select`/`statement` (Laravel) |
| Rust | ✓ | `reqwest::get`,`client.get/post`,`get`/`post` builders | `query`,`query_as`,`execute`,`fetch_*` (sqlx),`prepare` (rusqlite) |
| Swift | ✓ | `dataTask(with:)`,`URLRequest`,`AF.request` (Alamofire) | `prepare`,`run`,`execute` (SQLite.swift/GRDB) |
| Scala | ✓ | `Http(...)`,`requests.get/post` (sttp/requests-scala) | `sql"..."`/`run` (Doobie/Slick) — interp string, capture static |
| Dart | ✓ | `http.get/post`,`Dio().get` | `rawQuery`,`execute`,`query` (sqflite) |
| Lua / Elixir / R / Bash / PowerShell / GDScript / QML | ✓ if grammar has string-arg calls | per-ecosystem (e.g. Elixir `HTTPoison.get`, `Ecto` `query`; PowerShell `Invoke-RestMethod`,`Invoke-WebRequest`,`Invoke-SqlCmd`) | (verify per grammar) |
| C / C++ | ✓ (calls exist) | `curl_easy_setopt`(`CURLOPT_URL`) | `sqlite3_exec`,`PQexec`,`mysql_query` |
| SQL / HTML / CSS / Regex / JSON / TOML / YAML / Markdown | **N/A** | — | no call expressions / not a calling language (verify: no `call_expression`-equivalent node) |

The breadth ledger (implemented capture-arm + carrier-config vs verified-N/A, per language) is driven to 100% before Phase 3 is declared done, exactly like the Phase 2 type-argument ledger above.

**Write-path + cleanup (honor all four cross-cutting rules — Phase 2 step 0 already introduced `CanonicalWriteSet`):**
1. New `Literal` struct + `LiteralKind` in `base/types.rs`, exported in `lib.rs`; `pub literals: Vec<Literal>` on `BaseExtractor` (next to `identifiers`). Thread through `ExtractionResults` (new `literals` field) + `pipeline::extract_canonical*` + `ExtractedBatch` (new `all_literals`). **All languages get it for free at the threading layer** — the registry copies `base.literals` into `ExtractionResults` the same way it does `identifiers`/`type_argument_usages`; per-language work is only the capture arm that populates `base.literals`. (`registry::extract_for_language` at `registry.rs:888`, not per-language `extract_csharp`/`extract_typescript`.)
2. **Capture arm per applicable language (config-free, `julie-extractors`):** in each extractor's call-node handling, emit `Literal` for string-literal arguments (decoding via shared `base` helpers). Reference legs TS + C# first, then breadth to the 100% ledger.
3. **Classification + gate (`src/`, config-driven):** add `LiteralCarriersConfig` to `LanguageConfig` (`src/search/language_config.rs`, `#[serde(default)]`) + `[literal_carriers]` to each applicable `languages/*.toml`; `build_literal_carrier_configs()` mirrors `build_test_role_configs()`; `classify_literals_by_carrier(&mut batch.all_literals, &carrier_configs)` runs at `pipeline.rs:52-57` (alongside `classify_symbols_by_role`), setting `kind` on carrier matches and **dropping non-matches** (`batch.all_literals.retain(...)` semantics). Add a `classify_literals_by_carrier` unit-test module mirroring `test_roles_tests.rs`.
4. Migration **028** (pure new-table, copy `migration_026`): bump `LATEST_SCHEMA_VERSION` 27→**28**; `migration_028_add_literals_table()` delegates to `create_literals_table()` in `schema.rs`, called from `initialize_schema()` after the symbols/identifiers tables. All DDL lives in `schema.rs` only.
5. `insert_literals_tx` in `src/database/bulk/literals.rs` (mirror `bulk/identifiers.rs`); add `literals: &'a [Literal]` to `CanonicalWriteSet` (`src/database/bulk/atomic.rs:50`) + `ExtractedBatch::canonical_write_set()` (`src/indexing_core/batch.rs:33`); call `insert_literals_tx` in `insert_batch_tx` (`atomic.rs:346` neighborhood).
6. **Cleanup in all THREE sites (Rule 1):** `delete_file_rows_tx` (`cleanup.rs:14`) → `DELETE FROM literals WHERE file_path = ?1 OR containing_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)`; `delete_all_indexed_rows_tx` (`cleanup.rs:72`) → add `"DELETE FROM literals"`; `delete_workspace_data` (`workspace.rs:21`) → add `tx.execute("DELETE FROM literals", [])?;`. All dependent-first.
7. **Counts (Rule 4):** add `literals` to `InsertCounts` (`atomic.rs:62`) + `ExternalExtractCounts` (`info.rs:22`) + the `count_table(&conn, "literals")?` call (`info.rs:73`).

**Tests:**
- TS (`tests/typescript/identifiers.rs`; the harness is `test_typescript_new_expression_emits_constructor_call_identifier`, which actually runs to ~line 97 — read the whole function, the `containing_symbol_id` assertion is the point): `fetch("/api/users")` → literal `literal_text=="/api/users"`, `kind:Url`, `carrier=="fetch"`, `arg_position 0`, `enclosing_symbol_id`==caller. `axios.get("/api/users")` → `carrier=="axios.get"`. Template case `fetch(\`/api/users/${id}\`)` → decoded `"/api/users/{}"`.
- C# (`tests/csharp/identifier_extraction.rs`, `extract_all` helper at lines 21-34): `conn.Query<User>("SELECT Id, Name FROM Users WHERE Id = @id")` → literal containing `FROM Users`, `kind:Sql`, `carrier=="Query"`, `arg_position`, enclosing method. Verbatim `@"... FROM Orders"` and interpolated `$"SELECT * FROM {table}"` (static fragments survive, hole placeholdered).
- **Negative classification test (the allowlist boundary is the whole point):** a string-literal argument on a NON-carrier callee — `console.log("hello")` / `logger.Info("msg")` — emits **no** `Url`/`Sql` literal (either no `Literal` or `kind:Other`). Mirrors `test_di_non_registration_not_extracted`.
- **Negative name-leak assertions:** NO `Identifier` with `name=="/api/users"` or `name` containing `SELECT` is produced (proves no leak into the name index).
- **Re-index cleanup test (Rule 1):** in `incremental_update_atomic.rs`, same shape as Phase 2 — re-index a file with literals, assert no stale-row accumulation; plus the force-rebuild case.
- Migration test (fresh-at-latest + legacy-v27-upgrade-preserving-data + idempotent) and fresh-vs-migrated shape-equality test, per the Phase 2 pattern.
- Storage roundtrip + the polyglot `operations.rs` test extended to assert `literals` counts for the `.cs`/`.ts` files.

**Contract impact:** `schema_version`→28; **bump `EXTRACT_CONTRACT_VERSION` 1→2 in this batch's FINAL commit** (in lockstep with schema reaching 28 — see contract coordination); `EXTRACTION_CONTRACT_VERSION` batch suffix already bumped.

**Exit:** URL call-args and inline SQL literals are queryable structured records — decoded for interpolation/concatenation, classified by the shared config-driven carrier pass, cleaned on re-index, counted in `extract info` — for **every applicable language**, with TS + C# as the reference legs and a 100% implemented-vs-verified-N/A breadth ledger (Phase-2 pattern). `[literal_carriers]` config covers the common HTTP/DB client libraries per language; adding a client is a one-line TOML change. The non-calling formats (SQL/HTML/CSS/Regex/JSON/TOML/YAML/Markdown) are verified-N/A with cited grammar evidence.

---

## Out of scope (deliberately — Miller derives these itself)

The gap study found more real gaps whose correct fix is **Miller-side derivation from existing extracted data**, not julie extraction. **Do not implement these in julie** unless a future need makes a first-class form clearly better:
- **Call-site argument arity/payload** — recoverable from `identifiers.code_context` (overlaps Phase 3's literals for the high-value cases). *Caveat: `code_context` lines are capped at `ContextConfig.max_line_length = 120` (`base/types.rs:93`), so long argument lists can truncate.*
- **C# record positional parameters as field symbols** — recoverable by parsing the parameter list out of `symbols.signature`. **`signature` is genuinely uncapped** (no truncation in the extractor model or the SQLite TEXT column — verified; the 120-char cap applies only to `code_context` lines, not `signature`).
- **Per-parameter structured records (name/type/default/modifier)** — recoverable from `signature` (julie itself already re-derives return types this way at `csharp/type_inference.rs:31-58`).
- **Framework base-type detection** (`ControllerBase`/`DbContext`/`ComponentBase`) — edges are legitimately empty for external-assembly bases; Miller derives the base list from `signature` (safe — `signature` is uncapped). *Earlier drafts warned about "120-char code_context/signature truncation" dropping trailing interfaces; that risk applies only if Miller derives from `code_context`, not from `signature`. Derive from `signature`.*

Also confirmed **not gaps** (do not "fix"): inheritance/`Implements` edges (already extracted densely for C# `relationships.rs:114,141`, TS `relationships.rs:247,323,329`, Python `relationships.rs:86,88`), Go struct tags (raw tag already in `signature`; Go not in Miller's corpus).

## Migration recipe (quick reference — NEW-TABLE migrations, follow `migration_026`)

For each new table (027, 028):
1. **Re-verify contiguity first.** Re-read `LATEST_SCHEMA_VERSION` (`migrations.rs:16`) and the `apply_migration` match arms. Your migration number must be the next contiguous integer. If another change has already consumed 027, shift this batch to the next free numbers, update this plan's target schema, and **re-coordinate the final number with Miller before it re-pins.**
2. Bump `LATEST_SCHEMA_VERSION` to the next contiguous number.
3. Add `N => self.migration_N_<name>()?,` dispatch arm + the matching description arm in `record_migration`.
4. Write `fn migration_N_<name>(&self) -> Result<()>` whose body is **only** a `table_exists("...")` early-return guard + a call to `self.create_<name>_table()?`. **No inline `CREATE` DDL. No `has_column` (that is for column-add migrations like 025, not new-table migrations).**
5. Put all `CREATE TABLE`/`CREATE INDEX IF NOT EXISTS` DDL in `create_<name>_table()` in `schema.rs`, and call it from `initialize_schema()` in dependency order (after the tables it references). Both the migration (existing DBs) and `initialize_schema` (fresh DBs) call the same fn — this lockstep is what guarantees identical shapes.
6. Tests in `src/tests/core/database/migrations.rs`: (a) fresh-at-`LATEST` has the table + every index; (b) **legacy-vN upgrade on a populated old-shape DB** — build the prior-version DB by hand with real rows in the *parent* tables the new table references, migrate, assert version==LATEST, table+indexes exist, and parent rows survived; (c) idempotent re-open; (d) **fresh-vs-migrated shape equality** (`PRAGMA table_info` + index list identical between a fresh-at-N DB and a migrated-to-N DB). Assert on values, never just `is_ok()`.

## Verification (whole plan)

- `cargo nextest run` — full extractor + core + external_extract suites green.
- `cargo xtask` checks per `AGENTS.md` verification tier. The `CanonicalWriteSet` refactor (Phase 2 step 0) warrants a `cargo xtask test changed` (it touches shared persistence infrastructure).
- **Re-index cleanup is explicitly verified** (the orphaned-row failure mode): the `incremental_update_atomic.rs` tests for both new tables assert no stale-row accumulation on re-index and on force-rebuild, and no surviving row references a deleted parent id.
- End-to-end: `julie-server extract --db /tmp/t.db --root <small-polyglot-fixture> --json scan` then inspect: `symbol_annotations` has class/property/**field** rows (P1); `type_arguments` has ordered/nested rows (P2); `literals` has Url+Sql rows (P3). Then `julie-server extract --db /tmp/t.db --json info` reports **non-zero `type_arguments` and `literals` counts** (proves Rule 4 wiring), `extract_contract_version=2`, and `schema_version=28`.
- Record the final triple below so Miller's D5 gate moves in lockstep.

## Final version triple (fill in on completion)

> `(schema_version = __, extract_contract_version = __, EXTRACTION_CONTRACT_VERSION = "__")` — committed in <commit sha>. Notify the Miller gate owner to re-pin `MillerExtractContract` to these values at M4.
