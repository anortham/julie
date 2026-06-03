# Extraction Contract — `julie-extractors`

## Overview

`julie-extractors` parses source code with tree-sitter and emits a stable
`ExtractionResults` shape for 34 production languages (plus TSX/JSX
aliases). The crate source now lives in the external
[`anortham/julie-extractors`](https://github.com/anortham/julie-extractors)
repo and is consumed here as a pinned git dependency. The public API is the
canonical entry point `extract_canonical(file_path, content, workspace_root)`
plus the read-only `capability_snapshot()` function for declared per-language
guarantees. See [`TREE_SITTER_QUALITY_BAR.md`](TREE_SITTER_QUALITY_BAR.md)
for the quality rubric this contract is graded against.

## Tier Model

Languages live in target groups. The classification is
stored in `fixtures/extraction/capabilities.json` under each row's
`target_capabilities` flags.

- **Full** (`symbols + relationships + pending_relationships + identifiers + types`) — first-class extractors used by core MCP tools (Rust, TypeScript, Python, C#, Java, Go, etc.).
- **No-types** (`symbols + relationships + pending_relationships + identifiers`, `types=false`) — structural languages without a static type system (Lua, R, Ruby, etc.).
- **Relationship-data** (`symbols + relationships + identifiers`, `pending_relationships=false`, `types=false`) — CSS, HTML, and other declarative formats with intra-document references.
- **Relationship-data without identifiers** (`symbols + relationships`, `pending_relationships=false`, `identifiers=false`, `types=false`) — formats whose keys/headings are already modeled as symbols and relationships, not code identifiers (Markdown, TOML).
- **Pending relationship data without identifiers** (`symbols + relationships + pending_relationships`, `identifiers=false`, `types=false`) — JSON-like data where schema/domain references are relationships, not code identifiers.

Each row's `capabilities` field reports what the implementation
actually emits; `target_capabilities` reports what the
classification intends. Drift is enforced by
`capability_matrix_matches_registry_entries` in
`src/tests/capability_matrix.rs` of the external `anortham/julie-extractors` repo.
Each row also carries `kind_coverage`, which records fixture-proven
`supported`, intrinsic `not_applicable`, and planned `open_gaps`
entries for current `SymbolKind`, `RelationshipKind`, and
`IdentifierKind` values, plus the body-span/body-hash coverage domain.

## `ExtractionResults` Shape

The crate returns `ExtractionResults` (defined in `src/base/extraction_results.rs` of the external `anortham/julie-extractors` repo)
with seven fields:

- `symbols: Vec<Symbol>` — every named entity (file, function, class,
  method, field, import, etc.). Each `Symbol` carries `id`,
  `language`, `name`, `kind`, `signature`, `file_path`, line/column
  spans, optional `body_span`, optional `body_hash`, optional
  `parent_id`, `metadata`, `doc_comment`, etc.
- `relationships: Vec<Relationship>` — resolved edges. Each carries
  `from_symbol_id`, `to_symbol_id`, `kind`
  (Calls/Uses/Imports/Inherits/References/...), `file_path`,
  `line_number`, `confidence`, and free-form `metadata`.
- `pending_relationships: Vec<PendingRelationship>` — legacy
  flat-name pending entries. Newly written extractors should not
  use this path directly; emit via
  `structured_pending_relationships` and let the canonical pipeline
  derive the legacy queue.
- `structured_pending_relationships: Vec<StructuredPendingRelationship>`
  — typed pending entries with an `UnresolvedTarget`
  (`display_name`, `terminal_name`, `receiver`, `namespace_path`,
  `import_context`) plus the original `PendingRelationship`. This is
  the contract the cross-file resolver consumes.
- `identifiers: Vec<Identifier>` — usage locations (call sites,
  references, member accesses) with `name`, `kind`, `file_path`, and
  an optional `containing_symbol_id`.
- `types: HashMap<String, TypeInfo>` — symbol-id → type metadata
  for languages with static types.
- `parse_diagnostics: Vec<ParseDiagnostic>` — tree-sitter error and
  missing-node spans.

See
`src/base/types.rs` in the external `anortham/julie-extractors` repo
for field-by-field details.

## Body Span And Hash Contract

`body_span` and `body_hash` are additive symbol fields for symbols with
a coherent executable, declarative, or structural body. They are emitted
for every applicable language/kind and recorded in
`kind_coverage.body_spans`.

- `body_span` uses the same coordinate model as declaration spans:
  1-based lines, 0-based columns, and byte offsets into the original
  source content after embedded-language offsets are applied.
- `body_span` must be in the same file as the declaration span and
  contained by that declaration span.
- When tree-sitter exposes a body node, `body_span` covers that body
  node. Delimited and indentation-sensitive languages include the
  grammar body delimiters or suite when those nodes are part of the
  body.
- `body_span` excludes documentation comments, decorators/attributes,
  and declaration headers when the grammar separates them from the
  body.
- Leaf declarations with no coherent body use `body_span = None` and
  classify the kind as `not_applicable` in
  `kind_coverage.body_spans`.
- `body_hash` is present exactly when `body_span` is present. It is a
  deterministic digest of the normalized token stream inside the body
  span: whitespace-only formatting changes do not change the hash;
  comments, literals, identifiers, operators, punctuation, and
  delimiters do.

Downstream consumers must read `kind_coverage.body_spans`; they must not
hardcode language allowlists.

## Structured Pending Relationship Contract

Pending relationships represent references whose targets cannot be
resolved at extraction time. The shape lives in
`src/base/relationship_resolution.rs` in the external `anortham/julie-extractors` repo:

- `target.terminal_name` — the searchable identifier (function name,
  type name, etc.).
- `target.display_name` — the fully-qualified form when known
  (`Module::SubModule::fn_name`).
- `target.receiver` — for method calls, the receiver expression
  (e.g., `self`, `obj.field`).
- `target.namespace_path` — segmented namespace/module path when the
  language uses explicit namespacing.
- `target.import_context` — the source module string for imported
  symbols (e.g., `./other` for `import { foo } from './other'`).
- `caller_scope_symbol_id` — the symbol id of the containing scope
  (function, class, component, file).
- `pending.kind` — RelationshipKind (Calls/Imports/References/...).
- `pending.file_path`, `pending.line_number` — emission site.
- `pending.confidence` — 0.0-1.0 hint.

Negative-case fixtures and locking tests enforce that intra-file
references don't leak into the pending queue. See per-language
`tests/<lang>/cross_file_pending.rs` and
`capability_matrix_negative_cases_emit_no_wrong_edges`.

## Capability Snapshot API

Downstream consumers read per-language guarantees from a stable,
typed snapshot loaded at compile time:

```rust
use julie_extractors::{capability_snapshot, EXTRACTION_CONTRACT_VERSION};

let snap = capability_snapshot();
for row in snap.languages() {
    println!("{}: targets={:?} actual={:?}",
        row.language, row.target_capabilities, row.capabilities);
}

if let Some(rust) = snap.get("rust") {
    assert!(rust.target_capabilities.symbols);
    assert!(rust
        .kind_coverage
        .symbols
        .supported
        .iter()
        .any(|kind| kind == "function"));
}

// Drift detection
let _version = EXTRACTION_CONTRACT_VERSION; // "2026-05-11.body-span-v1"
```

The snapshot is loaded from `capabilities.json` via `include_str!` in the external `anortham/julie-extractors` crate —
**no build script**. See `src/capability_snapshot.rs` in that repo.

## Typed Evidence Schema

Every `capability_gaps` row in `capabilities.json` carries a typed
`evidence` object. Three kinds:

- `{"kind": "test", "value": "<test_name>", "command": "<runner>"}`
  — a locking nextest reference; the test name must resolve via
  `cargo nextest list`. Used for Recipe B no-pending classifications
  (CSS, regex, Markdown, YAML, Razor, etc.).
- `{"kind": "fixture", "value": "fixtures/...", "command": "..."}`
  — a fixture path on disk; the file must exist.
- `{"kind": "commit", "value": "<sha>", "command": "git show <sha>"}`
  — a commit SHA reachable via `git cat-file -e`.

Bare-string evidence is rejected by
`capability_matrix_evidence_resolves`. The schema is enforced by
`src/tests/capability_matrix.rs` in the external `anortham/julie-extractors` repo.

## Kind Coverage Schema

Every `kind_coverage` domain has this shape:

```json
{
  "supported": ["function"],
  "not_applicable": ["event"],
  "open_gaps": [
    {
      "kind": "overrides",
      "reason": "why current fixtures do not prove it",
      "required_closure": "specific fixture or extractor work required",
      "planned_closure_task": "Milestone/task reference"
    }
  ]
}
```

`supported` claims must appear in golden fixture output. `open_gaps`
must carry a concrete closure reference. `not_applicable` means the
kind is intrinsic nonsense for that language/domain, not merely
unimplemented.

The `body_spans` domain uses the same schema. A `supported` claim means
at least one golden fixture symbol of that kind carries both
`body_span` and `body_hash`.

## Where to Find Machine-Checked Truth

The authoritative sources now live in the external [`anortham/julie-extractors`](https://github.com/anortham/julie-extractors) repo:

- `capabilities.json` — declared capabilities, fixtures, and typed evidence per language. The crate loads this via `include_str!`. Never move this file.
- Certification report and real-world evidence JSON — generated by the certify tooling in that repo. These are no longer checked into `docs/` here.
- Downstream-consumer gate — `cargo nextest run --test downstream_smoke` in the external repo spawns a tempdir consumer crate and runs a program calling `extract_canonical` + `capability_snapshot` + `kind_coverage` + `EXTRACTION_CONTRACT_VERSION`.
