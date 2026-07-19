# Julie Extractors v2.16 Consumer Upgrade Design

## Status

Approved direction: full consumer adoption.

Julie currently pins `julie-extractors` v2.14.0. The target is v2.16.0, including
the v2.15 and v2.16 extractor additions and first-class consumption of the
`source_regions`, `structural_facts`, and `complexity_metrics` collections that
Julie currently discards.

Authoritative upstream release inputs:

- [v2.15.0 release](https://github.com/anortham/julie-extractors/releases/tag/v2.15.0)
- [v2.16.0 release](https://github.com/anortham/julie-extractors/releases/tag/v2.16.0)
- `julie_extractors::EXTRACTION_CONTRACT_VERSION`
- `julie_extractors::base::{SourceRegion, SourceRegionKind, StructuralFact, ComplexityMetric}`

## Goals

- Pin every Julie workspace crate to `julie-extractors` v2.16.0.
- Preserve all three enrichment domains in Julie's canonical SQLite database.
- Keep full indexing, watcher updates, and the external extract CLI behaviorally
  identical.
- Expose structural facts through a generic `patterns` MCP and standalone CLI
  tool.
- Add source-region filtering to `fast_search` without changing ordinary search
  behavior.
- Show stored complexity metrics in `deep_dive`.
- Force a reindex when the upstream extraction contract changes.
- Keep every new implementation file at or below 500 lines.

## Non-Goals

- Reimplementing extractor logic in Julie.
- Adding or changing language grammars in this repository.
- Exposing raw tree-sitter query execution.
- Replacing Julie's canonical database with the upstream artifact database.
- Adding a complexity ranking or hotspots tool in this change.
- Changing the external extract JSONL or SQLite artifact contract owned by
  `julie-extractors`.
- Releasing or pushing Julie.

## Current Gap

`process_file_with_parser_using` consumes symbols, relationships, identifiers,
types, type arguments, literals, and parse diagnostics from
`ExtractionResults`. It drops:

- `source_regions`
- `structural_facts`
- `complexity_metrics`

The same omission exists in the watcher path. Consequently, new v2.15/v2.16
facts such as Symfony routes and expanded `http.client_request.v1` coverage are
computed upstream but unavailable to Julie tools.

## Architecture Decision

### Typed canonical tables

Add schema version 29 with one table per upstream domain:

- `source_regions`
- `structural_facts`
- `complexity_metrics`

Each table keeps the upstream stable ID, language, file-relative span, optional
symbol binding, metadata JSON, and domain-specific typed columns. File and
symbol indexes support cleanup and tool queries. The canonical atomic write set
borrows all three typed slices, so a file replacement removes and reinserts all
owned data in the same transaction as symbols and relationships.

### One normalization seam

Replace the parser result tuple with `NormalizedExtractionData`. A single
`normalize_extraction_results` function:

- applies existing literal carrier classification;
- applies existing test-role classification;
- flattens type-argument usages;
- converts upstream type maps to rows;
- carries source regions, structural facts, complexity metrics, and diagnostics
  unchanged.

Full indexing, watcher updates, and external extraction all consume this
normalized structure. This is the only supported seam from upstream extraction
output to Julie persistence.

### Consumer surfaces

`patterns` is a new read-only tool modeled on the proven generic structural-fact
contract:

- `operation=list` lists observed pattern IDs and counts.
- `operation=summary` groups by language/pattern/capture, file, or directory.
- `operation=search` accepts an exact `pattern_id` or case-insensitive pattern
  substring `query`.
- Optional `path`, `language`, `where`, `facet`, `limit`, `workspace`, and
  `format` filters remain language-agnostic.

`fast_search` gains a wire-level `regions` parameter. Values are a comma list of
`comment`, `doc_comment`, `string_literal`, or `embedded`; `docstring` aliases
`doc_comment`. Region-scoped requests use the existing line-search candidate
pipeline and apply stored spans while file content is inspected. Ordinary
search continues through the unchanged unified lexical/semantic path.

`deep_dive` adds the selected symbol's stored complexity metric to
`SymbolContext` and prints one compact header line:

```text
complexity: decisions=4 loops=2 nesting=3 params=2 lines=18
```

No metric means no line.

## Interface Boundaries

### `julie-core`

Owns schema, migrations, typed inserts, cleanup, and read queries. Tools do not
issue ad hoc SQL.

### `julie-pipeline`

Owns conversion from `julie_extractors::ExtractionResults` to the canonical
write shape. It is the shared dependency of live indexing and external
extraction.

### `julie-runtime`

The watcher calls the shared normalization seam and assembles the same
`CanonicalWriteSet`; it does not duplicate enrichment transformations.

### `julie-tools`

Owns tool parameters, validation, querying, formatting, and result-size limits.
The MCP handler and CLI are adapters only.

## Alternatives Rejected

### One generic JSON facts table

Rejected because it loses typed constraints, makes cleanup and indexing weaker,
and pushes schema interpretation into every consumer.

### Read the upstream artifact database beside Julie's database

Rejected because it splits canonical truth, transaction boundaries, workspace
routing, and watcher freshness across two stores.

### Expose only the dependency bump

Rejected because Julie would continue paying extraction cost while discarding
the new data, leaving the consumer upgrade incomplete.

### Add three standalone tools

Rejected because source regions and complexity are more useful as filters and
context on existing workflows. Only structural facts need a new discovery
surface.

## Migration and Reindex Behavior

- SQLite schema moves from 28 to 29.
- Migration 29 creates all three tables and indexes idempotently.
- `SEMANTIC_INDEX_ENGINE_VERSION` continues to embed the exact upstream
  `EXTRACTION_CONTRACT_VERSION`.
- If v2.16 retains the v2.14 extraction-contract string, append a Julie-owned
  consumer-shape suffix to the engine version so existing workspaces still
  reindex and populate the new tables.
- External extract databases migrate through the same `SymbolDatabase`
  initialization path.

## Error and Validation Rules

- Unknown `patterns.operation`, `patterns.group_by`, `patterns.format`,
  malformed `where`, unknown region names, or region-scoped semantic/hybrid
  search return typed invalid-parameter errors.
- Metadata filters are equality-only and parameterized; malformed stored JSON is
  skipped.
- Missing optional symbol bindings are stored as `NULL`, matching identifiers
  and literals.
- Per-file cleanup explicitly deletes enrichment rows because bulk writes
  temporarily disable foreign-key enforcement.
- Limits clamp to 1–500.

## Verification

- TDD exact tests for every slice.
- `cargo check` after each implementation step.
- `cargo xtask test changed` after coherent batches.
- `cargo xtask test bucket extractor-dep-integration` because the parser and
  extractor dependency changes.
- `cargo xtask test system` because watcher and workspace indexing behavior
  changes.
- `cargo xtask test dev` once at branch completion.
- Standalone dogfood with `julie-server patterns`, region-scoped
  `julie-server search`, and `julie-server deep-dive`.
- Evidence recorded in a copy of
  `docs/plans/verification-ledger-template.md`.

## Architecture Quality

Architecture risk is high: the change touches canonical persistence, every
indexing writer, a public search contract, a new MCP/CLI tool, and deep-dive
formatting. The approved shape controls that risk with one typed persistence
lane and one shared normalization seam. If implementation discovers that a
writer bypasses `CanonicalWriteSet`, or that a planned tool must issue raw SQL,
that is a plan mismatch and must be surfaced before redesigning locally.
