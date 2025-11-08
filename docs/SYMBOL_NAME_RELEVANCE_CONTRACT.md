# Symbol Name Relevance Check - Contract

## Problem Statement

Query expansion tries multiple variants (exact, snake_case, camelCase, wildcards). The quality check currently only filters "code vs docs" but doesn't verify if the symbol NAME actually matches the query.

**Example Failure:**
- Query: "ProcessFilesOptimized"
- Variant 1: "ProcessFilesOptimized" (exact)
- Matches: `expand_query` function (because it has "ProcessFilesOptimized" in a comment)
- Quality check: Sees `content_type=None` (code) and returns early ❌
- Never tries: Variant 2 "process_files_optimized" which would find the actual function ❌

## Function Contract

### Signature
```rust
pub fn is_symbol_name_relevant(
    query: &str,        // Original user query
    symbol_name: &str,  // Name of the symbol found
    variant: &str       // The query variant that matched
) -> bool
```

### Purpose
Determine if a symbol's name is actually relevant to the search query, filtering out spurious matches where the query appears only in comments/docs.

### Input Constraints
- All parameters are non-empty strings
- `query`: Original user input (e.g., "ProcessFilesOptimized")
- `symbol_name`: Actual symbol name from search results (e.g., "expand_query" or "process_files_optimized")
- `variant`: The query variant that produced this match (e.g., "ProcessFilesOptimized" or "process_files_optimized")

### Return Value
- `true`: Symbol name is relevant (matches query intent)
- `false`: Symbol name is NOT relevant (spurious match via comments)

### Logic Requirements

1. **Normalize to snake_case**: Convert all inputs to snake_case for comparison
   - "ProcessFilesOptimized" → "process_files_optimized"
   - "expand_query" → "expand_query" (already snake_case)

2. **Check exact match**: If normalized symbol_name == normalized variant, return true
   - "process_files_optimized" == "process_files_optimized" ✅

3. **Check substring match**: If one contains the other, return true
   - Handles partial matches and method names like "UserService.getData"

4. **Otherwise**: Return false
   - "expand_query" vs "process_files_optimized" → No match ❌

### Test Cases

#### Test 1: Exact match (different casing)
```rust
assert!(is_symbol_name_relevant(
    "ProcessFilesOptimized",           // query
    "process_files_optimized",         // symbol_name
    "process_files_optimized"          // variant
));
```

#### Test 2: Spurious match via comment
```rust
assert!(!is_symbol_name_relevant(
    "ProcessFilesOptimized",           // query
    "expand_query",                    // symbol_name (wrong function!)
    "ProcessFilesOptimized"            // variant
));
```

#### Test 3: CamelCase query finds snake_case symbol
```rust
assert!(is_symbol_name_relevant(
    "createAuthServiceLogin",          // query
    "create_auth_service_login",       // symbol_name
    "create_auth_service_login"        // variant (snake_case)
));
```

#### Test 4: Method name partial match
```rust
assert!(is_symbol_name_relevant(
    "getUserData",                     // query
    "get_user_data",                   // symbol_name
    "get_user_data"                    // variant
));
```

#### Test 5: Reject unrelated symbols
```rust
assert!(!is_symbol_name_relevant(
    "getUserData",                     // query
    "create_user",                     // symbol_name (different!)
    "getUserData"                      // variant
));
```

### Integration Point

Location: `src/tools/search/text_search.rs` lines 164-174

Current code:
```rust
if count > 0 {
    // Check if we found "good" results (actual code, not just documentation)
    let has_code_symbols = symbols.iter().any(|s| s.content_type.is_none());

    if has_code_symbols {
        // ❌ TOO SIMPLISTIC - just checks code vs docs
        return Ok(symbols);
    }
}
```

New code:
```rust
if count > 0 {
    // Check if we found relevant code symbols (not just comment mentions)
    let has_relevant_code = symbols.iter().any(|s| {
        let is_code = s.content_type.is_none();
        let is_relevant = is_symbol_name_relevant(query, &s.name, variant);
        is_code && is_relevant
    });

    if has_relevant_code {
        // ✅ IMPROVED - checks both code vs docs AND name relevance
        return Ok(symbols);
    }
}
```

## Success Criteria

After implementation:
- ✅ "ProcessFilesOptimized" finds `process_files_optimized` function as first result
- ✅ "createAuthServiceLogin" finds `create_auth_service_login` function as first result (already works)
- ✅ Does NOT return `expand_query` for "ProcessFilesOptimized" query
- ✅ All existing tests still pass (no regressions)

---

**Status:** Contract Defined ✅
**Next Step:** Write failing test
