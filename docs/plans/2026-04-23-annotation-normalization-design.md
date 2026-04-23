# Annotation Normalization Design

> Plan B of the CLI + Security Intelligence initiative.
> Companion to: `docs/plans/2026-04-23-cli-annotations-early-warnings.md`

## Goal

Add a canonical annotation layer across all 34 supported languages. Decorators (Python, TS, JS), annotations (Java, Kotlin, Scala, Dart), attributes (C#, Rust, C++, PHP, VB.NET, PowerShell), and module attributes (Elixir, GDScript, Swift) all normalize into one data model. Downstream consumers (test detection, security signals, search, dashboard reporting) query a single contract instead of per-language metadata hacks.

## Non-Goals

- Exhaustive argument parsing (route paths, config predicates, derive trait lists beyond expansion)
- A new MCP tool or search target for annotations
- Taint analysis or security scanning (that's Plan C, and even Plan C only claims structural signals)

---

## Design Decisions

### 1. Storage: Junction Table

Annotations are stored in a dedicated SQLite junction table, not a JSON column.

```sql
CREATE TABLE symbol_annotations (
  symbol_id TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
  ordinal   INTEGER NOT NULL,
  annotation TEXT NOT NULL,
  raw_text   TEXT,
  PRIMARY KEY (symbol_id, ordinal)
);

CREATE INDEX idx_anno_annotation ON symbol_annotations(annotation, symbol_id);
CREATE INDEX idx_anno_symbol     ON symbol_annotations(symbol_id, ordinal);
```

**Why junction table over JSON column:**
- Downstream features (test detection, security signals, dashboard) need set membership queries, counts, grouping, and joins against config rules. These are native junction-table operations.
- JSON `json_each()` has no clean index story for annotation membership.
- Schema extends cleanly if we later add per-annotation metadata (arguments, source spans).
- Tantivy handles full-text annotation search; SQLite handles structured queries and reporting.

**Columns:**
- `symbol_id`: FK to `symbols(id)`, cascade delete on symbol removal.
- `ordinal`: declaration order after carrier expansion. Preserves `Vec<String>` semantics.
- `annotation`: canonical normalized name (the match key). See normalization contract below.
- `raw_text`: original source fragment for the expanded row (not the whole carrier). Preserves arguments, qualified names, and syntax for debugging and future features.

**Write pattern:** Delete-and-reinsert within a transaction during indexing. The current `INSERT OR REPLACE` on `symbols` would cascade-delete child rows, so symbol upserts must use `INSERT ... ON CONFLICT(id) DO UPDATE` or the annotation write must follow the symbol write in the same transaction.

### 2. Symbol Struct Changes

Add a canonical annotations field to `Symbol` in `crates/julie-extractors/src/base/types.rs`:

```rust
/// Canonical annotation markers (normalized names)
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub annotations: Vec<String>,
```

Add annotations to `SymbolOptions` so extractors can pass them through `create_symbol()`:

```rust
pub annotations: Vec<String>,  // in SymbolOptions, default empty
```

The `metadata` field retains its current role for language-specific data. Annotations move out of metadata into the dedicated field. Existing `metadata["decorators"]` and `metadata["attributes"]` entries are removed from extractors as they're migrated.

### 3. Normalization Contract

All extractors call a shared normalization helper. The helper takes raw annotation text and the source language, and returns `Vec<AnnotationMarker>`.

```rust
// crates/julie-extractors/src/base/annotations.rs

pub struct AnnotationMarker {
    pub canonical: String,
    pub raw_text: String,
}

pub fn normalize_annotations(raw_texts: &[String], language: &str) -> Vec<AnnotationMarker>;
```

**Rules (applied in order):**

| Rule | Description | Example |
|------|-------------|---------|
| Strip syntax wrappers | Remove `@`, `#[`, `]`, `[]`, `<>` surrounding text | `@Injectable()` -> `Injectable()` |
| Expand carriers | `derive(X, Y)` and multi-attribute `[X, Y]` produce one row per semantic unit | `derive(Debug, Clone)` -> `Debug`, `Clone` |
| Drop invocation arguments | Remove parenthesized argument lists | `app.route("/api")` -> `app.route` |
| Expression paths: keep full callable path | Dotted/qualified paths before args are preserved | `pytest.mark.parametrize(...)` -> `pytest.mark.parametrize` |
| Type names: collapse to rightmost | Fully qualified Java-style names collapse | `org.junit.jupiter.api.Test` -> `Test` |
| C# `Attribute` suffix: strip at storage time | Trailing `Attribute` removed from canonical | `TestMethodAttribute` -> `TestMethod` |
| Preserve source case | Do not lowercase canonical values | `GetMapping` stays `GetMapping` |
| Deduplicate | Repeated identical canonical values on one symbol stored once | Two `@override` -> one row |

**Carrier expansion detail:**
- Rust `#[derive(Debug, Clone)]` -> rows: `Debug` (raw: `Debug`), `Clone` (raw: `Clone`). No `derive` row.
- C# `[Authorize, Route("api")]` -> rows: `Authorize` (raw: `Authorize`), `Route` (raw: `Route("api")`).
- Rust `#[cfg(test)]` -> single row: `cfg` (raw: `cfg(test)`). `cfg` is NOT a carrier; its argument is a predicate, not a list of annotations.
- Rust `#[serde(rename_all = "camelCase")]` -> single row: `serde` (raw: `serde(rename_all = "camelCase")`).

**`raw_text` after expansion:** Each expanded row gets its own fragment, not the whole carrier source. For `#[derive(Debug, Clone)]`, the `Debug` row's raw_text is `"Debug"`, not `"#[derive(Debug, Clone)]"`.

### 4. Language Coverage

**16 languages with annotation syntax (extraction required):**

| Language | Syntax | Carrier Forms | Notes |
|----------|--------|---------------|-------|
| Python | `@decorator` | None | Existing extraction in `decorators.rs` |
| TypeScript | `@decorator` | None | Existing extraction in `helpers.rs` |
| JavaScript | `@decorator` | None | Similar to TypeScript but separate tree-sitter grammar; extraction logic shared where possible |
| Java | `@Annotation` | None | Type-name collapse for qualified forms |
| C# | `[Attribute]` | `[X, Y]` multi-attribute | Strip `Attribute` suffix, expand lists |
| VB.NET | `<Attribute>` | `<X, Y>` multi-attribute | Same rules as C#, angle brackets |
| Rust | `#[attr]` | `derive(X, Y)` | Expand derive, keep others as-is |
| Kotlin | `@Annotation` | None | Extracted from modifier lists |
| Scala | `@annotation` | None | Java-like, from modifier lists |
| PHP | `#[Attribute]` | `#[X, Y]` multi-attribute | PHP 8+ native attributes |
| Swift | `@attribute` | None | Built-in attrs: `available`, `objc`, etc. |
| Dart | `@annotation` | None | `override`, `deprecated`, custom |
| C++ | `[[attr]]` | `[[X, Y]]` multi-attribute | C++11 attributes |
| PowerShell | `[Attribute()]` | None | .NET-style, strip `Attribute` suffix |
| GDScript | `@annotation` | None | `export`, `onready`, `tool` |
| Elixir | `@attr` | None | Module attributes: `moduledoc`, `spec`, `doc` |

**18 languages with no annotation syntax (no-op handling):**
C, Go, Lua, Zig, QML, R, SQL, HTML, CSS, Vue (inherits TS in script blocks), Regex, Bash, Markdown, JSON, TOML, YAML

### 5. Tantivy Search Integration

**Two new Tantivy fields** (mirroring the exact-plus-tokenized pattern from file mode):

- `annotations_exact`: multivalue, unstored, lowercased keyword field. Exact canonical match.
- `annotations_text`: multivalue, unstored, code-tokenized field. Lower boost, relaxed/partial matches.

Both fields are populated from the symbol's canonical annotation list during Tantivy projection.

**Query routing in definition search:**

- Query `@Test` -> strip `@`, search `annotations_exact` with strong boost. Symbol name/signature fields are NOT searched.
- Query `@GetMapping UserController` -> `@GetMapping` is an annotation `Must` clause on `annotations_exact`; `UserController` uses normal definition search on name/signature/content.
- Query `Test` (no prefix) -> normal definition search, annotation fields NOT searched. Hard separation.
- Pasted native syntax normalized at query time: `[Authorize]` -> `Authorize`, `#[tokio::test]` -> `tokio::test`, `@app.route("/x")` -> `app.route`.

**Scoring rules:**
- `@`-prefixed terms are filter-like `Must` clauses, not ambient relevance signals.
- `annotations_exact` gets a strong boost; `annotations_text` gets a weaker boost for relaxed fallback.
- Annotation matches do NOT feed into exact-name promotion (that stays name-only).
- In OR fallback, normal terms relax but annotation terms stay required.

**Schema compatibility:** Adding new fields triggers the existing compat-marker version bump and index recreation on mismatch.

### 6. Shared Normalization Helper Architecture

The normalization helper lives in `crates/julie-extractors/src/base/annotations.rs`.

**Extractor contract:**
1. Each extractor walks the AST to find annotation/decorator/attribute nodes.
2. Extractor collects raw text for each annotation (pre-normalization).
3. Extractor calls `normalize_annotations(&raw_texts, language)`.
4. Extractor receives `Vec<AnnotationMarker>` back.
5. Extractor sets `symbol.annotations` to the canonical names.
6. Raw texts are carried alongside for database storage.

**Language-specific rules inside the shared helper:**
- Carrier expansion: Rust `derive`, C# multi-attribute, C++ multi-attribute, PHP multi-attribute, VB.NET multi-attribute
- Type-name collapse: Java, Kotlin, Scala (rightmost name from qualified path)
- Suffix stripping: C#, VB.NET, PowerShell (`Attribute` suffix)
- Expression path preservation: Python, TypeScript, JavaScript, Elixir

The helper is a single function with a `match` on language for the language-specific rules. Common operations (wrapper stripping, arg removal, dedup) apply to all languages.

### 7. Test Detection Integration

`is_test_symbol()` in `test_detection.rs` already accepts `decorators: &[String]` and `attributes: &[String]`. After normalization:

- Both parameters receive canonical annotation names (not raw syntax).
- Per-language detection functions (`detect_java_kotlin`, `detect_csharp`, `detect_python`, etc.) can match against clean canonical names instead of parsing raw text.
- New detection coverage for languages that currently pass empty arrays (JavaScript, Scala, PHP, Swift, Dart, C++, VB.NET, PowerShell, GDScript, Elixir).
- Existing path-based and name-based fallback detection remains for languages without annotation syntax.

### 8. Migration Strategy

- **New junction table:** Created via schema migration in `initialize_schema()`. No data migration needed; the table starts empty.
- **Existing workspaces:** Reindexing populates the junction table. Julie can reindex on demand (`manage_workspace(operation="index", force=true)`).
- **Automatic population:** The catch-up indexing flow (session connect staleness check) will populate annotations for changed files. A full reindex populates all files.
- **Symbol write pattern change:** `INSERT OR REPLACE` on `symbols` must become `INSERT ... ON CONFLICT(id) DO UPDATE` to avoid cascading deletes of annotation rows. Alternatively, annotation writes follow symbol writes in the same transaction with explicit delete-reinsert.
- **Backward compatibility:** Old binaries that don't know about `symbol_annotations` will ignore it (table simply exists unused). The `annotations` field on `Symbol` defaults to empty via `serde(default)`.

### 9. Database Query Patterns

**Batch load annotations for a file's symbols:**
```sql
SELECT sa.symbol_id, sa.annotation, sa.raw_text
FROM symbol_annotations sa
WHERE sa.symbol_id IN (SELECT id FROM symbols WHERE file_path = ?)
ORDER BY sa.symbol_id, sa.ordinal
```

**Find all symbols with a specific annotation:**
```sql
SELECT s.*
FROM symbols s
JOIN symbol_annotations sa ON sa.symbol_id = s.id
WHERE sa.annotation = ?
```

**Count annotation usage across workspace:**
```sql
SELECT sa.annotation, COUNT(DISTINCT sa.symbol_id) as symbol_count
FROM symbol_annotations sa
GROUP BY sa.annotation
ORDER BY symbol_count DESC
```

**Lightweight symbol reads (no annotations):** Existing lightweight query paths remain unchanged. Annotations are loaded only when needed (full symbol reads, search result hydration, reporting).

### 10. Indexing Pipeline Changes

**Tantivy projection:** During `project_symbols_to_tantivy()`, load annotations from the junction table in batch (per-file, not per-symbol) and populate the two new Tantivy fields.

**Write order during indexing:**
1. Insert/upsert symbol row
2. Delete existing annotations for that symbol_id
3. Insert new annotation rows
4. (Steps 1-3 in a single transaction per file batch)

---

## Acceptance Criteria

### Normalization
- [ ] Shared helper normalizes all 16 annotation syntaxes correctly
- [ ] Carrier expansion produces separate rows for derive traits and multi-attribute lists
- [ ] Type-name collapse works for qualified Java/Kotlin/Scala annotations
- [ ] C#/VB.NET/PowerShell `Attribute` suffix stripped at storage time
- [ ] Deduplication removes repeated canonical values on one symbol
- [ ] Declaration order preserved via ordinal

### Storage
- [ ] Junction table created by schema migration
- [ ] Full extract-persist-load-serialize roundtrip for all 16 languages
- [ ] Symbol upserts don't cascade-delete annotation rows
- [ ] Batch load query for a file's annotations (no N+1)
- [ ] Reindexing populates annotations for existing workspaces

### Search
- [ ] `fast_search("@Test", search_target="definitions")` finds annotated symbols
- [ ] `fast_search("@app.route", search_target="definitions")` finds Python Flask handlers
- [ ] `fast_search("@GetMapping UserController", search_target="definitions")` finds annotated method on that class
- [ ] `fast_search("Test", search_target="definitions")` does NOT return annotation matches
- [ ] Normal definition and content search unaffected by annotation indexing
- [ ] Pasted native syntax (`[Authorize]`, `#[tokio::test]`) normalized at query time

### Test Detection
- [ ] `is_test_symbol()` receives canonical names for all 16 languages
- [ ] Java `@Test`, C# `[Fact]`, Python `@pytest.fixture` detected via canonical markers
- [ ] No regressions in existing test detection for any language
- [ ] Path-based fallback still works for languages without annotations

### Extractor Coverage
- [ ] All 16 annotation-bearing languages extract and normalize
- [ ] All 18 non-annotation languages have no-op handling (no errors, no phantom annotations)
- [ ] Existing SOURCE/CONTROL test fixtures updated for annotation extraction
- [ ] New fixtures added for languages gaining annotation extraction for the first time

---

## Files Affected

### New Files
- `crates/julie-extractors/src/base/annotations.rs` — shared normalization helper

### Modified Files (Extractors)
- `crates/julie-extractors/src/base/types.rs` — `Symbol.annotations` field, `SymbolOptions.annotations`
- `crates/julie-extractors/src/base/mod.rs` — re-export annotations module
- `crates/julie-extractors/src/python/decorators.rs` — call shared normalizer
- `crates/julie-extractors/src/python/functions.rs` — set `symbol.annotations`
- `crates/julie-extractors/src/python/types.rs` — set `symbol.annotations`
- `crates/julie-extractors/src/typescript/helpers.rs` — call shared normalizer
- `crates/julie-extractors/src/typescript/classes.rs` — set `symbol.annotations`
- `crates/julie-extractors/src/typescript/functions.rs` — set `symbol.annotations`
- `crates/julie-extractors/src/java/methods.rs` — persist canonical annotations
- `crates/julie-extractors/src/java/helpers.rs` — extract annotation text from modifiers
- `crates/julie-extractors/src/csharp/helpers.rs` — parse attribute lists
- `crates/julie-extractors/src/csharp/members.rs` — set `symbol.annotations`
- `crates/julie-extractors/src/rust/helpers.rs` — normalize attributes
- `crates/julie-extractors/src/rust/functions.rs` — set `symbol.annotations`
- `crates/julie-extractors/src/kotlin/helpers.rs` — separate annotations from modifiers
- `crates/julie-extractors/src/kotlin/functions.rs` — set `symbol.annotations`
- Plus: JavaScript, Scala, PHP, Swift, Dart, C++, VB.NET, PowerShell, GDScript, Elixir extractors
- `crates/julie-extractors/src/test_detection.rs` — receive canonical names

### Modified Files (Core)
- `src/database/schema.rs` — `symbol_annotations` table creation
- `src/database/symbols/storage.rs` — annotation write pattern
- `src/database/symbols/queries.rs` — annotation batch load
- `src/search/schema.rs` — `annotations_exact` and `annotations_text` fields
- `src/search/index.rs` — populate annotation fields during projection
- `src/search/query.rs` — `@` prefix routing in definition search
- `src/search/scoring.rs` — annotation boost rules

### Test Files
- `fixtures/` — SOURCE/CONTROL fixtures for all 16 languages
- `src/tests/` — unit tests for normalization, storage roundtrip, search integration
