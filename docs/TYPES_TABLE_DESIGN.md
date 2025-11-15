# Types Table Design - Phase 4

## Purpose
Store type information extracted from code for LSP-quality type intelligence.

## TypeInfo Rust Structure
```rust
pub struct TypeInfo {
    pub symbol_id: String,              // Which symbol this type belongs to
    pub resolved_type: String,          // e.g., "String", "Vec<User>", "Promise<Data>"
    pub generic_params: Option<Vec<String>>,  // e.g., ["T", "U"]
    pub constraints: Option<Vec<String>>,     // e.g., ["T: Clone", "U: Send"]
    pub is_inferred: bool,              // true if inferred, false if explicit
    pub language: String,               // "python", "java", "typescript", etc.
    pub metadata: Option<HashMap<String, serde_json::Value>>, // Language-specific data
}
```

## SQL Schema

```sql
CREATE TABLE IF NOT EXISTS types (
    -- Primary key: one type per symbol (1:1 relationship)
    symbol_id TEXT PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,

    -- Type information
    resolved_type TEXT NOT NULL,       -- "String", "Vec<User>", "Promise<Data>"
    generic_params TEXT,               -- JSON array: ["T", "U"] or NULL
    constraints TEXT,                  -- JSON array: ["T: Clone"] or NULL
    is_inferred INTEGER NOT NULL,      -- 0 = explicit, 1 = inferred

    -- Metadata
    language TEXT NOT NULL,            -- Programming language
    metadata TEXT,                     -- JSON object for language-specific data

    -- Infrastructure
    last_indexed INTEGER DEFAULT 0     -- Unix timestamp of last update
);

-- Essential indexes for fast queries
CREATE INDEX IF NOT EXISTS idx_types_language ON types(language);
CREATE INDEX IF NOT EXISTS idx_types_resolved ON types(resolved_type);
CREATE INDEX IF NOT EXISTS idx_types_inferred ON types(is_inferred);
```

## Design Decisions

1. **symbol_id as PRIMARY KEY**: Each symbol has at most one type annotation
2. **ON DELETE CASCADE**: When symbol is deleted, its type info is deleted
3. **JSON storage**: `generic_params`, `constraints`, and `metadata` stored as JSON TEXT
   - Allows flexible array/object storage
   - Queried infrequently, so JSON parsing overhead acceptable
4. **is_inferred as INTEGER**: SQLite boolean convention (0/1)
5. **Indexes**: Language (filter by lang), resolved_type (search by type), is_inferred (filter explicit vs inferred)

## Language Support (8 languages)
- Python: Type hints, inferred types
- Java: Generic types, type bounds
- C#: Generic types, constraints
- PHP: Type declarations
- Kotlin: Type inference, nullability
- Dart: Generic types
- Go: Type declarations
- C++: Template types

## Migration Number
**Migration 006**: Add types table for type intelligence

## Bulk Storage Pattern
Mirror `bulk_store_identifiers`:
1. Drop indexes
2. Bulk INSERT OR REPLACE in transaction
3. Recreate indexes
4. WAL checkpoint

## Testing Strategy
1. Test type storage for each of 8 languages
2. Test generic parameters parsing
3. Test constraints storage
4. Test CASCADE deletion with symbols
5. Test bulk operations performance
