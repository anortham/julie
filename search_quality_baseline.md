# Search Quality Baseline

**Date:** 2025-11-08
**Julie Version:** v1.1.3 (before query expansion)
**Purpose:** Establish baseline search quality metrics for comparison after query expansion implementation

## Test Methodology

For each query, we measure:
- **Result Count:** Number of results returned
- **Precision:** Are the top 5 results relevant?
- **Ranking Quality:** Is the most relevant result first?
- **Variant Handling:** Does it find symbols with different naming conventions?

## Test Suite

### Category 1: Multi-Word Queries (Standard Search)

These should improve significantly with query expansion.

#### Test 1.1: "user auth controller"
- **Expected:** Find symbols/code related to user authentication controllers
- **Search Target:** definitions
- **Success Criteria:** Top result should be a controller/service handling user auth

#### Test 1.2: "error handling logic"
- **Expected:** Find error handling functions/modules
- **Search Target:** definitions
- **Success Criteria:** Results should be functions/classes that handle errors, not just comments mentioning "error"

#### Test 1.3: "process files optimized"
- **Expected:** Find `process_files_optimized` method or similar
- **Search Target:** definitions
- **Success Criteria:** Should find the actual method, not just docs mentioning those words

#### Test 1.4: "database connection pool"
- **Expected:** Find database connection pooling code
- **Search Target:** definitions
- **Success Criteria:** Relevant database connection handling code

### Category 2: Naming Convention Variants

Query expansion should help find symbols regardless of casing.

#### Test 2.1: "getUserData" (camelCase) searching for snake_case
- **Expected:** Should ideally find `get_user_data` if it exists
- **Search Target:** definitions
- **Baseline Expectation:** Probably won't find snake_case variant without expansion

#### Test 2.2: "process_files_optimized" (snake_case) exact match
- **Expected:** Find the method with this exact name
- **Search Target:** definitions
- **Baseline Expectation:** Should work (exact match)

#### Test 2.3: "ProcessFilesOptimized" (PascalCase) searching for snake_case
- **Expected:** Should ideally find `process_files_optimized`
- **Search Target:** definitions
- **Baseline Expectation:** Probably won't find without expansion

#### Test 2.4: "createAuthServiceLogin" (camelCase) searching for snake_case
- **Expected:** Should find `create_auth_service_login` if it exists
- **Search Target:** definitions
- **Baseline Expectation:** Probably won't find without expansion

### Category 3: Single-Word Queries

These should work well even without expansion.

#### Test 3.1: "SymbolDatabase"
- **Expected:** Find SymbolDatabase struct/class
- **Search Target:** definitions
- **Success Criteria:** Top result is SymbolDatabase definition

#### Test 3.2: "preprocess_query"
- **Expected:** Find preprocess_query function
- **Search Target:** definitions
- **Success Criteria:** Top result is the function definition

#### Test 3.3: "extract_symbols"
- **Expected:** Find extract_symbols functions across extractors
- **Search Target:** definitions
- **Success Criteria:** Multiple relevant extract_symbols functions

### Category 4: Edge Cases

#### Test 4.1: "nonexistent impossible function"
- **Expected:** No results or semantic fallback
- **Search Target:** definitions
- **Success Criteria:** Graceful handling (empty results or semantic suggestions)

#### Test 4.2: "a b c" (very short terms)
- **Expected:** May return many generic results
- **Search Target:** content
- **Success Criteria:** Doesn't crash, returns something sensible

#### Test 4.3: "" (empty query)
- **Expected:** Should reject or return error
- **Search Target:** definitions
- **Success Criteria:** Graceful error handling

### Category 5: Content Search (Grep-style)

These test full-text search in files.

#### Test 5.1: "SQLite FTS5" (in content)
- **Expected:** Find lines mentioning SQLite FTS5
- **Search Target:** content
- **Success Criteria:** Returns relevant code/comments about FTS5

#### Test 5.2: "query expansion" (in content)
- **Expected:** Find documentation/code mentioning query expansion
- **Search Target:** content
- **Success Criteria:** Returns relevant content

---

## Baseline Results

Results will be documented below after running the test suite.

### Test 1.1: "user auth controller"
```
Command: fast_search(query="user auth controller", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 1.2: "error handling logic"
```
Command: fast_search(query="error handling logic", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 1.3: "process files optimized"
```
Command: fast_search(query="process files optimized", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 1.4: "database connection pool"
```
Command: fast_search(query="database connection pool", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 2.1: "getUserData"
```
Command: fast_search(query="getUserData", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 2.2: "process_files_optimized"
```
Command: fast_search(query="process_files_optimized", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 2.3: "ProcessFilesOptimized"
```
Command: fast_search(query="ProcessFilesOptimized", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 2.4: "createAuthServiceLogin"
```
Command: fast_search(query="createAuthServiceLogin", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 3.1: "SymbolDatabase"
```
Command: fast_search(query="SymbolDatabase", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 3.2: "preprocess_query"
```
Command: fast_search(query="preprocess_query", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 3.3: "extract_symbols"
```
Command: fast_search(query="extract_symbols", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 4.1: "nonexistent impossible function"
```
Command: fast_search(query="nonexistent impossible function", search_method="text", limit=5, search_target="definitions")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 5.1: "SQLite FTS5"
```
Command: fast_search(query="SQLite FTS5", search_method="text", limit=5, search_target="content")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

### Test 5.2: "query expansion"
```
Command: fast_search(query="query expansion", search_method="text", limit=5, search_target="content")
Results: [TO BE FILLED]
Precision: [TO BE RATED]
Quality: [TO BE RATED]
```

---

## Quality Rating Scale

**Precision (Relevance of Results):**
- ★★★★★ Excellent - All top 5 results highly relevant
- ★★★★☆ Good - 4/5 results relevant
- ★★★☆☆ Fair - 3/5 results relevant
- ★★☆☆☆ Poor - 1-2/5 results relevant
- ★☆☆☆☆ Very Poor - 0/5 results relevant

**Ranking Quality:**
- ★★★★★ Excellent - Most relevant result is #1
- ★★★☆☆ Fair - Most relevant result in top 3
- ★☆☆☆☆ Poor - Most relevant result not in top 5

**Overall Search Quality:**
- Combine precision + ranking to get overall assessment
- Note specific failures or unexpected behavior
