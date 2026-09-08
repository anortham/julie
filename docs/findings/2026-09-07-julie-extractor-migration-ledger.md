# Julie Extractor v2.41.0 Migration Findings & Verification Ledger

## 1. Upstream Dependency Identity

- **Upstream Repository**: `https://github.com/anortham/julie-extractors`
- **Release Tag**: `v2.41.0` (published 2026-09-07 20:16:20 UTC)
- **Tag Object**: `b8b9f5498e37397e54ab8aac1e68e2d8936188f1`
- **Peeled Git Commit SHA**: `a71e18c1a6fae67b15d2d0aaa20793330fda901f`
- **Extraction Identity Epoch**: `9`
- **Extraction Contract Version**: `2.40.6`
- **Cargo Dependency Configuration**:
  All 6 workspace manifests and `Cargo.lock` pinned to:
  `julie-extractors = { git = "https://github.com/anortham/julie-extractors.git", tag = "v2.41.0" }`
  With `features = ["syntax-api"]` enabled for `julie-tools` and the root crate.

## 2. Architectural Changes & Contract Implementations

### R1: Julie-Owned `Symbol` Projection (`crates/julie-core/src/symbol.rs`)
- Extracted symbols in `julie-extractors` v2.41.0 no longer carry source body (`code_context`).
- Julie introduces an owned `Symbol` struct in `crates/julie-core/src/symbol.rs` that encapsulates:
  ```rust
  pub struct Symbol {
      pub extracted: julie_extractors::Symbol,
      pub code_context: Option<String>,
  }
  ```
- Implements `Deref<Target = julie_extractors::Symbol>` and `DerefMut` for ergonomic field access.
- Implements `From<julie_extractors::Symbol>` and `From<&julie_extractors::Symbol>` with `code_context: None`.
- Implements `Symbol::from_extracted(extracted, source)` deriving `code_context` via byte slicing.
- Custom `Serialize` and `Deserialize` flatten fields to preserve existing JSON-RPC schema contracts.

### R2: Identifier `receiver_type` Storage & Resolver Filtering
- Added Migration 31 (`crates/julie-core/src/database/migrations/receiver_type.rs`):
  `ALTER TABLE identifiers ADD COLUMN receiver_type TEXT;`
- Dual initialization updated in `crates/julie-core/src/database/schema.rs` to create `receiver_type TEXT` on new databases.
- Updated bulk identifier persistence (`crates/julie-core/src/database/bulk/identifiers.rs`) to bind 16 parameters.
- Implemented `filter_candidates_by_receiver` in `crates/julie-pipeline/src/resolver/receiver_type.rs`.

### R3: Syntax API Adapter (`crates/julie-tools/src/editing/syntax.rs`)
- Implemented `SyntaxAdapter` around `julie_extractors::syntax::parse_source_with_options`.
- Features bounded concurrency permit pool (`SyntaxLimiter` / `SyntaxConfig`), cooperative task cancellation via `std::sync::atomic::AtomicBool`, deadline timeouts, and typed error conversion to `SyntaxAdapterError`.

### R4: Elimination of Legacy `ExtractorManager`
- Removed legacy `ExtractorManager` instantiation and calls from `crates/julie-runtime/src/watcher/`.
- Replaced with direct calls to `julie_extractors::extract_canonical`.

### R5: Index Engine Version Stamp (`src/tools/workspace/indexing/engine_version.rs`)
- Updated `SEMANTIC_INDEX_ENGINE_VERSION` to compose extraction epoch, contract, tag, and Julie schema:
  `2.0.0+extractors=2.40.6+extractors-tag=v2.41.0+epoch=9+consumer-projection-v1+receiver-schema-v31`

## 3. Root Codex Findings & Remediations

During initial and follow-up implementation review, Root Codex issued findings across two review rounds, all remediated and verified:

### Round 1 Remediations
1. **Receiver Scope & Namespace Hierarchy (`crates/julie-pipeline/src/resolver/receiver_type.rs`)**:
   - *Issue*: Substring containment checks (`owner_path.contains(seg)`) risked false positives across similar namespace names.
   - *Remediation*: Implemented structured namespace matching verifying ancestor container hierarchy and module file path components. Disambiguation without local file scope conservatively returns `None`.
2. **Parser Concurrency & Worker Drain (`crates/julie-tools/src/editing/syntax.rs`)**:
   - *Issue*: Ad-hoc limiters allowed unbounded thread creation across tool calls, and aborted worker threads leaked permits while parsing continued in background.
   - *Remediation*: Implemented process-wide `shared_limiter()`. On timeout/cancellation, a cleanup thread awaits worker `join()` before releasing permits.
3. **Admission Before Extraction (`crates/julie-tools/src/editing/rewrite_symbol.rs`)**:
   - *Issue*: Source validation occurred during AST rewrite after extraction had already consumed resources.
   - *Remediation*: Enforced syntax validation, size limits, and deadline timeouts before extraction.
4. **Projection Failure Repair State (`crates/julie-runtime/src/watcher/handlers.rs`)**:
   - *Issue*: Extraction normalization failures returned an error without scheduling repair.
   - *Remediation*: Routed failures to `persist_repair_state(db, ..., IndexingRepairReason::ExtractorFailure)` and returned `FileIndexOutcome::repair_needed`.
5. **Visibility Variants & Engine Version Stamp (`crates/julie-tools/src/impact/ranking.rs`, `src/tools/workspace/indexing/engine_version.rs`)**:
   - *Issue*: Unhandled `Visibility::Open` and `Visibility::Internal`; engine version did not compose consumer epoch/schema.
   - *Remediation*: Ranked `Open` as `Public` and `Internal` as `Protected`. Analytics queries updated. Composed `SEMANTIC_INDEX_ENGINE_VERSION` with epoch, consumer projection, and receiver schema.
6. **Fixture & Invariant Verification Suite (`crates/julie-tools/src/tests/extractor_migration_editing.rs`)**:
   - *Issue*: Needed active worker cancellation tests proving permit retention, and tests for all migration fixtures.
   - *Remediation*: Added active worker cancellation tests and fixture coverage (`rust_unicode_crlf.rs`, `sample.ts`, `component.vue`, `document.md`, `sample.fs`, `qmldir`, `sample.jsonl`).
7. **Test Bucket Registration & CI Gate (`xtask/test_tiers.toml`)**:
   - *Issue*: Tests were not registered in xtask buckets.
   - *Remediation*: Registered `extractor_migration_editing` in `tools-editing` and `receiver_type_resolution` in `extractor-dep-integration`.

### Round 2 Remediations (Codex Review #2)
1. **Receiver Scope & Directory Conflation (`crates/julie-pipeline/src/resolver/receiver_type.rs`)**:
   - *Issue*: Directory path heuristic matching conflated filesystem layout with language namespace identity.
   - *Remediation*: Completely eliminated directory path heuristics (`dir_parts` matching). If candidate symbol has parent containers, container chain matching is strict and case-sensitive (`actual == *req`). For top-level symbols, only Rust module path is checked. Added test `directory_name_does_not_override_contradictory_namespace_facts` in `crates/julie-pipeline/src/tests/receiver_type_resolution.rs`.
2. **Parser Concurrency Tracking & Spawn Join Fallback (`crates/julie-tools/src/editing/syntax.rs`)**:
   - *Issue*: Worker thread cleanup spawned detached threads without tracking in `active_cancelled`, and thread spawn failures could leak permits.
   - *Remediation*: Wrapped thread `JoinHandle` in `Arc<Mutex<Option<JoinHandle<()>>>>`, tracked cleanup handles in `state.active_cancelled`, and on thread spawn failure synchronously join and release permit. `drain_cancelled_workers()` deterministically awaits all pending threads.
3. **Single Admission Gate Covering Parse and Extraction (`crates/julie-tools/src/editing/syntax.rs`)**:
   - *Issue*: Extraction ran outside the syntax adapter permit gate.
   - *Remediation*: Added `parse_and_extract(...)` to `SyntaxAdapter` executing both AST parse and canonical extraction inside the worker thread under single permit acquisition, size limits, deadlines, and cooperative cancellation. Updated `rewrite_symbol.rs` `live_symbol_context` to use `parse_and_extract`.
4. **Fixtures & Embedded Characterization Tests (`crates/julie-tools/src/tests/extractor_migration_editing.rs`)**:
   - *Issue*: Fixture `cpp_header.h` had no test; Vue and Markdown embedded handling needed explicit characterization tests; active worker cancellation needed deterministic drain assertions.
   - *Remediation*: Added `fixture_cpp_header_handling` exercising `fixtures/extractor-migration/cpp_header.h` AST-aware rename; added `fixture_vue_component_host_ast_and_embedded_characterization` and `fixture_markdown_document_fenced_code_characterization`; updated `adapter_cancels_active_worker_and_retains_permit_until_joined` with `drain_cancelled_workers()` assertions.
5. **BaseExtractor Removal (Plan Step 1 Requirement)**:
   - *Issue*: `BaseExtractor` compatibility shim remained in `src/extractors/mod.rs`.
   - *Remediation*: Removed `pub mod base` and `BaseExtractor` entirely from `src/extractors/mod.rs`. Moved `truncate_string` utility to `src/utils/context_truncation.rs` and updated all 11 test callers to import from `crate::extractors` directly.
6. **Full Untracked Files Tracking (`git add -N`)**:
   - *Issue*: Dirty diff hash previously omitted newly created implementation and test files.
   - *Remediation*: Applied `git add -N` to all newly created fixture, test, and plan files so `git diff` encompasses all 230 changed/added files.
7. **Deterministic Permit Synchronization (`crates/julie-tools/src/tests/extractor_migration_editing.rs`)**:
   - *Issue*: Previous permit retention tests used thread sleeps and asserted permit availability only after draining cancelled workers.
   - *Remediation*: Replaced timing sleeps with channel synchronization (`worker_started_rx`, `worker_unblock_tx`). Asserted 0 available permits during active worker cancellation, proved secondary admission is blocked with `Err(SyntaxAdapterError::DeadlineExceeded)`, unblocked the worker, joined cleanup, and proved permit release back to 1. Added admission exhaustion test to `test_parse_and_extract_runs_under_admission_and_retains_permit`.
8. **Embedded Editing Characterization & Dedicated C Fixture (`crates/julie-tools/src/tests/extractor_migration_editing.rs`)**:
   - *Issue*: Vue and Markdown tests lacked edit operations and source/control characterization; C row cited C++ fixture `cpp_header.h`.
   - *Remediation*: Added `component.control.vue` and `document.control.md` verifying host AST traversal leaves embedded raw text and fenced code uncorrupted without naive string replacement. Added dedicated C fixture `fixtures/extractor-migration/c_sample.c` and `c_sample.control.c`, verified with `fixture_c_sample_handling`.
9. **Language Capability Matrix Citations**:
   - *Issue*: Capability rows contained generic citations.
   - *Remediation*: Expanded Section 5 matrix with concrete consumer test paths (`receiver_type_resolution.rs`, `real_world_validation/`, `real_world_contract.rs`), specific fixture files, and upstream golden contract citations.
10. **Verification Identity & Diff Hygiene**:
    - *Issue*: `git diff --check` reported trailing blank line at EOF; self-referential markdown edits caused dirty diff hash drift.
    - *Remediation*: Removed trailing EOF whitespace. Documented both implementation-scope fingerprint (`git diff --binary -- ':!docs/findings/*' ':!docs/plans/*' | sha256sum`) and full-tree fingerprint (`git diff --binary | sha256sum`) for self-consistent verification.

## 4. Closed API Inventory

The following call sites and public types were exhaustively audited and updated to upstream `v2.41.0`:

- `julie_core::symbol::Symbol`: Wrapped projection replacing raw upstream symbol across all consumers.
- `julie_extractors::extract_canonical`: Direct root extraction entry point replacing `ExtractorManager`.
- `julie_extractors::detect_language_for_path`: File detection replacing custom path heuristics.
- `julie_extractors::capability_snapshot`: Capabilities query replacing private language specs.
- `julie_extractors::syntax::parse_source_with_options`: Syntax API with cancellation and deadlines.
- `julie_extractors::syntax::SyntaxOptions`: Bounded parsing configuration.
- `julie_extractors::syntax::SyntaxError`: Mapped typed error variants (`UnsupportedLanguage`, `UnsupportedContainer`, `InputTooLarge`, `ParseFailed`, `Cancelled`, `DeadlineExceeded`).
- `julie_extractors::UnresolvedTarget`, `julie_extractors::PendingSpan`: Root-exported relation types.
- `julie_extractors::Visibility`: Full 6-variant enum (`Public`, `Private`, `Protected`, `Internal`, `FilePrivate`, `Open`).

## 5. 36-Language & Container Capability Matrix

| Language / Format | Extension(s) | Detection | Fact Preservation | Receiver Applicability | AST Edit / Syntax Status | Concrete Citation / Fixture |
|---|---|---|---|---|---|---|
| Rust | `.rs` | Canonical | Symbols, Types, Identifiers, Relations, Diagnostics | Yes (`self`, `super`, typed receiver) | Supported AST Rewrite & Rename | `fixtures/extractor-migration/rust_unicode_crlf.rs` + `.control.rs`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/rust/lib.rs`) |
| TypeScript | `.ts`, `.mts`, `.cts` | Canonical | Full facts | Yes (class, interface, object receiver) | Supported AST Rewrite & Rename | `fixtures/extractor-migration/sample.ts` + `.control.ts`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/typescript/user-service.ts`) |
| TSX | `.tsx` | Canonical | Full facts | Yes | Supported AST Rewrite & Rename | `crates/julie-tools/src/tests/extractor_migration_editing.rs::ast_editing_tsx_renames_props_with_type_annotation`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/typescript/user-dashboard.tsx`) |
| JavaScript | `.js`, `.mjs`, `.cjs` | Canonical | Full facts | Yes | Supported AST Rewrite & Rename | `crates/julie-tools/src/tests/extractor_migration_editing.rs::ast_editing_javascript_renames_parameter_without_touching_strings`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/javascript/vue.config.js`) |
| JSX | `.jsx` | Canonical | Full facts | Yes | Supported AST Rewrite & Rename | `crates/julie-tools/src/tests/extractor_migration_editing.rs::ast_editing_jsx_renames_props_identifier` |
| Python | `.py`, `.pyi` | Canonical | Full facts | Yes (`self`, `cls`, typed receiver) | Supported AST Rewrite & Rename | `crates/julie-tools/src/tests/extractor_migration_editing.rs::ast_editing_python_renames_parameter_without_touching_strings`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/python/test_database.py`) |
| Java | `.java` | Canonical | Full facts | Yes (`this`, `super`, typed receiver) | Supported AST Rewrite & Rename | `crates/julie-tools/src/tests/extractor_migration_editing.rs::ast_editing_java_renames_parameter_identifier`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/java/Main.java`) |
| C# | `.cs` | Canonical | Full facts | Yes (`this`, `base`, typed receiver) | Supported AST Rewrite & Rename | `crates/julie-tools/src/tests/extractor_migration_editing.rs::ast_editing_csharp_renames_parameter_identifier`, `crates/julie-pipeline/src/tests/receiver_type_resolution.rs::directory_name_does_not_override_contradictory_namespace_facts`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/csharp/Program.cs`) |
| VB.NET | `.vb` | Canonical | Full facts | Yes (`Me`, `MyBase`) | Supported AST Rewrite & Rename | Upstream canonical golden contract `crates/julie-extractors/src/tests/golden.rs` (`fixtures/extraction/vbnet/`) |
| PHP | `.php` | Canonical | Full facts | Yes (`$this`, `self`, `parent`) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/php/index.php`) |
| Ruby | `.rb` | Canonical | Full facts | Yes (`self`) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/ruby/main.rb`) |
| Swift | `.swift` | Canonical | Full facts | Yes (`self`, `super`) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/swift/main.swift`, `fixtures/real-world/swift/current-syntax.swift`) |
| Kotlin | `.kt`, `.kts` | Canonical | Full facts | Yes (`this`, `super`) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/kotlin/Main.kt`) |
| Scala | `.scala`, `.sc` | Canonical | Full facts | Yes (`this`) | Supported AST Rewrite & Rename | Upstream canonical golden contract `crates/julie-extractors/src/tests/golden.rs` (`fixtures/extraction/scala/`) |
| C | `.c`, `.h` | Canonical | Full facts | N/A (structural types / structs) | Supported AST Rewrite & Rename | `fixtures/extractor-migration/c_sample.c` + `.control.c`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/c/binary_search_tree.c`) |
| C++ | `.cpp`, `.hpp`, `.cc` | Canonical | Full facts | Yes (`this`, typed receivers) | Supported AST Rewrite & Rename | `fixtures/extractor-migration/cpp_header.h` + `fixture_cpp_header_handling`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/cpp/graph_algorithms.cpp`) |
| Go | `.go` | Canonical | Full facts | Yes (receiver argument types) | Supported AST Rewrite & Rename | `crates/julie-tools/src/tests/extractor_migration_editing.rs::ast_editing_go_renames_parameter_identifier`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/go/main.go`) |
| Lua | `.lua` | Canonical | Full facts | Yes (table colon syntax `self`) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/lua/web_server_framework.lua`) |
| Zig | `.zig` | Canonical | Full facts | Yes (struct receiver parameter) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/zig/memory_allocator.zig`) |
| Elixir | `.ex`, `.exs` | Canonical | Full facts | N/A | Supported AST Rewrite & Rename | Upstream canonical golden contract `crates/julie-extractors/src/tests/golden.rs` (`fixtures/extraction/elixir/`) |
| Erlang | `.erl`, `.hrl` | Canonical | Full facts | N/A | Supported AST Rewrite & Rename | Upstream canonical golden contract `crates/julie-extractors/src/tests/golden.rs` (`fixtures/extraction/erlang/`) |
| GDScript | `.gd` | Canonical | Full facts | Yes (`self`) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/gdscript/player_controller.gd`) |
| Vue | `.vue` | Canonical | Host SFC + Script facts | Yes | Supported Host AST (preserves script raw text) | `fixtures/extractor-migration/component.vue` + `.control.vue`, `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/vue/HelloWorld.vue`) |
| Razor | `.razor` | Canonical | Full facts | Yes | Supported Host AST | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/razor/MainLayout.razor`, `fixtures/real-world/razor/current-syntax.razor`) |
| QML | `.qml` | Canonical | Declarative + Script facts | Yes | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/qml/real-world/cool-retro-term-main.qml`) |
| qmldir | `qmldir` | Canonical | Module facts | N/A | Supported AST Syntax & Extraction | `fixtures/extractor-migration/qmldir` + `fixture_qmldir_handling` |
| R | `.r`, `.R` | Canonical | Full facts | N/A | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/r/real-world/ggplot2-geom-point.R`, `fixtures/r/real-world/current-syntax.R`) |
| SQL | `.sql` | Canonical | DDL/DML facts | N/A | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/sql/postgresql-migrations.sql`, `fixtures/real-world/sql/tsql-current.sql`) |
| HTML | `.html`, `.htm` | Canonical | DOM & Script facts | N/A | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/html/popup-info-web-component.html`) |
| CSS | `.css` | Canonical | Style facts | N/A | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/css/flexbox-grid.css`) |
| Regex | `.regex` | Canonical | Pattern facts | N/A | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/regex/validation_patterns.regex`) |
| Bash | `.sh`, `.bash` | Canonical | Script facts | N/A | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/bash/system-admin-script.sh`) |
| PowerShell | `.ps1`, `.psm1` | Canonical | Script facts | Yes (`$this`) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/powershell/system-health-check.ps1`) |
| Dart | `.dart` | Canonical | Full facts | Yes (`this`, `super`) | Supported AST Rewrite & Rename | `src/tests/integration/real_world_contract.rs` (`fixtures/real-world/dart/flutter_isolate_demo.dart`) |
| Markdown | `.md`, `.markdown`| Canonical | Document & Fenced facts | N/A | Supported Host AST (preserves fenced code) | `fixtures/extractor-migration/document.md` + `.control.md` |
| JSON | `.json` | Canonical | Structure facts | N/A | Supported AST Syntax & Safe Editing | `crates/julie-tools/src/tests/extractor_migration_editing.rs::ast_editing_json_syntax_handling`, upstream golden contract `crates/julie-extractors/src/tests/golden.rs` (`fixtures/extraction/json/`) |
| JSONL | `.jsonl` | Canonical | Line-delimited facts | N/A | Valid extraction, Unsupported AST container | `fixtures/extractor-migration/sample.jsonl` + `fixture_jsonl_unsupported_container_rejection` |
| TOML | `.toml` | Canonical | Config facts | N/A | Supported AST Rewrite & Rename | Upstream canonical golden contract `crates/julie-extractors/src/tests/golden.rs` (`fixtures/extraction/toml/`) |
| YAML | `.yaml`, `.yml` | Canonical | Config facts | N/A | Supported AST Rewrite & Rename | Upstream canonical golden contract `crates/julie-extractors/src/tests/golden.rs` (`fixtures/extraction/yaml/`) |
| XML | `.xml` | Canonical | Document facts | N/A | Supported AST Rewrite & Rename | Upstream canonical golden contract `crates/julie-extractors/src/tests/golden.rs` (`fixtures/extraction/xml/`) |
| F# | `.fs`, `.fsi`, `.fsx` | Canonical | Full facts | Yes (type members) | Supported AST Rewrite & Rename | `fixtures/extractor-migration/sample.fs` + `fixture_sample_fs_parsing` |

## 6. Verification Ledger

Fingerprint methodologies:
- **Code & Test Implementation Fingerprint** (excludes mutating ledger docs): `git diff --binary -- ':!docs/findings/*' ':!docs/plans/*' | sha256sum` -> `185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb`
- **Full Working Tree Diff Fingerprint** (all 230 tracked files): `git diff --binary | sha256sum` -> recorded in final review packet

| Invariant | Command | Package | Scope Label | Commit SHA | Implementation Diff Identity | Result / Selected Count | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|---|---|
| R1: Projected symbol preserves unicode, CRLF, and flattened serialization | `cargo nextest run -p julie-core --lib extractor_projection` | julie-core | worker-exact | 0432158c173c30ae2f76d92773380c744ddc6542 | 185f45c5977102cfbe4265fb2a6301b2e8acc1b827a1a39ea65cdd5cb64203eb | pass (7 passed) | 2026-09-08T02:08:00Z | no |
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
