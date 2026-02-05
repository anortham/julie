# Design: Replace FTS5 with Tantivy + CodeTokenizer

**Date:** 2026-02-04
**Status:** Draft
**Scope:** Replace Julie's FTS5 search layer with Tantivy + code-aware tokenizer from Razorback. Remove ORT/embeddings/HNSW.

---

## Problem Statement

Julie's search uses SQLite FTS5 with the `unicode61` tokenizer:

```sql
tokenize = "unicode61 separators '_::->.'",
```

This tokenizer was designed for natural language, not code. It cannot:
- Split CamelCase identifiers (`getUserData` is one opaque token)
- Split snake_case identifiers (relies on separator config, fragile)
- Preserve language-specific operators (`?.`, `??`, `=>`) as meaningful tokens
- Understand naming conventions across languages

To compensate, Julie has ~800 lines of workaround code:
- `query_preprocessor.rs` (~500 lines) routing around FTS5 limitations
- `sanitize_fts5_query()` (~100 lines) escaping characters FTS5 chokes on
- `query_expansion.rs` generating CamelCase/snake_case variants at query time
- FTS5 corruption recovery on every database open
- Trigger management during bulk operations

Meanwhile, the Razorback project (`~/Source/razorback`) built a Tantivy-based search with a custom CodeTokenizer that solves tokenization at the right layer (index time) and has per-language configuration. Julie should adopt this approach.

Additionally, the ORT/HNSW semantic embedding system adds significant complexity (build times, platform-specific GPU issues, dependency weight) for marginal search value. It should be removed.

---

## Architecture

### Current State

```
SQLite DB
├── symbols table (structured data)
├── symbols_fts (FTS5 virtual table)
├── files table (file content)
├── files_fts (FTS5 virtual table)
└── FTS5 triggers (sync)

vectors/ directory
├── HNSW index
└── ORT embeddings
```

### Target State

```
SQLite DB                    Tantivy Index
├── symbols table            ├── symbol documents
├── files table              ├── file content documents
├── identifiers table        └── CodeTokenizer (language-aware)
└── relations table

Per workspace:
.julie/indexes/{workspace_id}/
├── db/symbols.db           (existing, minus FTS5 tables)
└── tantivy/                (NEW, replaces vectors/)
    ├── meta.json
    └── segments/
```

**Principle:** SQLite owns structural/relational data. Tantivy owns search.

### What's Removed
- `src/embeddings/` module (ORT, HNSW, background embedding pipeline)
- `vectors/` directory per workspace
- FTS5 virtual tables (`symbols_fts`, `files_fts`)
- FTS5 triggers, rebuild logic, corruption recovery
- Query preprocessor FTS5 sanitization (~500 lines)
- Query expansion CamelCase/snake_case workarounds
- ORT, HNSW, and related crate dependencies

### What's Added
- `src/search/` module (Tantivy index management, CodeTokenizer, query building)
- `languages/*.toml` (30 per-language config files, embedded in binary)
- `tantivy = "0.22"` crate dependency

---

## New Module: `src/search/`

```
src/search/
├── mod.rs              # Public API: SearchIndex, SearchResult types
├── index.rs            # Tantivy index creation, open, commit, document ops
├── schema.rs           # Tantivy schema (symbol docs + file content docs)
├── tokenizer.rs        # CodeTokenizer (ported from Razorback, completed)
├── query.rs            # Query building: field boosting, filters, multi-term
├── language_config.rs  # Load/parse embedded language TOML configs
└── scoring.rs          # Post-search scoring: important_patterns, affix boost
```

### Tantivy Schema

Two document types in the same index, distinguished by `doc_type` field:

**Symbol documents** (from tree-sitter extraction):
```
doc_type:    "symbol"          (STRING, filter)
id:          "abc123"          (STRING, stored) — links to SQLite
name:        "UserService"     (TEXT, code tokenizer, stored) — boost 5.0x
signature:   "pub struct..."   (TEXT, code tokenizer, stored) — boost 3.0x
doc_comment: "Handles user..." (TEXT, code tokenizer, stored) — boost 2.0x
code_body:   "impl { ... }"   (TEXT, code tokenizer, NOT stored) — boost 1.0x
file_path:   "src/user.rs"    (STRING, filter + stored)
kind:        "class"           (STRING, filter + stored)
language:    "rust"            (STRING, filter + stored)
start_line:  42                (u64, stored)
```

**File content documents** (for grep-like search):
```
doc_type:    "file"            (STRING, filter)
id:          "file:src/main.rs" (STRING, stored)
file_path:   "src/main.rs"    (STRING, filter + stored)
content:     "fn main() {...}" (TEXT, code tokenizer, NOT stored)
language:    "rust"            (STRING, filter + stored)
```

### Public API

```rust
pub struct SearchIndex { /* tantivy::Index, IndexWriter, IndexReader */ }

impl SearchIndex {
    // Lifecycle
    pub fn open_or_create(path: &Path, configs: &[LanguageConfig]) -> Result<Self>;
    pub fn commit(&mut self) -> Result<()>;

    // Document management
    pub fn add_symbol(&mut self, doc: SymbolDocument) -> Result<()>;
    pub fn add_file_content(&mut self, doc: FileDocument) -> Result<()>;
    pub fn remove_by_file_path(&mut self, path: &str) -> Result<()>;

    // Search
    pub fn search_symbols(
        &self, query: &str, filters: SearchFilters, limit: usize
    ) -> Result<Vec<SymbolSearchResult>>;

    pub fn search_content(
        &self, query: &str, filters: SearchFilters, limit: usize
    ) -> Result<Vec<ContentSearchResult>>;
}

pub struct SearchFilters {
    pub language: Option<String>,
    pub file_pattern: Option<String>,
    pub kind: Option<String>,
}
```

---

## CodeTokenizer

Ported from Razorback (`crates/razorback-search/src/tokenizer.rs`) with unfinished features completed.

### Core Behavior (Working — Direct Port)

**CamelCase splitting:**
- `getUserData` → `["getuserdata", "get", "user", "data"]`
- `XMLParser` → `["xmlparser", "xml", "parser"]`

**snake_case splitting:**
- `user_service` → `["user_service", "user", "service"]`
- `MAX_BUFFER_SIZE` → `["max_buffer_size", "max", "buffer", "size"]`

**Preserved patterns (per-language):**
- `std::io::Read` → `["std", "::", "io", "::", "read"]`
- `obj?.method` → `["obj", "?.", "method"]`

### Completed Features (Finishing Razorback's TODO Items)

**1. `meaningful_affixes`**

At index time, emit affix-stripped variants as additional tokens:
- `is_valid` → also emits `"valid"` (strip `is_` prefix)
- `try_parse` → also emits `"parse"` (strip `try_` prefix)
- `process_mut` → also emits `"process"` (strip `_mut` suffix)

Searching `"valid"` now matches `is_valid`, `has_valid`, `check_valid`.

**2. `strip_prefixes/suffixes`**

At index time, emit stripped name variants:
- `IUserService` → also emits `"userservice"` (strip `I` prefix)
- `UserController` → also emits `"user"` (strip `Controller` suffix)
- `UserDto` → also emits `"user"` (strip `Dto` suffix)

Enables cross-convention search and reduces noise from naming patterns.

**3. `scoring.important_patterns`**

Post-search reranking (not tokenization). After Tantivy returns BM25-scored results:
```rust
for result in &mut results {
    if let Some(config) = configs.get(&result.language) {
        for pattern in &config.scoring.important_patterns {
            if result.signature.contains(pattern) {
                result.score *= 1.5; // Boost public API definitions
            }
        }
    }
}
results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
```

---

## Language Configuration

30 TOML config files, one per supported language. Embedded in binary via `include_str!`.

### Example: `languages/rust.toml`

```toml
[tokenizer]
preserve_patterns = ["::", "->", "=>", "?", "<>", "'", "#[", "#!["]
naming_styles = ["snake_case", "PascalCase", "SCREAMING_SNAKE_CASE"]
meaningful_affixes = ["try_", "into_", "as_", "from_", "to_", "is_", "has_", "_mut", "_ref", "_unchecked"]

[variants]
strip_prefixes = []
strip_suffixes = ["_impl", "_trait", "_fn"]

[scoring]
important_patterns = ["pub fn", "pub struct", "pub enum", "pub trait", "impl", "async fn"]
```

### Example: `languages/typescript.toml`

```toml
[tokenizer]
preserve_patterns = ["?.", "??", "=>", "<>", "...", "?.("]
naming_styles = ["camelCase", "PascalCase", "SCREAMING_SNAKE_CASE"]
meaningful_affixes = ["get", "set", "is", "has", "can", "should", "on", "handle", "Async"]

[variants]
strip_prefixes = ["I", "T", "E"]
strip_suffixes = ["Dto", "Model", "Service", "Controller", "Handler"]

[scoring]
important_patterns = ["export function", "export class", "export interface", "export type", "export const", "async function"]
```

### Loading Strategy

```rust
// At compile time
const RUST_CONFIG: &str = include_str!("../../languages/rust.toml");
const TYPESCRIPT_CONFIG: &str = include_str!("../../languages/typescript.toml");
// ... 28 more

// At startup
pub fn load_all_configs() -> HashMap<String, LanguageConfig> {
    let mut configs = HashMap::new();
    configs.insert("rust".into(), toml::from_str(RUST_CONFIG).unwrap());
    configs.insert("typescript".into(), toml::from_str(TYPESCRIPT_CONFIG).unwrap());
    // ...
    configs
}
```

**Single tokenizer instance** with union of all language patterns (same as Razorback):
- All `preserve_patterns` from all languages merged into one set
- Sorted by length descending (longer patterns match first)
- Registered once with Tantivy as the "code" tokenizer

---

## Query Building

### Field Boosting

For symbol search queries, build a BooleanQuery with per-field boosts:

```rust
for term in query_terms {
    subqueries.push((Occur::Should, boost(name_query(term), 5.0)));
    subqueries.push((Occur::Should, boost(sig_query(term), 3.0)));
    subqueries.push((Occur::Should, boost(doc_query(term), 2.0)));
    subqueries.push((Occur::Should, boost(body_query(term), 1.0)));
}

// Add filters as MUST clauses
if let Some(lang) = filters.language {
    subqueries.push((Occur::Must, term_query("language", lang)));
}
if let Some(kind) = filters.kind {
    subqueries.push((Occur::Must, term_query("kind", kind)));
}
```

### Query Preprocessing (Simplified)

With the CodeTokenizer handling CamelCase/snake_case at index time, query preprocessing becomes minimal:

```rust
pub fn preprocess_query(query: &str) -> Vec<String> {
    // 1. Trim and lowercase
    // 2. Split on whitespace for multi-term queries
    // 3. That's it — Tantivy's query parser + CodeTokenizer handle the rest
}
```

No more FTS5 escaping. No more query expansion variants. No more sanitization.

---

## Integration Changes

### Files to Remove
- `src/embeddings/` — entire module

### Files to Add
- `src/search/mod.rs`
- `src/search/index.rs`
- `src/search/schema.rs`
- `src/search/tokenizer.rs`
- `src/search/query.rs`
- `src/search/language_config.rs`
- `src/search/scoring.rs`
- `languages/*.toml` (30 files)

### Files to Modify

| File | Change |
|------|--------|
| `src/database/schema.rs` | Remove FTS5 table creation, triggers, rebuild functions |
| `src/database/mod.rs` | Remove `check_and_rebuild_fts5_indexes()` |
| `src/database/symbols/queries.rs` | Remove `find_symbols_by_pattern()`, `sanitize_fts5_query()` |
| `src/database/files.rs` | Remove `search_file_content_fts()` and FTS5 sanitization |
| `src/database/symbols/bulk.rs` | Remove FTS5 trigger dance, add Tantivy population |
| `src/database/bulk_operations.rs` | Remove FTS5 rebuild after bulk ops |
| `src/database/migrations.rs` | Add migration to drop FTS5 tables |
| `src/tools/search/text_search.rs` | Rewrite to call SearchIndex instead of FTS5 |
| `src/tools/search/query_preprocessor.rs` | Simplify dramatically (remove FTS5 sanitization) |
| `src/utils/query_expansion.rs` | Remove or drastically simplify |
| `src/workspace/` | Add SearchIndex alongside Database in workspace init |
| `Cargo.toml` | Add tantivy, remove ort + embedding crates |

### Database Migration

```rust
// Migration 007: Drop FTS5, transition to Tantivy
fn migrate_007_drop_fts5(&self) -> Result<()> {
    // Drop FTS5 virtual tables (data is re-derivable)
    self.conn.execute("DROP TABLE IF EXISTS symbols_fts", [])?;
    self.conn.execute("DROP TABLE IF EXISTS files_fts", [])?;

    // Drop FTS5 triggers
    for trigger in &["symbols_ai", "symbols_ad", "symbols_au",
                      "files_ai", "files_ad", "files_au"] {
        self.conn.execute(&format!("DROP TRIGGER IF EXISTS {trigger}"), [])?;
    }

    Ok(())
}
```

On next workspace indexing pass, Tantivy index is populated from SQLite data.

---

## Implementation Phases

### Phase 1: Add Tantivy Alongside FTS5 (Additive Only)

1. Create `src/search/` module: schema, tokenizer, index management
2. Port language configs from Razorback, embed with `include_str!`
3. Wire Tantivy index creation into workspace initialization
4. Populate Tantivy index during regular indexing (alongside existing FTS5)
5. **Tests:** Tokenizer unit tests, index build/query integration tests

**Exit criteria:** Tantivy index builds correctly alongside FTS5. Both work. No behavior change for users.

### Phase 2: Switch Search Queries to Tantivy

6. Rewrite `text_search_impl()` to query Tantivy
7. Simplify query preprocessor (remove FTS5 sanitization)
8. Simplify/remove query expansion
9. **Tests:** Search quality validation — ensure results match or exceed FTS5

**Exit criteria:** All search goes through Tantivy. FTS5 is populated but unused.

### Phase 3: Remove FTS5 and ORT

10. Remove FTS5 table creation, triggers, rebuild, integrity checks
11. Add database migration to drop FTS5 tables
12. Remove `src/embeddings/` entirely
13. Remove ORT/HNSW dependencies from `Cargo.toml`
14. Clean up bulk operations
15. **Tests:** Full test suite passes. Binary size reduced. Build time reduced.

**Exit criteria:** No FTS5 or ORT code remains. Clean codebase.

### Phase 4: Complete Language Config Features

16. Wire `meaningful_affixes` into CodeTokenizer
17. Wire `strip_prefixes/suffixes` into CodeTokenizer
18. Implement `scoring.important_patterns` post-search reranking
19. **Tests:** Affix matching, variant stripping, scoring boosts validated

**Exit criteria:** Full language config system operational. Search quality improved.

---

## Success Criteria

1. **Search quality:** Searching "user" finds `UserService`, `getUserData`, `user_service` (cross-convention matching)
2. **Performance:** Search latency ≤ current FTS5 (<5ms for typical queries)
3. **Binary size:** Reduced (ORT removal saves significant size)
4. **Build time:** Reduced (ORT/ONNX compilation is slow)
5. **Code simplification:** Net reduction of ~1000+ lines (FTS5 workarounds + embeddings)
6. **Zero FTS5 workarounds:** No query sanitization, no corruption recovery, no trigger management
7. **All existing tests pass** (adapted for new search backend)

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Tantivy index size larger than FTS5 | Monitor; Tantivy supports segment merging and compression |
| Tantivy startup time (opening index) | Lazy initialization; open reader on first search |
| Breaking search behavior users rely on | Phase 1 runs both engines in parallel for comparison |
| File content search quality regression | Test with Julie's own dogfood test suite |
| Large binary size increase from Tantivy | Measure; Tantivy is ~2-4MB compiled. ORT removal compensates |

---

## References

- Razorback source: `~/Source/razorback/crates/razorback-search/`
- Razorback language configs: `~/Source/razorback/languages/`
- Julie FTS5 schema: `src/database/schema.rs:107-282`
- Julie query expansion: `src/utils/query_expansion.rs`
- Julie query preprocessor: `src/tools/search/query_preprocessor.rs`
- Julie embeddings: `src/embeddings/`
