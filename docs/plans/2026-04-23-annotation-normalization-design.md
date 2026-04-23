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
  symbol_id      TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
  ordinal        INTEGER NOT NULL,
  annotation     TEXT NOT NULL,
  annotation_key TEXT NOT NULL,
  raw_text       TEXT,
  carrier        TEXT,
  PRIMARY KEY (symbol_id, ordinal)
);

CREATE INDEX idx_anno_key        ON symbol_annotations(annotation_key, symbol_id);
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
- `ordinal`: declaration order after carrier expansion. Preserves vector ordering semantics.
- `annotation`: canonical display value after syntax stripping and argument removal. Source case is preserved.
- `annotation_key`: canonical match key. This is case-folded, suffix-normalized, and used for config joins, exact search, and test detection.
- `raw_text`: original source fragment for the expanded row (not the whole carrier). Preserves arguments, qualified names, and syntax for debugging and UI/reporting.
- `carrier`: optional carrier that produced the row. For Rust `derive`, rows have `carrier = "derive"` and `annotation = "Debug"` / `"Clone"`.

**Write pattern:** Delete-and-reinsert within a transaction during indexing. The current `INSERT OR REPLACE` on `symbols` would cascade-delete child rows, so symbol upserts must use `INSERT ... ON CONFLICT(id) DO UPDATE` or the annotation write must follow the symbol write in the same transaction.

### 2. Symbol Struct Changes

Add a durable annotation marker field to `Symbol` in `crates/julie-extractors/src/base/types.rs`. The marker object must flow from extractor to database so `raw_text`, `annotation_key`, and `carrier` do not get lost between extraction and storage.

```rust
/// Canonical annotation marker with display, match, and source text forms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationMarker {
    pub annotation: String,
    pub annotation_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carrier: Option<String>,
}

/// Canonical annotation markers.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub annotations: Vec<AnnotationMarker>,
```

Add annotations to `SymbolOptions` so extractors can pass them through `create_symbol()`:

```rust
pub annotations: Vec<AnnotationMarker>,  // in SymbolOptions, default empty
```

The `metadata` field retains its current role for language-specific data. Annotations move out of metadata into the dedicated field. Existing `metadata["decorators"]` and `metadata["attributes"]` entries are removed from extractors as they are migrated. If a tool needs a lightweight list of names, derive it from `symbol.annotations.iter().map(|m| &m.annotation)` at the boundary.

### 3. Normalization Contract

All extractors call a shared normalization helper. The helper takes raw annotation text and the source language, and returns `Vec<AnnotationMarker>`.

```rust
// crates/julie-extractors/src/base/annotations.rs

pub struct AnnotationMarker {
    pub annotation: String,
    pub annotation_key: String,
    pub raw_text: Option<String>,
    pub carrier: Option<String>,
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
| Build display value | Preserve source case in `annotation` | `GetMapping` stays `GetMapping` |
| Build match key | Case-fold `annotation` and apply language suffix rules | `TestMethodAttribute` -> key `testmethod` |
| Deduplicate | Repeated identical match keys on one symbol stored once | Two `@override` -> one row |

**Carrier expansion detail:**
- Rust `#[derive(Debug, Clone)]` -> rows: `Debug` (key: `debug`, raw: `Debug`, carrier: `derive`), `Clone` (key: `clone`, raw: `Clone`, carrier: `derive`). No standalone `derive` row.
- C# `[Authorize, Route("api")]` -> rows: `Authorize` (key: `authorize`, raw: `Authorize`), `Route` (key: `route`, raw: `Route("api")`).
- Rust `#[cfg(test)]` -> single row: `cfg` (raw: `cfg(test)`). `cfg` is NOT a carrier; its argument is a predicate, not a list of annotations.
- Rust `#[serde(rename_all = "camelCase")]` -> single row: `serde` (raw: `serde(rename_all = "camelCase")`).

**`raw_text` after expansion:** Each expanded row gets its own fragment, not the whole carrier source. For `#[derive(Debug, Clone)]`, the `Debug` row's raw_text is `"Debug"`, not `"#[derive(Debug, Clone)]"`.

### 4. Language Coverage

**16 languages with annotation syntax (extraction required over the full rollout):**

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

**Rollout tiers:**
- Tier 1 builds the full contract, persistence, search, and test-detection API, then migrates languages with existing extraction paths: Python, TypeScript, Java, C#, Rust, Kotlin, Scala, Dart, PHP, VB.NET, GDScript, and Elixir.
- Tier 2 adds JavaScript decorator extraction, Swift declaration attributes, C++ standard attributes, and PowerShell command/parameter attributes with focused grammar tests.
- Languages with no annotation syntax get no-op handling in Tier 1 so shared code paths can rely on empty vectors.
- PowerShell must distinguish attributes from type brackets such as `[string]`; Swift must distinguish declaration attributes from type attributes; PHP namespace handling must use the display/key split instead of assuming one canonical spelling.

### 5. Tantivy Search Integration

**Two new Tantivy fields** (mirroring the exact-plus-tokenized pattern from file mode):

- `annotations_exact`: multivalue, unstored, lowercased keyword field populated from `annotation_key`.
- `annotations_text`: multivalue, unstored, code-tokenized field populated from display values and qualified raw call paths. Lower boost, relaxed/partial matches.
- `owner_names_text`: multivalue, unstored, code-tokenized field containing ancestor symbol names for methods and nested symbols.

Annotation fields are populated from the symbol's marker list during Tantivy projection. `owner_names_text` is populated by resolving `parent_id` chains in batch during projection.

**Query routing in definition search:**

- Query `@Test` -> strip `@`, search `annotations_exact` with strong boost. Symbol name/signature fields are NOT searched.
- Query `@GetMapping UserController` -> `@GetMapping` is an annotation `Must` clause on `annotations_exact`; `UserController` uses normal definition search plus `owner_names_text`, so a method annotated inside `UserController` can match.
- Query `Test` (no prefix) -> normal definition search, annotation fields NOT searched. Hard separation.
- Pasted native syntax normalized at query time: `[Authorize]` -> `Authorize`, `#[tokio::test]` -> `tokio::test`, `@app.route("/x")` -> `app.route`.
- Annotation context terms are tokenized from the raw remaining query text before lowercasing expansion. CamelCase owner terms such as `UserController` must match via atomic owner tokens (`user`, `controller`) without requiring the full compound token to be present.

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
5. Extractor passes the marker vector through `SymbolOptions`.
6. `create_symbol()` stores the marker vector on `Symbol`.
7. Database storage writes one row per marker. No raw-text bridge through metadata is required.

**Language-specific rules inside the shared helper:**
- Carrier expansion: Rust `derive`, C# multi-attribute, C++ multi-attribute, PHP multi-attribute, VB.NET multi-attribute
- Type-name collapse: Java, Kotlin, Scala (rightmost name from qualified path)
- Suffix stripping for match keys: C#, VB.NET, PowerShell (`Attribute` suffix)
- Expression path preservation: Python, TypeScript, JavaScript, Elixir

The helper is a single function with a `match` on language for the language-specific rules. Common operations (wrapper stripping, arg removal, dedup) apply to all languages.

### 7. Test Detection Integration

`is_test_symbol()` in `test_detection.rs` moves to one neutral annotation input:

```rust
pub fn is_test_symbol(
    language: &str,
    name: &str,
    file_path: &str,
    kind: &SymbolKind,
    annotation_keys: &[String],
    doc_comment: Option<&str>,
) -> bool;
```

- Per-language detection functions (`detect_java_kotlin`, `detect_csharp`, `detect_python`, etc.) match against normalized keys instead of parsing raw text or wrapper syntax.
- New detection coverage for languages that currently pass empty arrays (JavaScript, Scala, PHP, Swift, Dart, C++, VB.NET, PowerShell, GDScript, Elixir).
- Existing path-based and name-based fallback detection remains for languages without annotation syntax.

### 8. Migration Strategy

- **New junction table:** Created via schema migration in `initialize_schema()`. No data migration needed; the table starts empty.
- **Existing workspaces:** Reindexing populates the junction table. Julie can reindex on demand (`manage_workspace(operation="index", force=true)`).
- **Automatic population:** The catch-up indexing flow (session connect staleness check) will populate annotations for changed files. A full reindex populates all files.
- **Symbol write pattern change:** `INSERT OR REPLACE` on `symbols` must become `INSERT ... ON CONFLICT(id) DO UPDATE` to avoid cascading deletes of annotation rows. Alternatively, annotation writes follow symbol writes in the same transaction with explicit delete-reinsert.
- **Backward compatibility:** Old binaries that don't know about `symbol_annotations` will ignore it because the table is unused by old code. The `annotations` field on `Symbol` defaults to empty via `serde(default)`.

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
WHERE sa.annotation_key = ?
```

**Count annotation usage across workspace:**
```sql
SELECT sa.annotation, sa.annotation_key, COUNT(DISTINCT sa.symbol_id) as symbol_count
FROM symbol_annotations sa
GROUP BY sa.annotation, sa.annotation_key
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

**Search hydration:** Definition search currently converts Tantivy hits back into `Symbol` and then enriches from SQLite. Annotation hydration should happen in that enrichment path so tool output, `exclude_tests`, and search traces see the same marker data.

---

## Acceptance Criteria

### Normalization
- [ ] Shared helper normalizes all 16 annotation syntaxes correctly
- [ ] Carrier expansion produces separate rows for derive traits and multi-attribute lists
- [ ] Type-name collapse works for qualified Java/Kotlin/Scala annotations
- [ ] C#/VB.NET/PowerShell `Attribute` suffix stripped from `annotation_key`, while `annotation` preserves display spelling
- [ ] Deduplication removes repeated annotation keys on one symbol
- [ ] Declaration order preserved via ordinal
- [ ] Rust derive rows preserve `carrier = "derive"`

### Storage
- [ ] Junction table created by schema migration
- [ ] Full extract-persist-load-serialize roundtrip for Tier 1 languages
- [ ] Tier 2 languages have explicit no-op or pending extraction tests, not accidental false coverage
- [ ] Symbol upserts don't cascade-delete annotation rows
- [ ] Batch load query for a file's annotations (no N+1)
- [ ] Reindexing populates annotations for existing workspaces

### Search
- [ ] `fast_search("@Test", search_target="definitions")` finds annotated symbols
- [ ] `fast_search("@app.route", search_target="definitions")` finds Python Flask handlers
- [ ] `fast_search("@GetMapping UserController", search_target="definitions")` finds annotated methods whose ancestor owner is `UserController`
- [ ] `fast_search("Test", search_target="definitions")` does NOT return annotation matches
- [ ] Normal definition and content search unaffected by annotation indexing
- [ ] Pasted native syntax (`[Authorize]`, `#[tokio::test]`) normalized at query time

### Test Detection
- [ ] `is_test_symbol()` receives one annotation-key slice, not separate decorator and attribute buckets
- [ ] Java `@Test`, C# `[Fact]`, Python `@pytest.fixture` detected via canonical markers
- [ ] No regressions in existing test detection for any language
- [ ] Path-based fallback still works for languages without annotations

### Extractor Coverage
- [ ] Tier 1 annotation-bearing languages extract and normalize through the shared contract
- [ ] Tier 2 annotation-bearing languages have explicit pending extraction tests or no-op assertions, not phantom coverage
- [ ] All 18 non-annotation languages have no-op handling (no errors, no phantom annotations)
- [ ] Existing SOURCE/CONTROL test fixtures updated for annotation extraction
- [ ] New fixtures added for languages gaining annotation extraction for the first time

---

## Files Affected

### New Files
- `crates/julie-extractors/src/base/annotations.rs`: shared normalization helper

### Modified Files (Extractors)
- `crates/julie-extractors/src/base/types.rs`: `AnnotationMarker`, `Symbol.annotations`, `SymbolOptions.annotations`
- `crates/julie-extractors/src/base/creation_methods.rs`: copy `SymbolOptions.annotations` into `Symbol`
- `crates/julie-extractors/src/base/mod.rs`: re-export annotation types and helper module
- `crates/julie-extractors/src/python/decorators.rs`: call shared normalizer
- `crates/julie-extractors/src/python/functions.rs`: pass markers into `SymbolOptions`
- `crates/julie-extractors/src/python/types.rs`: pass markers into `SymbolOptions`
- `crates/julie-extractors/src/typescript/helpers.rs`: call shared normalizer
- `crates/julie-extractors/src/typescript/classes.rs`: pass markers into `SymbolOptions`
- `crates/julie-extractors/src/typescript/functions.rs`: pass markers into `SymbolOptions`
- `crates/julie-extractors/src/java/helpers.rs`: extract annotation text from modifiers
- `crates/julie-extractors/src/java/methods.rs`: persist canonical annotations
- `crates/julie-extractors/src/csharp/helpers.rs`: parse attribute lists
- `crates/julie-extractors/src/csharp/members.rs`: pass markers into `SymbolOptions`
- `crates/julie-extractors/src/rust/helpers.rs`: normalize attributes
- `crates/julie-extractors/src/rust/functions.rs`: pass markers into `SymbolOptions`
- `crates/julie-extractors/src/kotlin/helpers.rs`: separate annotations from modifiers
- `crates/julie-extractors/src/kotlin/declarations.rs`: pass markers into `SymbolOptions`
- Plus: JavaScript, Scala, PHP, Swift, Dart, C++, VB.NET, PowerShell, GDScript, Elixir extractors
- `crates/julie-extractors/src/test_detection.rs`: receive annotation keys

### Modified Files (Core)
- `src/database/schema.rs`: `symbol_annotations` table creation
- `src/database/migrations.rs`: schema migration for existing workspaces
- `src/database/helpers.rs`: hydrate marker vectors on full and lightweight symbol rows
- `src/database/symbols/storage.rs`: symbol upsert and annotation write pattern
- `src/database/symbols/search.rs`: full-symbol reads with annotation batch hydration
- `src/database/symbols/queries.rs`: file-symbol reads with annotation batch hydration
- `src/search/schema.rs`: `annotations_exact`, `annotations_text`, and `owner_names_text` fields
- `src/search/index.rs`: populate annotation and owner fields
- `src/search/query.rs`: `@` prefix routing in definition search
- `src/search/projection.rs`: pass annotation and owner data into Tantivy documents
- `src/tools/search/text_search.rs`: hydrate annotations during SQLite enrichment

### Test Files
- `crates/julie-extractors/src/tests/`: extractor and test-detection unit tests
- `src/tests/core/`: storage roundtrip tests
- `src/tests/tools/search/`: Tantivy and `fast_search` integration tests
